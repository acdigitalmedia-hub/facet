//! The app icon, drawn procedurally: a hexagonal gem whose six facets are the six gem
//! colors, on a dark rounded tile. Also contains a tiny dependency-free PNG encoder.

const FACETS: [[f32; 3]; 6] = [
    [230.0, 50.0, 70.0],   // ruby
    [245.0, 190.0, 40.0],  // topaz
    [50.0, 200.0, 100.0],  // emerald
    [60.0, 120.0, 240.0],  // sapphire
    [160.0, 80.0, 220.0],  // amethyst
    [225.0, 235.0, 245.0], // diamond
];

/// Color (premultiplied RGBA, 0..1) of the icon at normalized coordinates (u, v).
fn sample(u: f32, v: f32) -> [f32; 4] {
    // Rounded-square tile
    let m = 0.04;
    let rad = 0.2;
    let qx = ((u - 0.5).abs() - (0.5 - m - rad)).max(0.0);
    let qy = ((v - 0.5).abs() - (0.5 - m - rad)).max(0.0);
    if (qx * qx + qy * qy).sqrt() > rad {
        return [0.0; 4];
    }
    let bg = [0.09, 0.10, 0.14];

    // Pointy-top hexagon
    let (dx, dy) = (u - 0.5, v - 0.52);
    let dist = (dx * dx + dy * dy).sqrt();
    let ang = dy.atan2(dx).to_degrees(); // -180..180, 0 = right, 90 = down
    let rel = (ang + 90.0).rem_euclid(360.0); // 0 = straight up
    let sector = (rel / 60.0) as usize % 6;
    let within = rel % 60.0 - 30.0;
    let hex_r = |r: f32| r * 30f32.to_radians().cos() / within.to_radians().cos();
    let outer = 0.38;
    let inner = 0.17;
    if dist > hex_r(outer) {
        // Soft glow around the gem
        let glow = (1.0 - (dist - hex_r(outer)) / 0.08).clamp(0.0, 1.0) * 0.35;
        let c = [0.45 * glow + bg[0] * (1.0 - glow), 0.78 * glow + bg[1] * (1.0 - glow), 1.0 * glow + bg[2] * (1.0 - glow)];
        return [c[0], c[1], c[2], 1.0];
    }
    let base = FACETS[sector].map(|x| x / 255.0);
    // Light from the top-left: facets facing it are brighter.
    let light = 0.75 + 0.25 * ((rel + 30.0 - 300.0).to_radians().cos());
    let mut c = base.map(|x| (x * light).min(1.0));
    if dist <= hex_r(inner) {
        // The flat "table" on top of the gem: brighter, tinted toward white
        c = c.map(|x| x * 0.55 + 0.45);
    }
    // Thin dark lines between facets
    let edge_ang = (within.abs() - 30.0).abs().to_radians().sin() * dist;
    if edge_ang < 0.006 || (dist - hex_r(inner)).abs() < 0.006 {
        c = c.map(|x| x * 0.55);
    }
    [c[0], c[1], c[2], 1.0]
}

/// Renders the icon as straight (non-premultiplied) RGBA8, `size` x `size`.
pub fn render(size: usize) -> Vec<u8> {
    let ss = 4;
    let mut out = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let mut acc = [0.0f32; 4];
            for sy in 0..ss {
                for sx in 0..ss {
                    let u = (x as f32 + (sx as f32 + 0.5) / ss as f32) / size as f32;
                    let v = (y as f32 + (sy as f32 + 0.5) / ss as f32) / size as f32;
                    let s = sample(u, v);
                    for k in 0..3 {
                        acc[k] += s[k] * s[3];
                    }
                    acc[3] += s[3];
                }
            }
            let n = (ss * ss) as f32;
            let a = acc[3] / n;
            for k in 0..3 {
                let c = if a > 0.0 { acc[k] / n / a } else { 0.0 };
                out.push((c.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
            out.push((a * 255.0).round() as u8);
        }
    }
    out
}

/// Fixed-size icon buffers for the window (16, 32 and 64 px).
#[allow(dead_code)]
pub fn window_icon() -> macroquad::miniquad::conf::Icon {
    let mut icon = macroquad::miniquad::conf::Icon { small: [0; 16 * 16 * 4], medium: [0; 32 * 32 * 4], big: [0; 64 * 64 * 4] };
    icon.small.copy_from_slice(&render(16));
    icon.medium.copy_from_slice(&render(32));
    icon.big.copy_from_slice(&render(64));
    icon
}

// ---------- Minimal PNG encoder (uncompressed deflate) ----------

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
}

#[allow(dead_code)]
pub fn png(size: usize, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(size * (size * 4 + 1));
    for row in rgba.chunks(size * 4) {
        raw.push(0); // no filter
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (k, b) in blocks.iter().enumerate() {
        z.push(if k + 1 == blocks.len() { 1 } else { 0 });
        let n = b.len() as u16;
        z.extend_from_slice(&n.to_le_bytes());
        z.extend_from_slice(&(!n).to_le_bytes());
        z.extend_from_slice(b);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(size as u32).to_be_bytes());
    ihdr.extend_from_slice(&(size as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}
