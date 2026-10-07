// NES APU (2A03): pulse 1/2, triangle, noise, DMC, frame sequencer.

pub const LENGTH_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22, 192, 24,
    72, 26, 16, 28, 32, 30,
];

pub const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 1, 0, 0, 0, 0, 0, 0],
    [0, 1, 1, 0, 0, 0, 0, 0],
    [0, 1, 1, 1, 1, 0, 0, 0],
    [1, 0, 0, 1, 1, 1, 1, 1],
];

pub const TRIANGLE_TABLE: [u8; 32] = [
    15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11,
    12, 13, 14, 15,
];

pub const NOISE_PERIODS: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
];

pub const DMC_RATES: [u16; 16] = [
    428, 380, 340, 320, 286, 254, 226, 214, 190, 160, 142, 128, 106, 84, 72, 54,
];

// Step lengths in CPU cycles for the frame sequencer.
const STEP_CPU: [u16; 4] = [7456, 7456, 7458, 7458];
const STEP_CPU_5: [u16; 5] = [7456, 7456, 7458, 7458, 7452];

#[derive(Clone, Copy, Default)]
struct Envelope {
    start: bool,
    divider: u8,
    decay: u8,
}

#[derive(Clone, Copy)]
pub struct Pulse {
    pub enabled: bool,
    pub duty: u8,
    seq: u8,
    pub timer: u16,
    pub period: u16,
    pub length: u8,
    halt: bool,
    pub const_vol: bool,
    pub volume: u8,
    env: Envelope,
    sweep_enabled: bool,
    sweep_period: u8,
    sweep_negate: bool,
    sweep_shift: u8,
    sweep_reload: bool,
    sweep_divider: u8,
    sweep_silence: bool,
    sweep_mute: bool,
}

impl Default for Pulse {
    fn default() -> Self {
        Pulse {
            enabled: false,
            duty: 0,
            seq: 0,
            timer: 0,
            period: 0,
            length: 0,
            halt: false,
            const_vol: false,
            volume: 0,
            env: Envelope::default(),
            sweep_enabled: false,
            sweep_period: 0,
            sweep_negate: false,
            sweep_shift: 0,
            sweep_reload: false,
            sweep_divider: 0,
            sweep_silence: false,
            sweep_mute: false,
        }
    }
}

