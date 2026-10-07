// Cartridge mappers: 0 (NROM), 1 (MMC1), 2 (UxROM), 3 (CNROM), 4 (MMC3).

pub struct Mapper {
    pub kind: u8,
    pub prg_rom: Vec<u8>,
    pub chr: Vec<u8>,
    pub chr_is_ram: bool,
    pub chr_ram: Vec<u8>,
    pub prg_ram: Vec<u8>,
    pub battery: bool,
    pub mirroring: u8,

    // mapper 1
    pub shift: u8,
    pub shift_count: u8,
    pub ctrl: u8,
    pub chr0: u8,
    pub chr1: u8,
    pub prg_reg: u8,

    // mapper 2
    pub ux_bank: u8,

    // mapper 3
    pub cn_bank: u8,

    // mapper 4
    pub mmc3_bank: u8,
    pub mmc3_ram: u8,
    pub irq_latch: u8,
    pub irq_counter: u8,
    pub irq_reload: bool,
    pub irq_inhibit: bool,
    pub irq_pending: bool,
}

impl Mapper {
    pub fn new() -> Self {
        Mapper {
            kind: 0,
            prg_rom: Vec::new(),
            chr: Vec::new(),
            chr_is_ram: false,
            chr_ram: Vec::new(),
            prg_ram: Vec::new(),
            battery: false,
            mirroring: 0,
            shift: 0,
            shift_count: 0,
            ctrl: 0x0C,
            chr0: 0,
            chr1: 0,
            prg_reg: 0,
            ux_bank: 0,
            cn_bank: 0,
            mmc3_bank: 0,
            mmc3_ram: 0,
            irq_latch: 0,
            irq_counter: 0,
            irq_reload: false,
            irq_inhibit: false,
            irq_pending: false,
        }
    }

    #[inline]
    pub fn n8k(&self) -> usize {
        if self.prg_rom.is_empty() {
            1
        } else {
            self.prg_rom.len() / 8192
        }
    }

    pub fn power_on(&mut self) {
        self.shift = 0;
        self.shift_count = 0;
        self.ctrl = 0x0C;
        self.chr0 = 0;
        self.chr1 = 0;
        self.prg_reg = 0;
        self.ux_bank = 0;
        self.cn_bank = 0;
        self.mmc3_bank = 0;
        self.mmc3_ram = 0;
        self.irq_latch = 0;
        self.irq_counter = 0;
        self.irq_reload = false;
        self.irq_inhibit = false;
        self.irq_pending = false;
    }

