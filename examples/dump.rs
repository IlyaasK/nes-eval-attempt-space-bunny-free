use nes_emu::Nes;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    let rom = &args[1];
    let frames: usize = args[2].parse().unwrap();
    let outdir = &args[3];
    let script: Vec<(usize, u32)> = if args.len() > 4 {
        std::fs::read_to_string(&args[4])
            .unwrap()
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
            .map(|l| {
                let mut it = l.split_whitespace();
                let f: usize = it.next().unwrap().parse().unwrap();
                let k = u32::from_str_radix(it.next().unwrap(), 16).unwrap();
                (f, k)
            })
            .collect()
    } else {
        Vec::new()
    };
    let start_frame: usize = if args.len() > 5 {
        args[5].parse().unwrap()
    } else {
        0
    };

    let data = std::fs::read(rom).unwrap();
    std::fs::create_dir_all(outdir).unwrap();

    let mut n = Nes::new();
    if !n.load_rom(&data) {
        eprintln!("load failed");
        std::process::exit(1);
    }
    let mut si = 0usize;
    let mut keys = 0u32;
    let tracing = env::var("TRACE").is_ok();
    let first_trace = env::var("TRACEFROM").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    for f in 0..frames {
        while si < script.len() && script[si].0 == f {
            keys = script[si].1;
            si += 1;
        }
        n.keys = keys;
        n.trace = tracing && f >= first_trace;
        n.ppu.trace2 = n.trace;
        n.run_frame();
        n.trace = false;
        n.ppu.trace2 = n.trace;
        if f >= start_frame {
            let fb = &n.ppu.framebuffer;
            let mut bytes = Vec::with_capacity(122880);
            for v in fb.iter() {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
            std::fs::write(format!("{}/frame_{:05}.idx", outdir, f), &bytes).unwrap();
        }
        if env::var("PAL").is_ok() && f + 1 == frames {
            eprintln!("mask={:02X} ctrl={:02X} pal={:02X?} nt0={:02X?} attr={:02X?} chr0={:02X?} chr20={:02X?} banks={:02X?} chr0reg={:02X} m1ctrl={:02X}", n.ppu.mask, n.ppu.ctrl, n.ppu.palette, &n.ppu.vram[..8], (0..8).map(|i| n.ppu.chr_read((i*0x200+0x20) as u16)).collect::<Vec<_>>(), n.ppu.chr_banks, n.mapper.chr0, n.mapper.ctrl);
        }
        if f + 1 == frames {
            let mut wav = Vec::new();
            wav.extend_from_slice(b"RIFF");
            let nsamp = n.audio.len() as u32 * 2;
            wav.extend_from_slice(&(36 + nsamp).to_le_bytes());
            wav.extend_from_slice(b"WAVEfmt ");
            wav.extend_from_slice(&16u32.to_le_bytes());
            wav.extend_from_slice(&1u16.to_le_bytes());
            wav.extend_from_slice(&1u16.to_le_bytes());
            wav.extend_from_slice(&44100u32.to_le_bytes());
            wav.extend_from_slice(&88200u32.to_le_bytes());
            wav.extend_from_slice(&2u16.to_le_bytes());
            wav.extend_from_slice(&16u16.to_le_bytes());
            wav.extend_from_slice(b"data");
            wav.extend_from_slice(&nsamp.to_le_bytes());
            for s in n.audio.iter() {
                wav.extend_from_slice(&s.to_le_bytes());
            }
            std::fs::write(format!("{}/audio.wav", outdir), &wav).unwrap();
        }
        if env::var("PRAM").is_ok() && f + 1 == frames {
            let pr = &n.mapper.prg_ram;
            let mut s = String::new();
            let mut i = 0;
            while i < pr.len() {
                if pr[i] == 0 { i += 1; continue; }
                let st = i;
                while i < pr.len() && pr[i] != 0 { i += 1; }
                s.push_str(&String::from_utf8_lossy(&pr[st..i]));
                s.push('\n');
            }
            eprintln!("PRAM:\n{}", s);
        }
        if tracing {
            eprintln!(
                "frame {} samples={} sl={} dot={} pc={:04X} warm={}",
                f,
                n.audio.len(),
                n.ppu.scanline,
                n.ppu.dot,
                n.pc,
                n.ppu.warmup
            );
        }
    }
}