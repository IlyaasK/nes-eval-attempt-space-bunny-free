// 6502 CPU (2A03 without decimal mode) - cycle accurate, on impl Nes.

use crate::Nes;

impl Nes {
    #[inline]
    pub fn get_p(&self, b: bool) -> u8 {
        (self.c as u8)
            | ((self.z as u8) << 1)
            | ((self.i as u8) << 2)
            | ((self.d as u8) << 3)
            | ((b as u8) << 4)
            | 0x20
            | ((self.v as u8) << 6)
            | ((self.n as u8) << 7)
    }

    #[inline]
    pub fn set_p(&mut self, v: u8) {
        self.c = v & 1 != 0;
        self.z = v & 2 != 0;
        self.i = v & 4 != 0;
        self.d = v & 8 != 0;
        self.v = v & 0x40 != 0;
        self.n = v & 0x80 != 0;
    }

    #[inline]
    fn set_zn(&mut self, v: u8) {
        self.z = v == 0;
        self.n = v & 0x80 != 0;
    }

    #[inline]
    fn fetch(&mut self) -> u8 {
        let v = self.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        v
    }

    #[inline]
    fn push(&mut self, v: u8) {
        let a = 0x100 | self.sp as u16;
        self.write(a, v);
        self.sp = self.sp.wrapping_sub(1);
    }