impl Pulse {
    fn write_reg0(&mut self, v: u8) {
        self.duty = (v >> 6) & 3;
        self.halt = v & 0x20 != 0;
        self.const_vol = v & 0x10 != 0;
        self.volume = v & 0x0F;
        self.length = LENGTH_TABLE[((v >> 3) & 0x1F) as usize];
    }
    fn write_reg1(&mut self, v: u8) {
        self.sweep_enabled = v & 0x80 != 0;
        self.sweep_period = (v >> 4) & 7;
        self.sweep_negate = v & 8 != 0;
        self.sweep_shift = v & 7;
        self.sweep_reload = true;
    }
    fn write_reg2_lo(&mut self, v: u8) {
        self.period = (self.period & 0x700) | v as u16;
    }
    fn write_reg2_hi(&mut self, v: u8) {
        self.period = (self.period & 0xFF) | ((v & 7) as u16) << 8;
        if self.enabled {
            self.length = LENGTH_TABLE[((v >> 3) & 0x1F) as usize];
        }
        self.seq = 0;
        self.env.start = true;
    }
    fn clock_envelope(&mut self) {
        if self.env.start {
            self.env.start = false;
            self.env.decay = 15;
            self.env.divider = self.volume;
        } else if self.env.divider == 0 {
            self.env.divider = self.volume;
            if self.env.decay > 0 {
                self.env.decay -= 1;
            } else if self.halt {
                self.env.decay = 15;
            }
        } else {
            self.env.divider -= 1;
        }
    }
    fn target_period(&self) -> u16 {
        let change = self.period >> self.sweep_shift;
        if self.sweep_negate {
            self.period.wrapping_sub(change)
        } else {
            self.period.wrapping_add(change)
        }
    }
    fn clock_sweep(&mut self) {
        if self.sweep_divider == 0 && self.sweep_enabled && self.sweep_shift > 0 && !self.sweep_mute {
            self.period = self.target_period();
        }
        if self.sweep_divider == 0 || self.sweep_reload {
            self.sweep_divider = self.sweep_period;
            self.sweep_reload = false;
            self.sweep_mute = self.period < 8;
        } else {
            self.sweep_divider -= 1;
        }
        let t = self.target_period();
        self.sweep_silence = t > 0x7FF || (self.period < 8 && self.sweep_negate);
    }
    #[inline]
    fn output(&self) -> f64 {
        if self.length == 0 || self.sweep_silence {
            return 0.0;
        }
        if DUTY_TABLE[self.duty as usize][(self.seq & 7) as usize] == 0 {
            return 0.0;
        }
        if self.const_vol {
            self.volume as f64
        } else {
            self.env.decay as f64
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct Triangle {
    pub enabled: bool,
    seq: u8,
    pub timer: u16,
    pub period: u16,
    pub length: u8,
    halt: bool,
    linear: u8,
    linear_reload: u8,
    lin_counter: u8,
}

impl Triangle {
    fn write_reg3(&mut self, v: u8) {
        self.halt = v & 0x80 != 0;
        self.linear_reload = v & 0x7F;
    }
    fn write_reg4(&mut self, v: u8) {
        self.period = (self.period & 0x7FF) | ((v & 7) as u16) << 8;
        if self.enabled {
            self.length = LENGTH_TABLE[((v >> 3) & 0x1F) as usize];
        }
        self.lin_counter = self.linear_reload;
    }
    fn clock_linear(&mut self) {
        if self.lin_counter > 0 {
            self.lin_counter -= 1;
        }
        if !self.halt {
            self.lin_counter = self.linear_reload;
        }
    }
    fn clock_timer(&mut self) {
        if self.timer == 0 {
            self.timer = self.period;
            if self.period >= 2 && self.lin_counter > 0 && self.length > 0 {
                self.seq = (self.seq + 1) & 31;
            }
        } else {
            self.timer -= 1;
        }
    }
    #[inline]
    fn output(&self) -> f64 {
        if self.length > 0 && self.lin_counter > 0 {
            TRIANGLE_TABLE[self.seq as usize] as f64
        } else {
            0.0
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct Noise {
    pub enabled: bool,
    shift: u16,
    mode: bool,
    pub timer: u16,
    pub period: u16,
    pub length: u8,
    halt: bool,
    pub const_vol: bool,
    pub volume: u8,
    env: Envelope,
}

impl Noise {
    fn write_reg3(&mut self, v: u8) {
        self.halt = v & 0x20 != 0;
        self.const_vol = v & 0x10 != 0;
        self.volume = v & 0x0F;
    }
    fn write_reg4(&mut self, v: u8) {
        self.period = NOISE_PERIODS[(v & 0x0F) as usize];
        self.mode = v & 0x40 != 0;
        if self.enabled {
            self.length = LENGTH_TABLE[((v >> 3) & 0x1F) as usize];
        }
        self.env.start = true;
    }
    fn clock_envelope(&mut self) {
        if self.env.start {
            self.env.start = false;
            self.env.decay = 15;
            self.env.divider = self.volume;
        } else if self.env.divider == 0 {
            self.env.divider = self.volume;
            if self.env.decay > 0 {
                self.env.decay -= 1;
            } else if self.halt {
                self.env.decay = 15;
            }
        } else {
            self.env.divider -= 1;
        }
    }
    fn clock_timer(&mut self) {
        if self.timer == 0 {
            self.timer = self.period;
            let feedback = if self.mode { 6 } else { 1 };
            let bit = (self.shift & 1) ^ ((self.shift >> feedback) & 1);
            self.shift = (self.shift >> 1) | (bit << 14);
        } else {
            self.timer -= 1;
        }
    }
    #[inline]
    fn output(&self) -> f64 {
        if self.length == 0 || (self.shift & 1) != 0 {
            return 0.0;
        }
        if self.const_vol {
            self.volume as f64
        } else {
            self.env.decay as f64
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct Dmc {
    pub enabled: bool,
    irq_enabled: bool,
    loop_flag: bool,
    rate: u16,
    timer: u16,
    output: u8,
    memory_addr: u16,
    sample_length: u16,
    cur_addr: u16,
    bytes_left: u16,
    buffer: u8,
    filled: bool,
    shift: u8,
    bits_left: u8,
    silence: bool,
    irq_flag: bool,
    pub steal: u8,
}

impl Dmc {
    fn restart(&mut self) {
        self.cur_addr = self.memory_addr;
        self.bytes_left = self.sample_length;
    }
    #[inline]
    fn output(&self) -> f64 {
        if self.silence {
            0.0
        } else {
            self.output as f64
        }
    }
}

pub struct Apu {
    pub irq_line: bool,
    pub enabled: bool,
    pub frame_mode: bool,
    pub frame_irq_inhibit: bool,
    pub frame_irq: bool,
    frame_counter: u16,
    frame_step: u8,
    quarter_flag: bool,
    half_flag: bool,
    pub cycle_parity: bool,

    pub pulse: [Pulse; 2],
    pub triangle: Triangle,
    pub noise: Noise,
    pub dmc: Dmc,

    // audio output chain
    pub res_acc: f64,
    hp_prev: f64,
    hp_lp: f64,
    pub hp_coeff: f64,
    pub out_scale: f64,
    pub raw_last: f64,
}

impl Apu {
    pub fn new() -> Self {
        Apu {
            irq_line: false,
            enabled: true,
            frame_mode: false,
            frame_irq_inhibit: false,
            frame_irq: false,
            frame_counter: 0,
            frame_step: 0,
            quarter_flag: false,
            half_flag: false,
            cycle_parity: true,
            pulse: [Pulse::default(), Pulse::default()],
            triangle: Triangle::default(),
            noise: Noise {
                shift: 1,
                ..Default::default()
            },
            dmc: Dmc {
                shift: 1,
                ..Default::default()
            },
            res_acc: 0.0,
            hp_prev: 0.0,
            hp_lp: 0.0,
            hp_coeff: 0.0,
            out_scale: 1.0,
            raw_last: 0.0,
        }
    }

    #[inline]
    pub fn mixer(&self) -> f64 {
        let p1 = self.pulse[0].output();
        let p2 = self.pulse[1].output();
        let t = self.triangle.output();
        let n = self.noise.output();
        let d = self.dmc.output();
        let pulse_out = 95.88 / ((8128.0 / (p1 + p2 + 1.0)) + 100.0);
        let tnd_out = 159.79 / (1.0 / (t / 8227.0 + n / 12241.0 + d / 22638.0) + 100.0);
        pulse_out + tnd_out
    }

    /// Advance the APU by one CPU cycle.
    #[inline]
    pub fn tick(&mut self) {
        self.quarter_flag = false;
        self.half_flag = false;
        if self.enabled {
            self.frame_counter = self.frame_counter.wrapping_add(1);
            let limit = if self.frame_mode {
                STEP_CPU_5[self.frame_step as usize]
            } else {
                STEP_CPU[self.frame_step as usize]
            };
            if self.frame_counter >= limit {
                self.frame_counter = 0;
                self.frame_step += 1;
                let nsteps = if self.frame_mode { 5 } else { 4 };
                if self.frame_step >= nsteps {
                    if !self.frame_mode && !self.frame_irq_inhibit {
                        self.frame_irq = true;
                    }
                    self.frame_step = 0;
                }
                self.do_step_clocks();
            }
        }

        if self.quarter_flag {
            self.pulse[0].clock_envelope();
            self.pulse[1].clock_envelope();
            self.noise.clock_envelope();
            self.triangle.clock_linear();
        }

        self.cycle_parity = !self.cycle_parity;
        if self.cycle_parity {
            let p0 = &mut self.pulse[0];
            if p0.timer == 0 {
                p0.timer = p0.period;
                p0.seq = p0.seq.wrapping_add(1);
            } else {
                p0.timer -= 1;
            }
            let p1 = &mut self.pulse[1];
            if p1.timer == 0 {
                p1.timer = p1.period;
                p1.seq = p1.seq.wrapping_add(1);
            } else {
                p1.timer -= 1;
            }
            self.triangle.clock_timer();
            self.noise.clock_timer();

            if self.half_flag {
                for i in 0..2 {
                    let p = &mut self.pulse[i];
                    if p.length > 0 && !p.halt {
                        p.length -= 1;
                    }
                }
                if self.noise.length > 0 && !self.noise.halt {
                    self.noise.length -= 1;
                }
                if self.triangle.length > 0 && !self.triangle.halt {
                    self.triangle.length -= 1;
                }
                self.pulse[0].clock_sweep();
                self.pulse[1].clock_sweep();
            }

            self.clock_dmc();
        }

        let line = self.frame_irq || self.dmc.irq_flag;
        self.irq_line = line;
    }

    fn do_step_clocks(&mut self) {
        if !self.frame_mode {
            self.quarter_flag = true;
            self.half_flag = self.frame_step == 1 || self.frame_step == 3;
        } else {
            // step index 3 (the 4th step) gets no quarter frame clock
            self.quarter_flag = self.frame_step != 3;
            self.half_flag = self.frame_step == 1 || self.frame_step == 4;
        }
    }

    fn clock_dmc(&mut self) {
        let d = &mut self.dmc;
        if !self.enabled {
            return;
        }
        if d.timer == 0 {
            d.timer = d.rate;
            if !d.silence {
                d.bits_left = d.bits_left.wrapping_sub(1);
                if d.bits_left == 0 {
                    d.bits_left = 8;
                    if d.shift & 1 != 0 {
                        if d.output <= 125 {
                            d.output += 2;
                        }
                    } else if d.output >= 2 {
                        d.output -= 2;
                    }
                }
            }
            d.shift >>= 1;
        } else {
            d.timer -= 1;
        }
    }

    pub fn dmc_needs_fetch(&self) -> bool {
        self.enabled
            && self.dmc.bytes_left > 0
            && !self.dmc.filled
            && self.dmc.bits_left == 0
    }

    pub fn write(&mut self, addr: u16, v: u8) {
        match addr {
            0x4000..=0x4003 => {
                let p = &mut self.pulse[0];
                match addr & 3 {
                    0 => p.write_reg0(v),
                    1 => p.write_reg1(v),
                    2 => p.write_reg2_lo(v),
                    _ => p.write_reg2_hi(v),
                }
            }
            0x4004..=0x4007 => {
                let p = &mut self.pulse[1];
                match addr & 3 {
                    0 => p.write_reg0(v),
                    1 => p.write_reg1(v),
                    2 => p.write_reg2_lo(v),
                    _ => p.write_reg2_hi(v),
                }
            }
            0x4008 => self.triangle.write_reg3(v),
            0x400A => self.triangle.write_reg4(v),
            0x400B => self.noise.write_reg3(v),
            0x400C => self.noise.write_reg4(v),
            0x4010 => {
                self.dmc.irq_enabled = v & 0x80 != 0;
                if !self.dmc.irq_enabled {
                    self.dmc.irq_flag = false;
                }
                self.dmc.loop_flag = v & 0x40 != 0;
                self.dmc.rate = DMC_RATES[(v & 0x0F) as usize];
            }
            0x4011 => self.dmc.output = v,
            0x4012 => self.dmc.memory_addr = 0xC00 | (v as u16),
            0x4013 => self.dmc.sample_length = (v as u16) * 16 + 1,
            0x4015 => {
                self.pulse[0].enabled = v & 1 != 0;
                self.pulse[1].enabled = v & 2 != 0;
                self.triangle.enabled = v & 4 != 0;
                self.noise.enabled = v & 8 != 0;
                let d = v & 0x10 != 0;
                self.dmc.enabled = d;
                if !d {
                    self.dmc.bytes_left = 0;
                } else if self.dmc.bytes_left == 0 {
                    self.dmc.restart();
                }
                self.dmc.silence = true;
            }
            0x4017 => {
                self.frame_mode = v & 0x80 != 0;
                self.frame_irq_inhibit = v & 0x40 != 0;
                if self.frame_irq_inhibit {
                    self.frame_irq = false;
                }
                self.frame_counter = 0;
                self.frame_step = 0;
                if self.frame_mode {
                    self.quarter_flag = true;
                    self.half_flag = true;
                    self.pulse[0].clock_envelope();
                    self.pulse[1].clock_envelope();
                    self.noise.clock_envelope();
                    self.triangle.clock_linear();
                    for i in 0..2 {
                        let p = &mut self.pulse[i];
                        if p.length > 0 && !p.halt {
                            p.length -= 1;
                        }
                    }
                    if self.noise.length > 0 && !self.noise.halt {
                        self.noise.length -= 1;
                    }
                    if self.triangle.length > 0 && !self.triangle.halt {
                        self.triangle.length -= 1;
                    }
                    self.pulse[0].clock_sweep();
                    self.pulse[1].clock_sweep();
                }
            }
            _ => {}
        }
        self.irq_line = self.frame_irq || self.dmc.irq_flag;
    }

    pub fn read_4015(&mut self) -> u8 {
        let mut v = 0u8;
        if self.pulse[0].length > 0 {
            v |= 1;
        }
        if self.pulse[1].length > 0 {
            v |= 2;
        }
        if self.triangle.length > 0 {
            v |= 4;
        }
        if self.noise.length > 0 {
            v |= 8;
        }
        if self.dmc.bytes_left > 0 {
            v |= 0x10;
        }
        if self.frame_irq {
            v |= 0x40;
        }
        if self.dmc.irq_flag {
            v |= 0x80;
        }
        self.frame_irq = false;
        self.irq_line = self.dmc.irq_flag;
        v
    }

    pub fn reset_state(&mut self) {
        let par = self.cycle_parity;
        let coeff = self.hp_coeff;
        let scale = self.out_scale;
        let acc = self.res_acc;
        let mut n = Self::new();
        n.cycle_parity = par;
        n.hp_coeff = coeff;
        n.out_scale = scale;
        n.res_acc = acc;
        *self = n;
    }
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}