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

    let data = std::fs::read(rom).unwrap();
    std::fs::create_dir_all(outdir).unwrap();

    let mut n = Nes::new();
    if !n.load_rom(&data) {
        eprintln!("load failed");
        std::process::exit(1);
    }
    let mut si = 0usize;
    let mut keys = 0u32;
    for f in 0..frames {
        while si < script.len() && script[si].0 == f {
            keys = script[si].1;
            si += 1;
        }
        n.keys = keys;
        n.run_frame();
        let fb = &n.ppu.framebuffer;
        let mut bytes = Vec::with_capacity(122880);
        for v in fb.iter() {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        std::fs::write(format!("{}/frame_{:05}.idx", outdir, f), &bytes).unwrap();
        if env::var("TRACE").is_ok() { n.trace = true; println!("f{} n={} sl={} dot={} pc={:04X}", f, n.audio.len(), n.ppu.scanline, n.ppu.dot, n.pc); }
        if false { eprintln!("f{} n={} sl={} dot={} pc={:04X}", f, n.audio.len(), n.ppu.scanline, n.ppu.dot, n.pc); }
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
            eprintln!("frame {} samples={} sl={} dot={} pc={:04X} guard", f, n.audio.len(), n.ppu.scanline, n.ppu.dot, n.pc);
        }
    }
}// debug helper appended
