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
    if let Ok(v) = env::var("SCALE") { n.apu.out_scale = v.parse().unwrap(); }
    if env::var("APUD").is_ok() { n.ppu.trace2 = false; }
    if let Ok(v) = env::var("HP") { n.apu.hp_coeff = v.parse().unwrap(); }
    if !n.load_rom(&data) {
        eprintln!("load failed");
        std::process::exit(1);
    }
    let mut si = 0usize;
    let mut keys = 0u32;
    let tracing = env::var("TRACE").is_ok();
    let first_trace = env::var("TRACEFROM").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let dump_ppu = env::var("PAL").is_ok();
    let dump_pram = env::var("PRAM").is_ok();

    for f in 0..frames {
        while si < script.len() && script[si].0 == f {
            keys = script[si].1;
            si += 1;
        }
        n.keys = keys;
        n.trace = tracing && f >= first_trace;
        n.ppu.trace2 = n.trace;
        n.ppu.trace_sl = env::var("SL").ok().and_then(|v| v.parse().ok()).unwrap_or(32);
        n.run_frame();
        if env::var("APUD").is_ok() {
            eprintln!("f{} mixer={:.4} p0len={} p0per={} p0vol={} p0duty={} p1len={} trlen={} nzlen={} dmcout={}",
                f, n.apu.mixer(), n.apu.pulse[0].length, n.apu.pulse[0].period, n.apu.pulse[0].volume, n.apu.pulse[0].duty, n.apu.pulse[1].length, n.apu.triangle.length, n.apu.noise.length, 0);
        }
        n.trace = false;
        n.ppu.trace2 = false;

        if f >= start_frame {
            let fb = &n.ppu.framebuffer;
            let mut bytes = Vec::with_capacity(122880);
            for v in fb.iter() {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
            std::fs::write(format!("{}/frame_{:05}.idx", outdir, f), &bytes).unwrap();
        }

        if f + 1 == frames {
            if dump_ppu {
                let nz: Vec<usize> = (0..n.chr.len()).filter(|k| n.chr[*k] != 0).collect();
                let mut rs: Vec<(usize, usize)> = Vec::new();
                for k in nz.iter() {
                    if let Some(l) = rs.last_mut() {
                        if l.1 + 1 == *k {
                            l.1 = *k;
                            continue;
                        }
                    }
                    rs.push((*k, *k));
                }
                let nt_nz: Vec<usize> = (0..2048).filter(|k| n.ppu.vram[*k] != 0).collect();
                let mut rs2: Vec<(usize, usize)> = Vec::new();
                for k in nt_nz.iter() {
                    if let Some(l) = rs2.last_mut() {
                        if l.1 + 1 == *k {
                            l.1 = *k;
                            continue;
                        }
                    }
                    rs2.push((*k, *k));
                }
                eprintln!(
                    "mask={:02X} ctrl={:02X} pal={:02X?} chr20={:02X?} banks={:02X?} ptrnull={}",
                    n.ppu.mask,
                    n.ppu.ctrl,
                    n.ppu.palette,
                    (0..16).map(|k| n.ppu.chr_read(0x200 + k as u16)).collect::<Vec<_>>(),
                    n.ppu.chr_banks,
                    n.ppu.chr_writable.is_null()
                );
                eprintln!("CHRnz ranges: {:?} ptr_w={:x} chr={:x} len={} first={:02X?}", &rs[..rs.len().min(40)], n.ppu.chr_writable as usize, n.chr.as_ptr() as usize, n.chr.len(), &n.chr[..16]);
                eprintln!("VRAMnz ranges: {:?}", &rs2[..rs2.len().min(20)]);
                for r in 0..30u16 {
                    eprintln!("nt row {}: {:?}", r, (0..32).map(|c| n.ppu.vram[(r * 32 + c) as usize]).collect::<Vec<_>>());
                }
                eprintln!("attr: {:?}", &n.ppu.vram[0x3C0..0x3D0]);
                for a in [0x530u16,0x410,0x220] { eprintln!("chr {:04X}: {:02X?}", a, (0..16).map(|k| n.chr[(a as usize)+k]).collect::<Vec<_>>()); }
            }
            if dump_pram {
                let pr = &n.mapper.prg_ram;
                let mut s = String::new();
                let mut i = 0;
                while i < pr.len() {
                    if pr[i] == 0 {
                        i += 1;
                        continue;
                    }
                    let st = i;
                    while i < pr.len() && pr[i] != 0 {
                        i += 1;
                    }
                    s.push_str(&String::from_utf8_lossy(&pr[st..i]));
                    s.push('\n');
                }
                eprintln!("PRAM:\n{}", s);
            }
            let mut wav = Vec::new();
            let all: Vec<i16> = n.audio_all.clone();
            wav.extend_from_slice(b"RIFF");
            let nsamp = all.len() as u32 * 2;
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
            for s in all.iter() {
                wav.extend_from_slice(&s.to_le_bytes());
            }
            std::fs::write(format!("{}/audio.wav", outdir), &wav).unwrap();
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