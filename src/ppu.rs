// NES PPU (2C02) - dot based renderer.

pub struct Ppu {
    pub vram: [u8; 2048],
    pub palette: [u8; 32],
    pub oam: [u8; 256],
    pub secondary_oam: [u8; 32],

    pub ctrl: u8,
    pub mask: u8,
    pub status: u8,
    pub oamaddr: u8,
    pub scroll_x: u8,
    pub scroll_y: u8,
    pub v: u16,
    pub t: u16,
    pub fine_x: u8,
    pub write_toggle: bool,
    pub read_buffer: u8,
    pub open_bus: u8,

    pub scanline: u16,
    pub dot: u16,
    pub dc: u64,
    pub odd_frame: bool,
    pub frame_count: u64,

    pub nmi_latch: bool,
    pub nmi_output: bool,

    // background pipeline
    nt_byte: u8,
    at_byte: u8,
    bg_lo_latch: u8,
    bg_hi_latch: u8,
    at_bits: u8,
    sh_lo: u16,
    sh_hi: u16,
    sh_at: u16,
    pend_lo: u8,
    pend_hi: u8,
    pend_at: u8,

    // sprite pipeline
    pub sprite_count: usize,
    sp_lo: [u8; 8],
    sp_hi: [u8; 8],
    sp_attr: [u8; 8],
    sp_x: [u8; 8],
    pub sprite_zero_in_range: bool,

    eval_m: u16,
    eval_n: u8,
    eval_done: bool,

    pub framebuffer: Vec<u16>,
    pub mirroring: u8, // 0=horiz, 1=vert, 2=single0, 3=single1
    pub warmup: i64,
    pub trace2: bool,
    pub trace_sl: u16,
    pub last_pc: u16,
    pub chr_banks: [u8; 8],
    pub nt_bank: u8,
    pub chr_ptr: *const u8,
    pub chr_len: usize,
    pub chr_writable: *mut u8,
}

const MIRROR_HORIZ: u8 = 0;
const MIRROR_VERT: u8 = 1;

impl Ppu {
    pub fn new() -> Self {
        Ppu {
            vram: [0; 2048],
            palette: [0; 32],
            oam: [0; 256],
            secondary_oam: [0; 32],
            ctrl: 0,
            mask: 0,
            status: 0,
            oamaddr: 0,
            scroll_x: 0,
            scroll_y: 0,
            v: 0,
            t: 0,
            fine_x: 0,
            write_toggle: false,
            read_buffer: 0,
            open_bus: 0,
            scanline: 0,
            dot: 25,
            dc: 25,
            odd_frame: false,
            frame_count: 0,
            nmi_latch: false,
            nmi_output: false,
            nt_byte: 0,
            at_byte: 0,
            bg_lo_latch: 0,
            bg_hi_latch: 0,
            at_bits: 0,
            sh_lo: 0,
            sh_hi: 0,
            sh_at: 0,
            pend_lo: 0,
            pend_hi: 0,
            pend_at: 0,
            sprite_count: 0,
            sp_lo: [0; 8],
            sp_hi: [0; 8],
            sp_attr: [0; 8],
            sp_x: [0; 8],
            sprite_zero_in_range: false,
            eval_m: 0,
            eval_n: 0,
            eval_done: false,
            framebuffer: vec![0; 256 * 240],
            mirroring: MIRROR_HORIZ,
            warmup: 29658,
            trace2: false,
            trace_sl: 0,
            last_pc: 0,
            chr_banks: [0; 8],
            nt_bank: 0,
            chr_ptr: core::ptr::null(),
            chr_len: 0,
            chr_writable: core::ptr::null_mut(),
        }
    }

    #[inline]
    pub fn rendering_enabled(&self) -> bool {
        self.mask & 0x18 != 0
    }

    pub fn reset_state(&mut self) {
        let fb = core::mem::take(&mut self.framebuffer);
        let mir = self.mirroring;
        let cp = self.chr_ptr;
        let cl = self.chr_len;
        let cw = self.chr_writable;
        let mut p = Self::new();
        p.framebuffer = fb;
        p.mirroring = mir;
        p.chr_ptr = cp;
        p.chr_len = cl;
        p.chr_writable = cw;
        *self = p;
    }

