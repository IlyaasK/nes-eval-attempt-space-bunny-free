pub mod apu;
pub mod cpu;
pub mod mapper;
pub mod ppu;

use apu::Apu;
use mapper::Mapper;
use ppu::Ppu;

const CPU_CLOCK: f64 = 1789773.0;
const SAMPLE_RATE: f64 = 44100.0;

pub struct Nes {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub sp: u8,
    pub pc: u16,
    pub c: bool,
    pub z: bool,
    pub i: bool,
    pub d: bool,
    pub v: bool,
    pub n: bool,
    pub cycles: u64,

    pub ram: [u8; 2048],
    pub ppu: Ppu,
    pub apu: Apu,
    pub mapper: Mapper,
    pub chr: Vec<u8>,

    pub keys: u32,
    pub shift1: u8,
    pub strobe: bool,
    pub nmi_pending: bool,
    pub irq_line: bool,
    pub dma_pending: bool,
    pub dma_page: u8,
    pub open_bus: u8,

    pub audio: Vec<i16>,
    pub audio_all: Vec<i16>,
    pub audio_read: usize,
    pub res_acc: f64,
    pub hp_prev: f64,
    pub hp_lp: f64,
    pub trace: bool,
}

impl Nes {
    pub fn new() -> Self {
        Nes {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xFD,
            pc: 0,
            c: false,
            z: false,
            i: true,
            d: false,
            v: false,
            n: false,
            cycles: 0,
            ram: [0; 2048],
            ppu: Ppu::new(),
            apu: Apu::new(),
            mapper: Mapper::new(),
            chr: Vec::new(),
            keys: 0,
            shift1: 0,
            strobe: false,
            nmi_pending: false,
            irq_line: false,
            dma_pending: false,
            dma_page: 0,
            open_bus: 0,
            audio: Vec::new(),
            audio_all: Vec::new(),
            audio_read: 0,
            res_acc: 0.0,
            hp_prev: 0.0,
            hp_lp: 0.0,
            trace: false,
        }
    }

    pub fn load_rom(&mut self, data: &[u8]) -> bool {
        if data.len() < 16 {
            return false;
        }
        if &data[0..4] != b"NES\x1a" {
            return false;
        }
        let prg_banks = data[4] as usize;
        let chr_banks = data[5] as usize;
        let f6 = data[6];
        let f7 = data[7];
        let mut mapper_num = ((f6 >> 4) as usize) | (((f7 >> 4) & 0x0F) as usize) << 4;
        let mut mirroring = (f6 & 1) as u8;
        let battery = f6 & 2 != 0;
        let trainer = f6 & 4 != 0;
        let four_screen = f6 & 8 != 0;
        let nes2 = (f7 & 0x0C) == 0x08;
        let mut prg_ram_size: usize = 0;
        if nes2 {
            mapper_num = (((f7 & 0xF0) | (data[6] & 0xF0)) as usize) & 0xFF;
            mapper_num = (((data[6] >> 4) as usize) & 0x0F) | ((((data[7] & 0xF0) as usize) >> 4) << 4);
            prg_ram_size = ((((data[9] & 0x0F) as usize) << 8) | data[8] as usize) * 64;
            if f7 & 0x10 != 0 {
                // default expansion audio / battery
            }
            if f7 & 0x20 != 0 {
                // 4-screen VRAM
                mirroring = 2;
            }
            if f7 & 0x80 != 0 {
                // NES 2.0 mapper upper nibble comes from f7 high nibble
                mapper_num = ((f6 >> 4) as usize) | (((f7 >> 4) & 0x0F) as usize) << 4;
            }
        }
        let mut off = 16;
        if trainer {
            off += 512;
        }
        let prg_len = prg_banks * 16384;
        let chr_len = chr_banks * 8192;
        if data.len() < off + prg_len {
            return false;
        }
        let mapper = &mut self.mapper;
        mapper.kind = mapper_num as u8;
        mapper.prg_rom.clear();
        mapper.prg_rom.extend_from_slice(&data[off..off + prg_len]);
        mapper.battery = battery;

        self.chr.clear();
        let mut chr_writable: *mut u8 = core::ptr::null_mut();
        if chr_len == 0 {
            self.chr = vec![0u8; 8192];
            chr_writable = self.chr.as_mut_ptr();
        } else {
            self.chr.extend_from_slice(&data[off + prg_len..off + prg_len + chr_len]);
        }

        let mut pram = prg_ram_size;
        if mapper_num == 1 || mapper_num == 4 || battery {
            if pram < 8192 {
                pram = 8192;
            }
        }
        mapper.prg_ram = vec![0u8; pram];

        // mirroring
        if four_screen {
            self.ppu.mirroring = 2;
        } else if mapper_num == 1 {
            self.ppu.mirroring = mirroring;
        } else {
            self.ppu.mirroring = mirroring;
        }

        let chr_ptr = self.chr.as_ptr();
        let chr_len = self.chr.len();
        self.ppu.chr_ptr = chr_ptr;
        self.ppu.chr_len = chr_len;
        self.ppu.chr_writable = chr_writable;
        let chr_data: *mut u8 = self.chr.as_mut_ptr();
        let _ = chr_data;

        self.hard_reset();
        true
    }