    #[inline]
    pub fn prg_read(&self, addr: u16) -> u8 {
        let n = self.n8k().max(1);
        let a = addr & 0x7FFF;
        let (b, off) = match self.kind {
            0 | 3 => {
                let banks16 = (n / 2).max(1);
                let bk = ((a / 0x4000) as usize) % banks16;
                (bk, a & 0x3FFF)
            }
            1 => {
                let mode = self.ctrl & 3;
                let bank = (self.prg_reg & 0x0F) as usize;
                if n < 2 {
                    (0, a & 0x3FFF)
                } else {
                    match mode {
                        0 | 1 => {
                            let bb = (bank & !1) % (n / 2).max(1);
                            if a < 0x4000 {
                                (bb, a & 0x3FFF)
                            } else {
                                ((bb + 1) % (n / 2).max(1), a & 0x3FFF)
                            }
                        }
                        2 => {
                            if a < 0x4000 {
                                (0, a & 0x3FFF)
                            } else {
                                ((bank / 2) % (n / 2).max(1), a & 0x3FFF)
                            }
                        }
                        _ => {
                            if a < 0x4000 {
                                ((n / 2).max(1) - 1, a & 0x3FFF)
                            } else {
                                ((bank / 2) % (n / 2).max(1), a & 0x3FFF)
                            }
                        }
                    }
                }
            }
            2 => {
                let banks16 = (n / 2).max(1);
                if a < 0x4000 {
                    ((self.ux_bank as usize) % banks16, a & 0x3FFF)
                } else {
                    (banks16 - 1, a & 0x3FFF)
                }
            }
            4 => {
                let mode = (self.mmc3_bank >> 6) & 3;
                let r = (self.mmc3_bank & 7) as usize;
                match mode {
                    0 => {
                        if a < 0x4000 {
                            ((r & !1) % n, a & 0x1FFF)
                        } else {
                            (((r | 1) % n) as usize, a & 0x1FFF)
                        }
                    }
                    1 => {
                        if a < 0x4000 {
                            ((r & !1) % n, a & 0x1FFF)
                        } else {
                            (n - 1, a & 0x1FFF)
                        }
                    }
                    _ => {
                        if a < 0x2000 {
                            (n - 2, a & 0x1FFF)
                        } else if a < 0x4000 {
                            (((r | 2) % n) as usize, a & 0x1FFF)
                        } else if a < 0x6000 {
                            (n - 1, a & 0x1FFF)
                        } else {
                            (((r | 1) % n) as usize, a & 0x1FFF)
                        }
                    }
                }
            }
            _ => {
                let bk = ((a / 0x4000) as usize) % (n / 2).max(1);
                (bk, a & 0x3FFF)
            }
        };
        let wide = !matches!(self.kind, 4);
        let idx = if wide {
            (b * 0x4000 + off as usize) % self.prg_rom.len().max(1)
        } else {
            (b * 0x2000 + (off & 0x1FFF) as usize) % self.prg_rom.len().max(1)
        };
        self.prg_rom[idx]
    }

    pub fn prg_write(&mut self, addr: u16, v: u8) {
        let a = addr & 0x7FFF;
        match self.kind {
            1 => self.mmc1_write(a, v),
            2 => {
                if a >= 0x4000 {
                    self.ux_bank = v & 0x0F;
                }
            }
            3 => {
                if a >= 0x4000 {
                    self.cn_bank = v & 3;
                }
            }
            4 => self.mmc3_write(a, v),
            _ => {}
        }
    }

    pub fn prg_ram_read(&self, addr: u16) -> u8 {
        let a = (addr & 0x1FFF) as usize;
        if self.prg_ram.is_empty() {
            0
        } else {
            self.prg_ram[a % self.prg_ram.len()]
        }
    }

    pub fn prg_ram_write(&mut self, addr: u16, v: u8) {
        match self.kind {
            1 => {
                if self.chr0 & 0x10 == 0 {
                    return;
                }
            }
            4 => {
                if self.mmc3_ram & 2 != 0 {
                    return;
                }
            }
            _ => {}
        }
        let a = (addr & 0x1FFF) as usize;
        if !self.prg_ram.is_empty() {
            let l = self.prg_ram.len();
            self.prg_ram[a % l] = v;
        }
    }

    fn mmc1_write(&mut self, a: u16, v: u8) {
        if self.ctrl & 0x10 != 0 && a >= 0xE000 {
            // serial reset mode: writes set the shift register only
            self.shift = (self.shift >> 1) | ((v & 1) << 4);
            self.shift_count += 1;
            if self.shift_count == 5 {
                self.shift_count = 0;
                self.ctrl = self.shift;
                self.shift = 0;
            }
            return;
        }
        if v & 0x80 != 0 {
            self.shift = 0;
            self.shift_count = 0;
            self.ctrl |= 0x0C;
            return;
        }
        self.shift |= (v & 1) << self.shift_count;
        self.shift_count += 1;
        if self.shift_count == 5 {
            self.shift_count = 0;
            let val = self.shift & 0x1F;
            self.shift = 0;
            match a {
                0x0000..=0x1FFF => self.ctrl = val,
                0x2000..=0x3FFF => self.chr0 = val,
                0x4000..=0x5FFF => self.chr1 = val,
                0x6000..=0x7FFF => self.prg_reg = val,
                _ => {}
            }
        }
    }