    #[inline]
    fn mirror_vram(&self, addr: u16) -> usize {
        let a = addr & 0x0FFF;
        let off = (a & 0x3FF) as usize;
        let horiz = ((a >> 10) & 1) ^ ((self.nt_bank & 1) as u16);
        let vert = ((a >> 11) & 1) ^ ((self.nt_bank & 1) as u16);
        let table: usize = match self.mirroring {
            MIRROR_HORIZ => (vert & 1) as usize,
            MIRROR_VERT => (horiz & 1) as usize,
            2 => ((self.nt_bank & 1) as usize) << 1,
            _ => (((self.nt_bank & 1) as usize) << 1) | 1,
        };
        (table << 10) | off
    }

    #[inline]
    pub fn chr_read(&self, addr: u16) -> u8 {
        let a = (addr & 0x1FFF) as usize;
        let bank = self.chr_banks[a >> 10] as usize;
        let off = bank * 0x400 + (a & 0x3FF);
        if self.chr_len == 0 {
            0
        } else {
            unsafe { *self.chr_ptr.add(off % self.chr_len) }
        }
    }

    #[inline]
    pub fn chr_write(&mut self, addr: u16, v: u8) {
        if self.chr_writable.is_null() {
            return;
        }
        let a = (addr & 0x1FFF) as usize;
        let bank = self.chr_banks[a >> 10] as usize;
        let off = bank * 0x400 + (a & 0x3FF);
        if self.trace2 && off < 0x200 { println!("CHRW {:04X} -> {:04X} <- {:02X}", addr, off, v); }
        if self.chr_len != 0 {
            unsafe {
                *self.chr_writable.add(off % self.chr_len) = v;
            }
        }
    }

    #[inline]
    pub fn ppu_read(&self, addr: u16) -> u8 {
        let addr = addr & 0x3FFF;
        if addr >= 0x3F00 {
            self.palette[(addr & 0x1F) as usize]
        } else if addr < 0x2000 {
            self.chr_read(addr)
        } else {
            self.vram[self.mirror_vram(addr)]
        }
    }

    #[inline]
    pub fn palette_index(&self, idx: usize) -> u8 {
        let mut i = idx & 0x1F;
        if i & 0x13 == 0x10 {
            i -= 0x10;
        }
        self.palette[i]
    }

    #[inline]
    fn palette_write(&mut self, idx: usize, v: u8) {
        let mut i = idx & 0x1F;
        if i & 0x13 == 0x10 {
            i -= 0x10;
        }
        self.palette[i] = v & 0x3F;
    }

    pub fn ppu_write(&mut self, addr: u16, v: u8) {
        let addr = addr & 0x3FFF;
        if self.trace2 && addr >= 0x3F00 { println!("PALW {:04X} <- {:02X}", addr, v); }
        if addr >= 0x3F00 {
            self.palette_write((addr & 0x1F) as usize, v);
        } else if addr < 0x2000 {
            self.chr_write(addr, v);
        } else {
            let i = self.mirror_vram(addr);
            self.vram[i] = v;
        }
    }

    pub fn cpu_read(&mut self, addr: u16) -> u8 {
        let a = addr & 7;
        match a {
            0x02 => {
                if self.trace2 {
                    println!("R2002 status={:02X} sl={} dot={} cyc={}", self.status, self.scanline, self.dot, self.dc);
                }
                let r = (self.status & 0xE0) | (self.open_bus & 0x1F);
                self.status &= !0x80;
                self.nmi_latch = false;
                self.write_toggle = false;
                self.update_nmi();
                self.open_bus = r;
                r
            }
            0x04 => {
                let r = self.oam[self.oamaddr as usize];
                self.open_bus = r;
                r
            }
            0x07 => {
                let addr = self.v & 0x3FFF;
                let mut r = self.read_buffer;
                if addr >= 0x3F00 {
                    r = self.ppu_read(addr);
                    self.read_buffer = self.ppu_read(addr.wrapping_sub(0x1000) & 0x3FFF);
                } else {
                    self.read_buffer = self.ppu_read(addr);
                }
                self.increment_v();
                self.open_bus = r;
                r
            }
            _ => self.open_bus,
        }
    }