    pub fn hard_reset(&mut self) {
        self.ram = [0; 2048];
        for b in self.mapper.prg_ram.iter_mut() {
            *b = 0;
        }
        self.mapper.power_on();
        self.ppu.reset_state();
        self.apu.reset_state();
        self.ppu.vram.iter_mut().for_each(|b| *b = 0);
        self.ppu.palette = [0; 32];
        self.ppu.oam = [0; 256];
        self.ppu.secondary_oam = [0; 32];
        self.ppu.scanline = 0;
        self.ppu.dot = 25;
        self.ppu.dc = 25;
        self.ppu.warmup = 29658;
        self.ppu.nt_bank = 0;
        self.ppu.chr_banks = [0; 8];
        if !self.chr.is_empty() && !self.ppu.chr_writable.is_null() {
            for b in self.chr.iter_mut() {
                *b = 0;
            }
        }
        self.update_banks();

        self.a = 0;
        self.x = 0;
        self.y = 0;
        self.sp = 0xFD;
        self.c = false;
        self.z = false;
        self.i = true;
        self.d = false;
        self.v = false;
        self.n = false;
        self.cycles = 0;
        let lo = self.mapper.prg_read(0xFFFC) as u16;
        let hi = self.mapper.prg_read(0xFFFD) as u16;
        self.pc = lo | (hi << 8);

        self.shift1 = 0;
        self.strobe = false;
        self.nmi_pending = false;
        self.irq_line = false;
        self.dma_pending = false;
        self.open_bus = 0;
        self.audio.clear();
        self.audio_read = 0;
        self.res_acc = 0.0;
        self.hp_prev = 0.0;
        self.hp_lp = 0.0;
        self.ppu.frame_count = 0;
        self.ppu.odd_frame = false;
    }

    pub fn update_banks(&mut self) {
        let mut b = [0u8; 8];
        self.mapper.chr_banks(&mut b);
        self.ppu.chr_banks = b;
        self.ppu.nt_bank = self.mapper.nt_bank();
    }

    #[inline]
    pub fn tick(&mut self) {
        self.cycles += 1;
        self.ppu.tick();
        self.ppu.tick();
        self.ppu.tick();
        if self.mapper.kind == 4 && self.ppu.dot == 260 && self.ppu.scanline <= 240 {
            self.mapper.clock_irq();
        }
        self.apu.tick();
        if self.ppu.warmup > 0 {
            self.ppu.warmup -= 1;
        }
        if self.ppu.nmi_output && !self.nmi_pending {
            self.nmi_pending = true;
            if self.trace { println!("  nmi_output set ctrl={:02X} latch={} cyc={}", self.ppu.ctrl, self.ppu.nmi_latch, self.cycles); }
        }
        self.irq_line = self.apu.irq_line || self.mapper.irq_pending;
        self.audio_tick();
    }

    #[inline]
    fn audio_tick(&mut self) {
        let raw = self.apu.mixer();
        if self.apu.hp_coeff < 0.0 {
            self.res_acc += SAMPLE_RATE / CPU_CLOCK;
            if self.res_acc >= 1.0 {
                self.res_acc -= 1.0;
                let v = (raw * self.apu.out_scale) as i32;
                let v = v.clamp(-32768, 32767);
                self.audio.push(v as i16);
                self.audio_all.push(v as i16);
            }
            return;
        }
        // high pass filter
        let c = self.apu.hp_coeff;
        let y = raw - self.hp_prev + c * self.hp_lp;
        self.hp_prev = raw;
        self.hp_lp = y;
        self.res_acc += SAMPLE_RATE / CPU_CLOCK;
        if self.res_acc >= 1.0 {
            self.res_acc -= 1.0;
            let s = y * self.apu.out_scale;
            let v = s as i32;
            let v = if v > 32767 {
                32767
            } else if v < -32768 {
                -32768
            } else {
                v
            };
            self.audio.push(v as i16);
            self.audio_all.push(v as i16);
        }
    }

    #[inline]
    pub fn read_raw(&mut self, a: u16) -> u8 {
        match a & 0xFFFF {
            0x0000..=0x1FFF => {
                let v = self.ram[(a & 0x7FF) as usize];
                self.open_bus = v;
                v
            }
            0x2000..=0x3FFF => self.ppu.cpu_read(a),
            0x4015 => {
                let v = self.apu.read_4015();
                self.open_bus = v;
                v
            }
            0x4016 => {
                let v = if self.strobe {
                    (self.keys & 1) as u8
                } else {
                    let b = self.shift1 & 1;
                    self.shift1 = (self.shift1 >> 1) | 0x80;
                    b
                };
                (v & 1) | (self.open_bus & 0xE0)
            }
            0x4017 => self.open_bus,
            0x4018..=0x401F => self.open_bus,
            0x6000..=0x7FFF => self.mapper.prg_ram_read(a),
            0x8000..=0xFFFF => self.mapper.prg_read(a),
            _ => self.open_bus,
        }
    }