    fn mmc3_write(&mut self, a: u16, v: u8) {
        let r = (a >> 13) & 3;
        match r {
            0 => {
                self.mmc3_bank = v;
                if v & 0x80 != 0 {
                    self.irq_reload = true;
                }
            }
            1 => {
                self.mmc3_bank = (self.mmc3_bank & !7) | (v & 7);
                self.irq_latch = v;
            }
            2 => {
                self.mmc3_ram = v;
            }
            _ => {
                if a & 0x0001 == 0 {
                    self.irq_inhibit = false;
                    self.irq_reload = false;
                } else {
                    self.irq_counter = 0;
                    self.irq_pending = false;
                    self.irq_reload = true;
                    self.irq_inhibit = true;
                }
            }
        }
    }

    pub fn clock_irq(&mut self) {
        if self.kind != 4 {
            return;
        }
        if self.irq_counter == 0 || self.irq_reload {
            self.irq_counter = self.irq_latch;
            self.irq_reload = false;
        } else {
            self.irq_counter -= 1;
        }
        if self.irq_counter == 0 {
            self.irq_pending = true;
        }
    }

    /// Fill the PPU's 1 KiB CHR page table. Entry i is the 1 KiB page of CHR
    /// memory that PPU addresses $i*0x400..$i*0x400+$3FF map to.
    pub fn chr_banks(&self, out: &mut [u8; 8]) {
        let pages = (self.chr_ram_len() / 0x400).max(1) as u8;
        match self.kind {
            1 => {
                if self.ctrl & 0x10 == 0 {
                    // 8 KiB mode: chr0 selects the whole 8 KiB
                    let b = (self.chr0 & 0x1F) as u16;
                    for i in 0..8 {
                        out[i] = (b * 8 + i as u16).wrapping_rem(pages as u16) as u8;
                    }
                } else {
                    let b0 = (self.chr0 & 0x1F) as u16;
                    let b1 = (self.chr1 & 0x1F) as u16;
                    for i in 0..4 {
                        out[i] = (((b0 & 0x0F) * 4 + i as u16) % pages as u16) as u8;
                    }
                    for i in 0..4 {
                        out[4 + i] = (((b1 & 0x0F) * 4 + i as u16) % pages as u16) as u8;
                    }
                }
            }
            3 => {
                let b = (self.cn_bank & 3) as u16;
                for i in 0..8 {
                    out[i] = (b * 8 + i as u16).wrapping_rem(pages as u16) as u8;
                }
            }
            4 => {
                let r = (self.mmc3_bank & 7) as u16;
                if self.mmc3_bank & 0x80 == 0 {
                    out[0] = ((r & !1) * 2) as u8;
                    out[1] = out[0] + 1;
                    out[2] = ((r >> 1) & 1) as u8;
                    out[3] = out[2];
                    out[4] = ((r >> 2) & 1) as u8;
                    out[5] = out[4];
                    out[6] = ((r >> 3) & 1) as u8;
                    out[7] = out[6];
                } else {
                    out[0] = ((r >> 1) & 1) as u8;
                    out[1] = out[0];
                    out[2] = ((r >> 3) & 1) as u8;
                    out[3] = out[2];
                    out[4] = (r & !1) as u8;
                    out[5] = out[4];
                    out[6] = r as u8;
                    out[7] = out[6];
                }
            }
            _ => {
                for i in 0..8 {
                    out[i] = (i as u16).wrapping_rem(pages as u16) as u8;
                }
            }
        }
    }

    pub fn chr_ram_len(&self) -> usize {
        if self.chr.is_empty() {
            8192
        } else {
            self.chr.len()
        }
    }

    pub fn nt_bank(&self) -> u8 {
        match self.kind {
            4 => self.mmc3_ram & 1,
            _ => 0,
        }
    }
}