    fn increment_v(&mut self) {
        if self.ctrl & 0x04 != 0 {
            self.v = self.v.wrapping_add(32);
        } else {
            self.v = self.v.wrapping_add(1);
        }
        self.v &= 0x7FFF;
    }

    pub fn cpu_write(&mut self, addr: u16, val: u8) {
        let a = addr & 0x3FFF;
        if a < 0x2000 {
            return;
        }
        self.open_bus = val;
        match addr & 7 {
            0x0000 => {
                if self.trace2 { println!("SET ctrl {:02X} (was {:02X}) at cyc", val, self.ctrl); }
                self.ctrl = val;
                self.t = (self.t & 0xF3FF) | ((val as u16 & 3) << 10);
                if val & 0x80 == 0 {
                    self.nmi_latch = false;
                }
                self.update_nmi();
            }
            0x01 => self.mask = val,
            0x03 => self.oamaddr = val,
            0x04 => {
                self.oam[self.oamaddr as usize] = val;
                if self.ctrl & 0x02 != 0 {
                    self.oamaddr = self.oamaddr.wrapping_add(1);
                } else {
                    self.oamaddr = self.oamaddr.wrapping_add(4);
                }
            }
            0x05 => {
                if self.trace2 { println!("W2005 w={} v={:02X} t={:04X} v={:04X} f={}", self.write_toggle as u8, val, self.t, self.v, self.frame_count); }
                if !self.write_toggle {
                    self.t = (self.t & 0x7FE0) | ((val >> 3) as u16);
                    self.fine_x = val & 7;
                    self.scroll_x = val;
                } else {
                    self.t = (self.t & 0x0C1F) | ((val as u16 & 7) << 12) | ((val as u16 & 0xF8) << 2);
                    self.scroll_y = val;
                }
                self.write_toggle = !self.write_toggle;
            }
            0x06 => {
                if self.trace2 { println!("W2006 w={} v={:02X} t={:04X} v={:04X} f={}", self.write_toggle as u8, val, self.t, self.v, self.frame_count); }
                if !self.write_toggle {
                    self.t = (self.t & 0x00FF) | ((val as u16 & 0x3F) << 8);
                } else {
                    self.t = (self.t & 0xFF00) | val as u16;
                    self.v = self.t;
                }
                self.write_toggle = !self.write_toggle;
            }
            0x07 => {
                let addr = self.v & 0x3FFF;
                if self.trace2 { println!("W2007 v={:04X} <- {:02X} ctrl={:02X} mask={:02X} w={} pc={:04X} sl={} dot={} f={}", addr, val, self.ctrl, self.mask, self.write_toggle as u8, self.last_pc, self.scanline, self.dot, self.frame_count); }
                self.ppu_write(addr, val);
                self.increment_v();
            }
            _ => {}
        }
    }

    pub fn clear_nmi_latch(&mut self) {
        self.nmi_latch = false;
        self.update_nmi();
    }

    fn update_nmi(&mut self) {
        self.nmi_output = self.nmi_latch && (self.ctrl & 0x80) != 0;
    }

    #[inline]
    fn inc_x(&mut self) {
        if self.v & 0x001F == 31 {
            self.v &= !0x001F;
            self.v ^= 0x0400;
        } else {
            self.v += 1;
        }
    }
    #[inline]
    fn inc_y(&mut self) {
        if self.v & 0x7000 != 0x7000 {
            self.v += 0x1000;
        } else {
            self.v &= !0x7000;
            let mut y = (self.v & 0x03E0) >> 5;
            if y == 29 {
                y = 0;
                self.v ^= 0x0800;
            } else if y == 31 {
                y = 0;
            } else {
                y += 1;
            }
            self.v = (self.v & !0x03E0) | (y << 5);
        }
    }
    #[inline]
    fn copy_x(&mut self) {
        self.v = (self.v & !0x041F) | (self.t & 0x041F);
    }
    #[inline]
    fn copy_y(&mut self) {
        self.v = (self.v & !0x7BE0) | (self.t & 0x7BE0);
    }