    #[inline]
    pub fn write_raw(&mut self, a: u16, v: u8) {
        if self.trace {
            println!("W {:04X} <- {:02X}", a, v);
        }
        match a & 0xFFFF {
            0x0000..=0x1FFF => {
                self.ram[(a & 0x7FF) as usize] = v;
            }
            0x2000..=0x3FFF => {
                let reg = a & 7;
                if self.trace {
                    let names = ["ctrl","mask","status","oamaddr","oamdata","scroll","addr","data"];
                    println!("[{}] {:04X} <- {:02X} ({}) pc={:04X} warm={}", self.cycles, a, v, names[reg as usize], self.pc, self.ppu.warmup);
                }
                if self.ppu.warmup > 0 && (reg == 0 || reg == 1 || reg == 5 || reg == 6) {
                    return;
                }
                self.ppu.cpu_write(a, v);
            }
            0x4000..=0x4013 => {
                self.apu.write(a, v);
            }
            0x4014 => {
                self.dma_page = v;
                self.dma_pending = true;
            }
            0x4015 => {
                self.apu.write(a, v);
            }
            0x4016 => {
                self.open_bus = v;
                if v & 1 != 0 {
                    self.strobe = true;
                    self.shift1 = (self.keys & 0xFF) as u8;
                } else {
                    self.strobe = false;
                }
            }
            0x4017 => {
                self.open_bus = v;
                self.apu.write(a, v);
            }
            0x4018..=0x401F => {}
            0x4020..=0x5FFF => {}
            0x6000..=0x7FFF => self.mapper.prg_ram_write(a, v),
            0x8000..=0xFFFF => {
                self.mapper.prg_write(a, v);
                self.update_banks();
            }
            _ => {}
        }
    }

    #[inline]
    pub fn read(&mut self, a: u16) -> u8 {
        let v = self.read_raw(a);
        self.tick();
        v
    }

    #[inline]
    pub fn write(&mut self, a: u16, v: u8) {
        self.write_raw(a, v);
        self.tick();
    }

    pub fn set_audio_params(&mut self, scale: f64, hp: f64) {
        self.apu.out_scale = scale;
        self.apu.hp_coeff = hp;
    }

    pub fn run_frame(&mut self) {
        self.audio.clear();
        self.audio_read = 0;
        let start = self.ppu.frame_count;
        let mut guard: u64 = 0;
        while self.ppu.frame_count == start {
            self.step();
            guard += 1;
            if guard > 200000 {
                break;
            }
        }
    }
}

impl Default for Nes {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------- wasm ABI ----------------

static mut STATE: Option<Box<Nes>> = None;
static mut ROM: Vec<u8> = Vec::new();

#[inline]
fn nes() -> &'static mut Nes {
    unsafe { STATE.as_mut().unwrap() }
}

#[no_mangle]
pub extern "C" fn emu_init() -> i32 {
    unsafe {
        ROM = vec![0u8; 8 * 1024 * 1024];
        STATE = Some(Box::new(Nes::new()));
    }
    1
}

#[no_mangle]
pub extern "C" fn emu_rom_buffer() -> *mut u8 {
    unsafe { ROM.as_mut_ptr() }
}

#[no_mangle]
pub extern "C" fn emu_load_rom(len: i32) -> i32 {
    let l = len.max(0) as usize;
    let data: &[u8] = unsafe {
        let rom = &raw mut ROM;
        if l > (*rom).len() {
            return 0;
        }
        core::slice::from_raw_parts((*rom).as_ptr(), l)
    };
    let n = nes();
    if n.load_rom(data) {
        1
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn emu_reset() -> i32 {
    nes().hard_reset();
    1
}

#[no_mangle]
pub extern "C" fn emu_set_keys(k: u32) {
    let n = nes();
    n.keys = k;
}

#[no_mangle]
pub extern "C" fn emu_run_frame() {
    nes().run_frame();
}

#[no_mangle]
pub extern "C" fn emu_framebuffer() -> *const u16 {
    nes().ppu.framebuffer.as_ptr()
}

#[no_mangle]
pub extern "C" fn emu_audio_buffer() -> *const i16 {
    let n = nes();
    unsafe { n.audio.as_ptr().add(n.audio_read) }
}

#[no_mangle]
pub extern "C" fn emu_audio_samples() -> i32 {
    let n = nes();
    let c = n.audio.len() - n.audio_read;
    n.audio_read = n.audio.len();
    c as i32
}

#[no_mangle]
pub extern "C" fn emu_audio_rate() -> i32 {
    44100
}