    #[inline]
    fn pull(&mut self) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        let a = 0x100 | self.sp as u16;
        self.read(a)
    }

    fn addr_zp(&mut self) -> u16 {
        self.fetch() as u16
    }

    fn addr_zp_x(&mut self) -> u16 {
        let b = self.fetch();
        self.tick();
        (b.wrapping_add(self.x)) as u16
    }

    fn addr_zp_y(&mut self) -> u16 {
        let b = self.fetch();
        self.tick();
        (b.wrapping_add(self.y)) as u16
    }

    fn addr_abs(&mut self) -> u16 {
        let lo = self.fetch() as u16;
        let hi = self.fetch() as u16;
        lo | (hi << 8)
    }

    fn addr_abs_x(&mut self, rmw: bool) -> u16 {
        let base = self.addr_abs();
        let a = base.wrapping_add(self.x as u16);
        if (a & 0xFF00) != (base & 0xFF00) || rmw {
            if rmw {
                // dummy write to the un-fixed address
                let cur = self.read_raw(a);
                self.write(a, cur);
            } else {
                self.tick();
            }
        }
        a
    }

    fn addr_abs_y(&mut self, rmw: bool) -> u16 {
        let base = self.addr_abs();
        let a = base.wrapping_add(self.y as u16);
        if (a & 0xFF00) != (base & 0xFF00) || rmw {
            if rmw {
                let cur = self.read_raw(a);
                self.write(a, cur);
            } else {
                self.tick();
            }
        }
        a
    }

    fn addr_ind_x(&mut self) -> u16 {
        let b = self.fetch();
        self.tick();
        let p = (b.wrapping_add(self.x) & 0xFF) as u16;
        let lo = self.read(p) as u16;
        let hi = self.read((p + 1) & 0xFF) as u16;
        lo | (hi << 8)
    }

    fn addr_ind_y(&mut self, rmw: bool) -> u16 {
        let b = self.fetch();
        self.tick();
        let p = b as u16;
        let lo = self.read(p) as u16;
        let hi = self.read((p + 1) & 0xFF) as u16;
        let base = lo | (hi << 8);
        let a = base.wrapping_add(self.y as u16);
        if (a & 0xFF00) != (base & 0xFF00) || rmw {
            if rmw {
                let cur = self.read_raw(a);
                self.write(a, cur);
            } else {
                self.tick();
            }
        }
        a
    }

    fn rmw(&mut self, addr: u16, f: impl Fn(&mut Nes, u8) -> u8) {
        let v = self.read(addr);
        self.write(addr, v);
        let r = f(self, v);
        self.write(addr, r);
    }

    fn branch(&mut self, cond: bool) {
        let off = self.fetch() as i8;
        if cond {
            self.tick();
            let target = (self.pc as i16).wrapping_add(off as i16) as u16;
            if (target & 0xFF00) != (self.pc & 0xFF00) {
                self.tick();
            }
            self.pc = target;
        }
    }

    pub fn step(&mut self) {
        self.ppu.last_pc = self.pc;
        if self.dma_pending {
            self.do_dma();
            return;
        }
        if self.nmi_pending {
            self.nmi_pending = false;
            self.ppu.clear_nmi_latch();
            self.interrupt_nmi();
            return;
        }
        if !self.i && self.irq_line {
            self.interrupt_irq();
            return;
        }
        let op = self.fetch();
        if self.trace {
            println!("pc={:04X} op={:02X} a={:02X} x={:02X} y={:02X} sp={:02X} p={:02X} cyc={} sl={} dot={}", self.pc.wrapping_sub(1), op, self.a, self.x, self.y, self.sp, self.get_p(false), self.cycles, self.ppu.scanline, self.ppu.dot);
        }
        match op {
            // ---- LDA ----
            0xA9 => {
                let v = self.fetch();
                self.a = v;
                self.set_zn(v);
            }
            0xA5 => {
                let a = self.addr_zp();
                let v = self.read(a);
                self.a = v;
                self.set_zn(v);
            }
            0xB5 => {
                let a = self.addr_zp_x();
                let v = self.read(a);
                self.a = v;
                self.set_zn(v);
            }
            0xAD => {
                let a = self.addr_abs();
                let v = self.read(a);
                self.a = v;
                self.set_zn(v);
            }
            0xBD => {
                let a = self.addr_abs_x(false);
                let v = self.read(a);
                self.a = v;
                self.set_zn(v);
            }
            0xB9 => {
                let a = self.addr_abs_y(false);
                let v = self.read(a);
                self.a = v;
                self.set_zn(v);
            }
            0xA1 => {
                let a = self.addr_ind_x();
                let v = self.read(a);
                self.a = v;
                self.set_zn(v);
            }
            0xB1 => {
                let a = self.addr_ind_y(false);
                let v = self.read(a);
                self.a = v;
                self.set_zn(v);
            }
            // ---- LDX ----
            0xA2 => {
                let v = self.fetch();
                self.x = v;
                self.set_zn(v);
            }
            0xA6 => {
                let a = self.addr_zp();
                let v = self.read(a);
                self.x = v;
                self.set_zn(v);
            }
            0xB6 => {
                let a = self.addr_zp_y();
                let v = self.read(a);
                self.x = v;
                self.set_zn(v);
            }
            0xAE => {
                let a = self.addr_abs();
                let v = self.read(a);
                self.x = v;
                self.set_zn(v);
            }
            0xBE => {
                let a = self.addr_abs_y(false);
                let v = self.read(a);
                self.x = v;
                self.set_zn(v);
            }
            // ---- LDY ----
            0xA0 => {
                let v = self.fetch();
                self.y = v;
                self.set_zn(v);
            }
            0xA4 => {
                let a = self.addr_zp();
                let v = self.read(a);
                self.y = v;
                self.set_zn(v);
            }
            0xB4 => {
                let a = self.addr_zp_x();
                let v = self.read(a);
                self.y = v;
                self.set_zn(v);
            }
            0xAC => {
                let a = self.addr_abs();
                let v = self.read(a);
                self.y = v;
                self.set_zn(v);
            }
            0xBC => {
                let a = self.addr_abs_x(false);
                let v = self.read(a);
                self.y = v;
                self.set_zn(v);
            }
            // ---- STA ----
            0x85 => {
                let a = self.addr_zp();
                self.write(a, self.a);
            }
            0x95 => {
                let a = self.addr_zp_x();
                self.write(a, self.a);
            }
            0x8D => {
                let a = self.addr_abs();
                self.write(a, self.a);
            }
            0x9D => {
                let a = self.addr_abs_x(false);
                self.write(a, self.a);
            }
            0x99 => {
                let a = self.addr_abs_y(false);
                self.write(a, self.a);
            }
            0x81 => {
                let a = self.addr_ind_x();
                self.write(a, self.a);
            }
            0x91 => {
                let a = self.addr_ind_y(false);
                self.write(a, self.a);
            }
            // ---- STX ----
            0x86 => {
                let a = self.addr_zp();
                self.write(a, self.x);
            }
            0x96 => {
                let a = self.addr_zp_y();
                self.write(a, self.x);
            }
            0x8E => {
                let a = self.addr_abs();
                self.write(a, self.x);
            }
            // ---- STY ----
            0x84 => {
                let a = self.addr_zp();
                self.write(a, self.y);
            }
            0x94 => {
                let a = self.addr_zp_x();
                self.write(a, self.y);
            }
            0x8C => {
                let a = self.addr_abs();
                self.write(a, self.y);
            }
            // ---- transfers ----
            0xAA => {
                self.x = self.a;
                self.set_zn(self.x);
                self.tick();
            }
            0xA8 => {
                self.y = self.a;
                self.set_zn(self.y);
                self.tick();
            }
            0xBA => {
                self.x = self.sp;
                self.set_zn(self.x);
                self.tick();
            }
            0x8A => {
                self.a = self.x;
                self.set_zn(self.a);
                self.tick();
            }
            0x9A => {
                self.sp = self.x;
                self.tick();
            }
            0x98 => {
                self.a = self.y;
                self.set_zn(self.a);
                self.tick();
            }
            // ---- stack ----
            0x48 => {
                let a = 0x100 | self.sp as u16;
                self.tick();
                self.write(a, self.a);
                self.sp = self.sp.wrapping_sub(1);
            }
            0x08 => {
                let a = 0x100 | self.sp as u16;
                self.tick();
                let p = self.get_p(true);
                self.write(a, p);
                self.sp = self.sp.wrapping_sub(1);
            }
            0x68 => {
                self.tick();
                self.tick();
                self.sp = self.sp.wrapping_add(1);
                let a = 0x100 | self.sp as u16;
                let v = self.read(a);
                self.a = v;
                self.set_zn(v);
            }
            0x28 => {
                self.tick();
                self.tick();
                self.sp = self.sp.wrapping_add(1);
                let a = 0x100 | self.sp as u16;
                let v = self.read(a);
                self.set_p(v);
            }
            // ---- logic ----
            0x29 => {
                let v = self.fetch();
                self.a &= v;
                self.set_zn(self.a);
            }
            0x25 => {
                let a = self.addr_zp();
                let v = self.read(a);
                self.a &= v;
                self.set_zn(self.a);
            }
            0x35 => {
                let a = self.addr_zp_x();
                let v = self.read(a);
                self.a &= v;
                self.set_zn(self.a);
            }
            0x2D => {
                let a = self.addr_abs();
                let v = self.read(a);
                self.a &= v;
                self.set_zn(self.a);
            }
            0x3D => {
                let a = self.addr_abs_x(false);
                let v = self.read(a);
                self.a &= v;
                self.set_zn(self.a);
            }
            0x39 => {
                let a = self.addr_abs_y(false);
                let v = self.read(a);
                self.a &= v;
                self.set_zn(self.a);
            }
            0x21 => {
                let a = self.addr_ind_x();
                let v = self.read(a);
                self.a &= v;
                self.set_zn(self.a);
            }
            0x31 => {
                let a = self.addr_ind_y(false);
                let v = self.read(a);
                self.a &= v;
                self.set_zn(self.a);
            }
            0x09 => {
                let v = self.fetch();
                self.a |= v;
                self.set_zn(self.a);
            }
            0x05 => {
                let a = self.addr_zp();
                let v = self.read(a);
                self.a |= v;
                self.set_zn(self.a);
            }
            0x15 => {
                let a = self.addr_zp_x();
                let v = self.read(a);
                self.a |= v;
                self.set_zn(self.a);
            }
            0x0D => {
                let a = self.addr_abs();
                let v = self.read(a);
                self.a |= v;
                self.set_zn(self.a);
            }
            0x1D => {
                let a = self.addr_abs_x(false);
                let v = self.read(a);
                self.a |= v;
                self.set_zn(self.a);
            }
            0x19 => {
                let a = self.addr_abs_y(false);
                let v = self.read(a);
                self.a |= v;
                self.set_zn(self.a);
            }
            0x01 => {
                let a = self.addr_ind_x();
                let v = self.read(a);
                self.a |= v;
                self.set_zn(self.a);
            }
            0x11 => {
                let a = self.addr_ind_y(false);
                let v = self.read(a);
                self.a |= v;
                self.set_zn(self.a);
            }
            0x49 => {
                let v = self.fetch();
                self.a ^= v;
                self.set_zn(self.a);
            }
            0x45 => {
                let a = self.addr_zp();
                let v = self.read(a);
                self.a ^= v;
                self.set_zn(self.a);
            }
            0x55 => {
                let a = self.addr_zp_x();
                let v = self.read(a);
                self.a ^= v;
                self.set_zn(self.a);
            }
            0x4D => {
                let a = self.addr_abs();
                let v = self.read(a);
                self.a ^= v;
                self.set_zn(self.a);
            }
            0x5D => {
                let a = self.addr_abs_x(false);
                let v = self.read(a);
                self.a ^= v;
                self.set_zn(self.a);
            }
            0x59 => {
                let a = self.addr_abs_y(false);
                let v = self.read(a);
                self.a ^= v;
                self.set_zn(self.a);
            }
            0x41 => {
                let a = self.addr_ind_x();
                let v = self.read(a);
                self.a ^= v;
                self.set_zn(self.a);
            }
            0x51 => {
                let a = self.addr_ind_y(false);
                let v = self.read(a);
                self.a ^= v;
                self.set_zn(self.a);
            }
            // ---- bit ----
            0x24 => {
                let a = self.addr_zp();
                let v = self.read(a);
                self.z = (self.a & v) == 0;
                self.n = v & 0x80 != 0;
                self.v = v & 0x40 != 0;
            }
            0x2C => {
                let a = self.addr_abs();
                let v = self.read(a);
                self.z = (self.a & v) == 0;
                self.n = v & 0x80 != 0;
                self.v = v & 0x40 != 0;
            }
            // ---- arithmetic ----
            0x69 => {
                let v = self.fetch();
                self.adc(v);
            }
            0x65 => {
                let a = self.addr_zp();
                let v = self.read(a);
                self.adc(v);
            }
            0x75 => {
                let a = self.addr_zp_x();
                let v = self.read(a);
                self.adc(v);
            }
            0x6D => {
                let a = self.addr_abs();
                let v = self.read(a);
                self.adc(v);
            }
            0x7D => {
                let a = self.addr_abs_x(false);
                let v = self.read(a);
                self.adc(v);
            }
            0x79 => {
                let a = self.addr_abs_y(false);
                let v = self.read(a);
                self.adc(v);
            }
            0x61 => {
                let a = self.addr_ind_x();
                let v = self.read(a);
                self.adc(v);
            }
            0x71 => {
                let a = self.addr_ind_y(false);
                let v = self.read(a);
                self.adc(v);
            }
            0xE9 => {
                let v = self.fetch();
                self.sbc(v);
            }
            0xE5 => {
                let a = self.addr_zp();
                let v = self.read(a);
                self.sbc(v);
            }
            0xF5 => {
                let a = self.addr_zp_x();
                let v = self.read(a);
                self.sbc(v);
            }
            0xED => {
                let a = self.addr_abs();
                let v = self.read(a);
                self.sbc(v);
            }
            0xFD => {
                let a = self.addr_abs_x(false);
                let v = self.read(a);
                self.sbc(v);
            }
            0xF9 => {
                let a = self.addr_abs_y(false);
                let v = self.read(a);
                self.sbc(v);
            }
            0xE1 => {
                let a = self.addr_ind_x();
                let v = self.read(a);
                self.sbc(v);
            }
            0xF1 => {
                let a = self.addr_ind_y(false);
                let v = self.read(a);
                self.sbc(v);
            }
            // ---- compare ----
            0xC9 => {
                let v = self.fetch();
                self.cmp(self.a, v);
            }
            0xC5 => {
                let a = self.addr_zp();
                let v = self.read(a);
                self.cmp(self.a, v);
            }
            0xD5 => {
                let a = self.addr_zp_x();
                let v = self.read(a);
                self.cmp(self.a, v);
            }
            0xCD => {
                let a = self.addr_abs();
                let v = self.read(a);
                self.cmp(self.a, v);
            }
            0xDD => {
                let a = self.addr_abs_x(false);
                let v = self.read(a);
                self.cmp(self.a, v);
            }
            0xD9 => {
                let a = self.addr_abs_y(false);
                let v = self.read(a);
                self.cmp(self.a, v);
            }
            0xC1 => {
                let a = self.addr_ind_x();
                let v = self.read(a);
                self.cmp(self.a, v);
            }
            0xD1 => {
                let a = self.addr_ind_y(false);
                let v = self.read(a);
                self.cmp(self.a, v);
            }
            0xE0 => {
                let v = self.fetch();
                self.cmp(self.x, v);
            }
            0xE4 => {
                let a = self.addr_zp();
                let v = self.read(a);
                self.cmp(self.x, v);
            }
            0xEC => {
                let a = self.addr_abs();
                let v = self.read(a);
                self.cmp(self.x, v);
            }
            0xC0 => {
                let v = self.fetch();
                self.cmp(self.y, v);
            }
            0xC4 => {
                let a = self.addr_zp();
                let v = self.read(a);
                self.cmp(self.y, v);
            }
            0xCC => {
                let a = self.addr_abs();
                let v = self.read(a);
                self.cmp(self.y, v);
            }
            // ---- inc/dec ----
            0xE6 => {
                let a = self.addr_zp();
                self.rmw(a, |s, v| {
                    let r = v.wrapping_add(1);
                    s.set_zn(r);
                    r
                });
            }
            0xF6 => {
                let a = self.addr_zp_x();
                self.rmw(a, |s, v| {
                    let r = v.wrapping_add(1);
                    s.set_zn(r);
                    r
                });
            }
            0xEE => {
                let a = self.addr_abs();
                self.rmw(a, |s, v| {
                    let r = v.wrapping_add(1);
                    s.set_zn(r);
                    r
                });
            }
            0xFE => {
                let a = self.addr_abs_x(true);
                self.rmw(a, |s, v| {
                    let r = v.wrapping_add(1);
                    s.set_zn(r);
                    r
                });
            }
            0xC6 => {
                let a = self.addr_zp();
                self.rmw(a, |s, v| {
                    let r = v.wrapping_sub(1);
                    s.set_zn(r);
                    r
                });
            }
            0xD6 => {
                let a = self.addr_zp_x();
                self.rmw(a, |s, v| {
                    let r = v.wrapping_sub(1);
                    s.set_zn(r);
                    r
                });
            }
            0xCE => {
                let a = self.addr_abs();
                self.rmw(a, |s, v| {
                    let r = v.wrapping_sub(1);
                    s.set_zn(r);
                    r
                });
            }
            0xDE => {
                let a = self.addr_abs_x(true);
                self.rmw(a, |s, v| {
                    let r = v.wrapping_sub(1);
                    s.set_zn(r);
                    r
                });
            }
            0xE8 => {
                self.x = self.x.wrapping_add(1);
                self.set_zn(self.x);
                self.tick();
            }
            0xC8 => {
                self.y = self.y.wrapping_add(1);
                self.set_zn(self.y);
                self.tick();
            }
            0xCA => {
                self.x = self.x.wrapping_sub(1);
                self.set_zn(self.x);
                self.tick();
            }
            0x88 => {
                self.y = self.y.wrapping_sub(1);
                self.set_zn(self.y);
                self.tick();
            }
            // ---- shifts ----
            0x0A => {
                self.tick();
                let v = self.a;
                self.c = v & 0x80 != 0;
                let r = v << 1;
                self.a = r;
                self.set_zn(r);
            }
            0x06 => {
                let a = self.addr_zp();
                self.rmw(a, |s, v| {
                    s.c = v & 0x80 != 0;
                    v << 1
                });
            }
            0x16 => {
                let a = self.addr_zp_x();
                self.rmw(a, |s, v| {
                    s.c = v & 0x80 != 0;
                    v << 1
                });
            }
            0x0E => {
                let a = self.addr_abs();
                self.rmw(a, |s, v| {
                    s.c = v & 0x80 != 0;
                    v << 1
                });
            }
            0x1E => {
                let a = self.addr_abs_x(true);
                self.rmw(a, |s, v| {
                    s.c = v & 0x80 != 0;
                    v << 1
                });
            }
            0x4A => {
                self.tick();
                let v = self.a;
                self.c = v & 1 != 0;
                let r = v >> 1;
                self.a = r;
                self.set_zn(r);
            }
            0x46 => {
                let a = self.addr_zp();
                self.rmw(a, |s, v| {
                    s.c = v & 1 != 0;
                    v >> 1
                });
            }
            0x56 => {
                let a = self.addr_zp_x();
                self.rmw(a, |s, v| {
                    s.c = v & 1 != 0;
                    v >> 1
                });
            }
            0x4E => {
                let a = self.addr_abs();
                self.rmw(a, |s, v| {
                    s.c = v & 1 != 0;
                    v >> 1
                });
            }
            0x5E => {
                let a = self.addr_abs_x(true);
                self.rmw(a, |s, v| {
                    s.c = v & 1 != 0;
                    v >> 1
                });
            }
            0x2A => {
                self.tick();
                let v = self.a;
                self.c = v & 1 != 0;
                let r = (v << 1) | (self.c as u8);
                self.a = r;
                self.set_zn(r);
            }
            0x26 => {
                let a = self.addr_zp();
                self.rmw(a, |s, v| {
                    let c = v & 0x80 != 0;
                    s.c = c;
                    (v << 1) | (c as u8)
                });
            }
            0x36 => {
                let a = self.addr_zp_x();
                self.rmw(a, |s, v| {
                    let c = v & 0x80 != 0;
                    s.c = c;
                    (v << 1) | (c as u8)
                });
            }
            0x2E => {
                let a = self.addr_abs();
                self.rmw(a, |s, v| {
                    let c = v & 0x80 != 0;
                    s.c = c;
                    (v << 1) | (c as u8)
                });
            }
            0x3E => {
                let a = self.addr_abs_x(true);
                self.rmw(a, |s, v| {
                    let c = v & 0x80 != 0;
                    s.c = c;
                    (v << 1) | (c as u8)
                });
            }
            0x6A => {
                self.tick();
                let v = self.a;
                self.c = v & 0x80 != 0;
                let r = (v >> 1) | ((self.c as u8) << 7);
                self.a = r;
                self.set_zn(r);
            }
            0x66 => {
                let a = self.addr_zp();
                self.rmw(a, |s, v| {
                    let c = v & 0x80 != 0;
                    s.c = c;
                    (v >> 1) | ((c as u8) << 7)
                });
            }
            0x76 => {
                let a = self.addr_zp_x();
                self.rmw(a, |s, v| {
                    let c = v & 0x80 != 0;
                    s.c = c;
                    (v >> 1) | ((c as u8) << 7)
                });
            }
            0x6E => {
                let a = self.addr_abs();
                self.rmw(a, |s, v| {
                    let c = v & 0x80 != 0;
                    s.c = c;
                    (v >> 1) | ((c as u8) << 7)
                });
            }
            0x7E => {
                let a = self.addr_abs_x(true);
                self.rmw(a, |s, v| {
                    let c = v & 0x80 != 0;
                    s.c = c;
                    (v >> 1) | ((c as u8) << 7)
                });
            }
            // ---- branches ----
            0x10 => self.branch(!self.n),
            0x30 => self.branch(self.n),
            0x50 => self.branch(!self.v),
            0x70 => self.branch(self.v),
            0x90 => self.branch(!self.c),
            0xB0 => self.branch(self.c),
            0xD0 => self.branch(!self.z),
            0xF0 => self.branch(self.z),
            // ---- jumps ----
            0x4C => {
                let a = self.addr_abs();
                self.pc = a;
            }
            0x6C => {
                let p = self.addr_abs();
                let lo = self.read(p) as u16;
                let hi_p = if p & 0xFF == 0xFF {
                    (p & 0xFF00)
                } else {
                    p.wrapping_add(1)
                };
                let hi = self.read(hi_p) as u16;
                self.pc = lo | (hi << 8);
            }
            0x20 => {
                let t = self.addr_abs();
                if self.trace { println!("JSR push {:04X} -> {:04X}", self.pc, t); }
                self.push((self.pc >> 8) as u8);
                self.push((self.pc & 0xFF) as u8);
                self.pc = t;
            }
            0x60 => {
                self.tick();
                let lo = self.pull() as u16;
                let hi = self.pull() as u16;
                if self.trace { println!("RTS -> {:04X}", lo | (hi << 8)); }
                self.pc = lo | (hi << 8);
                self.tick();
                self.read(self.pc);
            }
            0x40 => {
                self.tick();
                self.tick();
                let p = self.pull();
                self.set_p(p);
                let lo = self.pull() as u16;
                let hi = self.pull() as u16;
                self.pc = lo | (hi << 8);
            }
            0x00 => {
                self.tick();
                self.pc = self.pc.wrapping_add(1);
                self.push((self.pc >> 8) as u8);
                self.push((self.pc & 0xFF) as u8);
                let p = self.get_p(true);
                self.push(p);
                self.i = true;
                self.pc = self.read(0xFFFE) as u16 | ((self.read(0xFFFF) as u16) << 8);
            }
            // ---- flags ----
            0x18 => {
                self.c = false;
                self.tick();
            }
            0x38 => {
                self.c = true;
                self.tick();
            }
            0x58 => {
                self.i = false;
                self.tick();
            }
            0x78 => {
                self.i = true;
                self.tick();
            }
            0xB8 => {
                self.v = false;
                self.tick();
            }
            0xD8 => {
                self.d = false;
                self.tick();
            }
            0xF8 => {
                self.d = true;
                self.tick();
            }
            0xEA => {
                self.tick();
            }
            _ => {
                self.illegal(op);
            }
        }
    }

    #[inline]
    fn adc(&mut self, m: u8) {
        let a = self.a;
        let c = self.c as u16;
        let sum = a as u16 + m as u16 + c;
        let r = sum as u8;
        self.c = sum > 0xFF;
        self.v = ((a ^ r) & (m ^ r) & 0x80) != 0;
        self.a = r;
        self.set_zn(r);
    }

    #[inline]
    fn sbc(&mut self, m: u8) {
        let a = self.a;
        let c = self.c as u16;
        let sum = a as u16 + (!m) as u16 + c;
        let r = sum as u8;
        self.c = sum > 0xFF;
        self.v = ((a ^ r) & (!m ^ r) & 0x80) != 0;
        self.a = r;
        self.set_zn(r);
    }

    #[inline]
    fn cmp(&mut self, reg: u8, m: u8) {
        let r = reg.wrapping_sub(m);
        self.c = reg >= m;
        self.set_zn(r);
    }

    fn interrupt_nmi(&mut self) {
        if self.trace { println!("NMI at pc={:04X} sl={} dot={}", self.pc, self.ppu.scanline, self.ppu.dot); }
        self.read(self.pc);
        self.tick();
        self.read(self.pc);
        self.push((self.pc >> 8) as u8);
        self.push((self.pc & 0xFF) as u8);
        let p = self.get_p(false);
        self.push(p);
        self.i = true;
        let lo = self.read(0xFFFA) as u16;
        let hi = self.read(0xFFFB) as u16;
        if self.trace { println!("  -> NMI vector {:04X}", lo|hi<<8); }
        self.pc = lo | (hi << 8);
    }

    fn interrupt_irq(&mut self) {
        self.read(self.pc);
        self.tick();
        self.read(self.pc);
        self.push((self.pc >> 8) as u8);
        self.push((self.pc & 0xFF) as u8);
        let p = self.get_p(false);
        self.push(p);
        self.i = true;
        let lo = self.read(0xFFFE) as u16;
        let hi = self.read(0xFFFF) as u16;
        self.pc = lo | (hi << 8);
    }

    fn illegal(&mut self, op: u8) {
        // Undocumented opcodes: implement the common ones, NOP the rest.
        match op {
            // SLO
            0x03 | 0x07 | 0x0F | 0x13 | 0x17 | 0x1B | 0x1F => {
                let a = self.il_addr(op, false);
                let v = self.read(a);
                self.a = {
                    let r = v << 1;
                    self.c = v & 0x80 != 0;
                    r
                };
                self.set_zn(self.a);
                self.write(a, self.a);
            }
            // RLA
            0x23 | 0x27 | 0x2F | 0x33 | 0x37 | 0x3B | 0x3F => {
                let a = self.il_addr(op, false);
                let v = self.read(a);
                let nc = v & 0x80 != 0;
                let r = (v << 1) | (self.c as u8);
                self.c = nc;
                self.a = r;
                self.set_zn(r);
                self.write(a, r);
            }
            // SRE
            0x43 | 0x47 | 0x4F | 0x53 | 0x57 | 0x5B | 0x5F => {
                let a = self.il_addr(op, false);
                let v = self.read(a);
                self.c = v & 1 != 0;
                let r = v >> 1;
                self.a = r;
                self.set_zn(r);
                self.write(a, r);
            }
            // RRA
            0x63 | 0x67 | 0x6F | 0x73 | 0x77 | 0x7B | 0x7F => {
                let a = self.il_addr(op, false);
                let v = self.read(a);
                self.c = v & 1 != 0;
                let m = v >> 1;
                self.write(a, m);
                self.adc(m);
            }
            // SAX
            0x83 | 0x87 | 0x8F | 0x97 => {
                let a = self.il_addr(op, false);
                self.write(a, self.a & self.x);
            }
            // LAX
            0xA3 | 0xA7 | 0xAF | 0xB3 | 0xB7 | 0xBF => {
                let a = self.il_addr(op, false);
                let v = self.read(a);
                self.a = v;
                self.x = v;
                self.set_zn(v);
            }
            // DCP
            0xC3 | 0xC7 | 0xCF | 0xD3 | 0xD7 | 0xDB | 0xDF => {
                let a = self.il_addr(op, false);
                let v = self.read(a).wrapping_sub(1);
                self.write(a, v);
                self.cmp(self.a, v);
            }
            // ISC
            0xE3 | 0xE7 | 0xEF | 0xF3 | 0xF7 | 0xFB | 0xFF => {
                let a = self.il_addr(op, false);
                let v = self.read(a).wrapping_add(1);
                self.write(a, v);
                self.cmp(self.a, v);
            }
            // ANC
            0x0B | 0x2B => {
                let v = self.fetch();
                self.a &= v;
                self.c = self.a & 0x80 != 0;
                self.set_zn(self.a);
            }
            // ALR
            0x4B => {
                let v = self.fetch();
                self.a &= v;
                self.c = self.a & 1 != 0;
                self.a >>= 1;
                self.set_zn(self.a);
            }
            // ARR
            0x6B => {
                let v = self.fetch();
                self.a &= v;
                let c = self.c as u8;
                let r = (self.a >> 1) | (c << 7);
                self.a = r;
                self.c = (r & 0x40 != 0) ^ (r & 0x20 != 0);
                self.v = (((r >> 6) ^ (r >> 5)) & 1) != 0;
                self.set_zn(r);
            }
            // SBX
            0xCB => {
                let v = self.fetch();
                let t = (self.a & self.x).wrapping_sub(v);
                self.c = (self.a & self.x) >= v;
                self.x = t;
                self.set_zn(t);
            }
            // NOPs
            0x1A | 0x3A | 0x5A | 0x7A | 0xDA | 0xFA => {
                self.tick();
            }
            0x80 | 0x82 | 0x89 | 0xC2 | 0xE2 => {
                self.fetch();
            }
            0x04 | 0x44 | 0x64 => {
                self.fetch();
                self.tick();
            }
            0x14 | 0x34 | 0x54 | 0x74 | 0xD4 | 0xF4 => {
                let b = self.fetch();
                self.tick();
                let a = b.wrapping_add(self.x) as u16;
                self.tick();
                let v = self.read(a);
                self.a &= v;
                self.set_zn(self.a);
            }
            0x0C => {
                let a = self.addr_abs();
                self.tick();
                let v = self.read(a);
                self.a &= v;
                self.set_zn(self.a);
            }
            0x1C | 0x3C | 0x5C | 0x7C | 0xDC | 0xFC => {
                let base = self.addr_abs();
                let a = base.wrapping_add(self.x as u16);
                if (a & 0xFF00) != (base & 0xFF00) {
                    self.tick();
                }
                let v = self.read(a);
                self.a &= v;
                self.set_zn(self.a);
            }
            _ => {
                self.tick();
            }
        }
    }

    fn il_addr(&mut self, op: u8, _rmw: bool) -> u16 {
        match op {
            0x03 | 0x23 | 0x43 | 0x63 | 0x83 | 0xA3 | 0xC3 | 0xE3 => self.addr_ind_x(),
            0x07 | 0x27 | 0x47 | 0x67 | 0x87 | 0xA7 | 0xC7 | 0xE7 => self.addr_zp(),
            0x0F | 0x2F | 0x4F | 0x6F | 0x8F | 0xAF | 0xCF | 0xEF => self.addr_abs(),
            0x13 | 0x33 | 0x53 | 0x73 | 0x93 | 0xB3 | 0xD3 | 0xF3 => self.addr_ind_y(false),
            0x17 | 0x37 | 0x57 | 0x77 | 0x97 | 0xB7 | 0xD7 | 0xF7 => self.addr_zp_x(),
            _ => self.addr_abs_y(false),
        }
    }

    fn do_dma(&mut self) {
        self.dma_pending = false;
        self.tick();
        if self.cycles & 1 == 1 {
            self.tick();
        }
        let page = self.dma_page;
        for i in 0..256u16 {
            self.tick();
            let addr = ((page as u16) << 8) | i;
            let b = self.read_raw(addr);
            self.ppu.oam[self.ppu.oamaddr as usize] = b;
            if self.ppu.ctrl & 0x02 != 0 {
                self.ppu.oamaddr = self.ppu.oamaddr.wrapping_add(1);
            } else {
                self.ppu.oamaddr = self.ppu.oamaddr.wrapping_add(4);
            }
            self.tick();
        }
    }
}