    #[inline]
    fn bg_pattern_addr(&self) -> u16 {
        let base = if self.ctrl & 0x10 != 0 { 0x1000u16 } else { 0 };
        let row = (self.v >> 12) & 7;
        (base + (self.nt_byte as u16) * 16 + row) & 0x3FFF
    }

    #[inline]
    fn sprite_height(&self) -> i32 {
        if self.ctrl & 0x20 != 0 {
            16
        } else {
            8
        }
    }

    pub fn tick(&mut self) {
        let sl = self.scanline;
        let dot = self.dot;
        let rendering = self.rendering_enabled() && (sl < 240 || sl == 261);
        let visible = sl < 240 && dot >= 1 && dot <= 256;
        let dc = self.dc;

        if sl == 261 && dot == 1 {
            self.status &= 0x1F;
            self.update_nmi();
        }

        if sl == 241 && dot == 1 {
            if self.trace2 { println!("VB241 ctrl={:02X} latch={} out={}", self.ctrl, self.nmi_latch, self.nmi_output); }
            self.status |= 0x80;
            if (self.ctrl & 0x80) != 0 {
                self.nmi_latch = true;
            }
            self.update_nmi();
        }

        if visible {
            self.render_pixel(dot as u16 - 1, dc);
        }

        if rendering {
            if dot <= 256 || dot >= 321 {
                self.sh_lo >>= 1;
                self.sh_hi >>= 1;
                self.sh_at >>= 1;
            }
            if (dot <= 256 && dot % 8 == 0) || dot == 333 {
                self.sh_lo = (self.sh_lo & 0x00FF) | ((self.pend_lo as u16) << 8);
                self.sh_hi = (self.sh_hi & 0x00FF) | ((self.pend_hi as u16) << 8);
                self.sh_at = (self.sh_at & 0x00FF) | ((self.pend_at as u16) << 8);
            }
            // ---- background fetch pipeline ----
            match dot % 8 {
                1 => {
                    self.nt_byte = self.ppu_read(0x2000 | (self.v & 0x0FFF));
                }
                3 => {
                    self.at_byte = self.ppu_read(
                        0x23C0 | (self.v & 0x0C00) | ((self.v >> 4) & 0x38) | ((self.v >> 2) & 0x07),
                    );
                    let quad = ((self.v >> 4) & 4) | (self.v & 2);
                    let val = (self.at_byte >> quad) & 3;
                    self.at_bits = (val & 1) | ((val >> 1) & 1) << 1;
                }
                5 => {
                    let addr = self.bg_pattern_addr();
                    self.bg_lo_latch = self.ppu_read(addr);
                }
                7 => {
                    let addr = self.bg_pattern_addr();
                    if self.trace2 && self.trace_sl == sl {
                        println!("S{} d{} v={:04X} nt={:02X} addr={:04X} lo={:02X} k={} kk={} finex={}", sl, dot, self.v, self.nt_byte, addr, self.bg_lo_latch, ((dc/8)&1), (((dc.wrapping_sub(8))/8)&1), self.fine_x);
                    }
                    self.bg_hi_latch = self.ppu_read((addr + 8) & 0x3FFF);
                    self.pend_lo = self.bg_lo_latch;
                    self.pend_hi = self.bg_hi_latch;
                    let sel = ((self.v >> 1) & 1) as usize;
                    self.pend_at = if ((self.at_bits >> sel) & 1) != 0 { 0xFF } else { 0x00 };
                }
                _ => {}
            }
            if (dot >= 8 && dot <= 256 && dot % 8 == 0) || dot == 328 || dot == 336 {
                self.inc_x();
            }
            if dot == 256 {
                self.inc_y();
            }
            if dot == 257 {
                self.copy_x();
            }
            if dot == 337 || dot == 339 {
                let _ = self.ppu_read(0x2000 | (self.v & 0x0FFF));
            }
            if sl == 261 && dot >= 280 && dot <= 304 {
                self.copy_y();
            }

            // ---- sprite evaluation (for next scanline) ----
            if sl < 240 || sl == 261 {
                if dot == 65 {
                    self.eval_m = 0;
                    self.eval_n = 0;
                    self.eval_done = false;
                } else if dot == 257 {
                    self.eval_done = true;
                }
                if (dot >= 65 && dot <= 256) && !self.eval_done {
                    self.eval_step();
                }
                // ---- sprite fetches ----
                if dot >= 257 && dot <= 320 {
                    self.sprite_fetch(dot);
                }
                if dot == 257 {
                    self.oamaddr = 0;
                }
                if dot == 338 {
                    for i in 0..8 {
                        self.secondary_oam[i * 4] = 0;
                    }
                }
            }
        } else if sl == 261 && dot >= 280 && dot <= 304 {
            self.copy_y();
        }

        // advance
        self.dc = dc + 1;
        self.dot += 1;
        if self.dot > 340 {
            self.dot = 0;
            self.scanline += 1;
            if self.scanline > 261 {
                self.scanline = 0;
                self.odd_frame = !self.odd_frame;
                self.frame_count += 1;
            }
        }
    }

    #[inline]
    fn render_pixel(&mut self, x: u16, dc: u64) {
        let sl = self.scanline;
        let emph = (self.mask >> 5) & 7;
        let grey = self.mask & 1 != 0;
        let show_bg = self.mask & 8 != 0;
        let show_sp = self.mask & 16 != 0;
        let clip_left = self.mask & 2 != 0;
        let clip_right = self.mask & 4 != 0;

        let rendering = self.rendering_enabled();
        let x_vis = if x < 8 { clip_left } else { true } && (x < 256);

        let mut bg_pixel: u8 = 0;
        let mut bg_pal: u8 = 0;
        let mut bg_ok = false;
        if rendering && show_bg && x_vis {
            let bit = self.fine_x as u16;
            let lo = ((self.sh_lo >> bit) & 1) as u8;
            let hi = ((self.sh_hi >> bit) & 1) as u8;
            let at = ((self.sh_at >> bit) & 1) as u8;
            let color = lo | (hi << 1);
            if color != 0 {
                bg_pal = (self.ctrl & 3) * 4 + at * 4;
                bg_pixel = color;
                bg_ok = true;
            }
        }

        // sprites
        let mut sp_pixel: u8 = 0;
        let mut sp_pal: u8 = 0;
        let mut sp_priority = true;
        let mut sp_opaque = false;
        let mut sp_zero = false;
        if rendering && show_sp && x_vis {
            for i in 0..self.sprite_count {
                let sx = self.sp_x[i] as u16;
                if x >= sx && x < sx + 8 {
                    let mut idx = (x - sx) as u8;
                    if self.sp_attr[i] & 0x40 != 0 {
                        idx = 7 - idx;
                    }
                    let lo = (self.sp_lo[i] >> idx) & 1;
                    let hi = (self.sp_hi[i] >> idx) & 1;
                    let color = lo | (hi << 1);
                    if color != 0 {
                        sp_opaque = true;
                        sp_pixel = color;
                        sp_pal = 0x10 + (self.sp_attr[i] & 3) * 4;
                        sp_priority = self.sp_attr[i] & 0x20 == 0;
                        sp_zero = i == 0 && self.sprite_zero_in_range;
                        break;
                    }
                }
            }
        }

        // sprite 0 hit
        if rendering
            && show_bg
            && show_sp
            && self.sprite_zero_in_range
            && bg_ok
            && sp_opaque
            && sp_zero
            && !(x < 8 && !clip_left)
            && x != 255
        {
            self.status |= 0x40;
        }

        let color_idx: usize = if sp_opaque && (!sp_priority || !bg_ok) {
            (sp_pal + sp_pixel) as usize
        } else if bg_ok {
            (bg_pal + bg_pixel) as usize
        } else {
            0
        };
        let mut c = self.palette_index(color_idx);
        if grey {
            c &= 0x30;
        }
        let out = ((emph as u16) << 6) | (c & 0x3F) as u16;
        if sl < 240 && (x as usize) < 256 {
            let idx = (sl as usize) * 256 + x as usize;
            self.framebuffer[idx] = out;
            if self.trace2 && self.trace_sl == sl && x < 48 {
                println!("PX {} {} lo={:04X} hi={:04X} at={:04X} fx={} bgp={} bgo={} idx={} c={:02X} pl={:02X} ph={:02X} nt={:02X}", sl, x, self.sh_lo, self.sh_hi, self.sh_at, self.fine_x, bg_pal, bg_pixel, color_idx, c, self.pend_lo, self.pend_hi, self.nt_byte);
            }
        }
    }

    fn eval_step(&mut self) {
        let h = self.sprite_height();
        let next_row: i32 = if self.scanline == 261 {
            0
        } else {
            (self.scanline + 1) as i32
        };
        if self.eval_done {
            return;
        }
        let mut m = self.eval_m;
        let mut n = self.eval_n;
        if m >= 256 {
            self.eval_done = true;
            return;
        }
        let sy = self.oam[m as usize] as i32;
        let adj = if h == 16 { sy - 1 } else { sy };
        let in_range = next_row - adj >= 0 && next_row - adj < h;
        if in_range {
            if n < 8 {
                for k in 0..4 {
                    self.secondary_oam[(n as usize) * 4 + k] = self.oam[m as usize + k];
                }
                if m == 0 {
                    self.sprite_zero_in_range = true;
                }
                n += 1;
                m += 4;
            } else {
                self.status |= 0x20;
                m += 2;
            }
        } else {
            m += 1;
        }
        self.eval_m = m;
        self.eval_n = n;
    }

    fn sprite_fetch(&mut self, dot: u16) {
        // dots 257-320: 8 sprites, 8 dots each
        if dot < 257 || dot > 320 {
            return;
        }
        let i = ((dot - 257) / 8) as usize;
        if i > 7 {
            return;
        }
        let step = (dot - 257) % 8;
        match step {
            1 => {
                self.sp_attr[i] = self.secondary_oam[i * 4 + 1];
                self.sp_x[i] = self.secondary_oam[i * 4 + 3];
            }
            3 => {
                self.sprite_count = self.eval_n as usize;
            }
            7 => {
                let y = self.secondary_oam[i * 4] as u16;
                let tile = self.secondary_oam[i * 4 + 2];
                let attr = self.secondary_oam[i * 4 + 1];
                let row = if self.scanline == 261 {
                    0u16
                } else {
                    (self.scanline as u16 + 1).wrapping_sub(y)
                };
                let mut r = row & 7;
                let mut tile_idx = tile as u16;
                if attr & 0x80 != 0 {
                    r = 7 - r;
                }
                let h = self.sprite_height();
                let addr: u16;
                if h == 16 {
                    let table = (tile_idx & 1) * 0x1000;
                    let base = if (tile_idx & 1) == 0 { 0 } else { 1 };
                    let idx = base * 16 + (r & 15) + if r < 8 { 0 } else { 16 };
                    addr = table + idx;
                } else {
                    let base = if self.ctrl & 0x08 != 0 { 0x1000 } else { 0 };
                    addr = base + tile_idx * 16 + r;
                }
                self.sp_lo[i] = self.ppu_read(addr & 0x3FFF);
                self.sp_hi[i] = self.ppu_read((addr + 8) & 0x3FFF);
                if attr & 0x40 != 0 {
                    self.sp_lo[i] = self.sp_lo[i].reverse_bits();
                    self.sp_hi[i] = self.sp_hi[i].reverse_bits();
                }
            }
            _ => {}
        }
    }
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}