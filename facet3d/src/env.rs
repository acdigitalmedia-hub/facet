//! Generated textures: a sky cubemap (for reflections and soft ambient light) and a tiling
//! ground-detail texture.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension};

use crate::mesh::rnd;
use crate::terrain::{fbm, smoothstep};
use crate::theme::Theme;

fn f16_bits(x: f32) -> u16 {
    let b = x.to_bits();
    let sign = ((b >> 16) & 0x8000) as u16;
    let exp = ((b >> 23) & 0xff) as i32 - 127 + 15;
    let man = b & 0x7f_ffff;
    if exp <= 0 {
        return sign;
    }
    if exp >= 31 {
        return sign | 0x7c00;
    }
    sign | ((exp as u16) << 10) | ((man >> 13) as u16)
}

fn lin3(c: [f32; 3]) -> Vec3 {
    let l = Color::srgb(c[0], c[1], c[2]).to_linear();
    Vec3::new(l.red, l.green, l.blue)
}

/// A simple sky: zenith-to-horizon gradient, a bright sun, and dim ground below.
pub fn sky_cubemap(th: &Theme) -> Image {
    const N: usize = 64;
    let to_sun = {
        let (e, a) = (th.sun_angle.0.to_radians(), th.sun_angle.1.to_radians());
        Vec3::new(e.cos() * a.sin(), e.sin(), e.cos() * a.cos())
    };
    let horizon = lin3(th.fog) * 1.1;
    let zenith = lin3(th.fog).lerp(Vec3::new(0.12, 0.25, 0.6), 0.55);
    let ground = lin3(th.ground) * 0.5;
    let sun = lin3(th.sun);
    let mut data = Vec::with_capacity(N * N * 6 * 8);
    for face in 0..6 {
        for y in 0..N {
            for x in 0..N {
                let u = (x as f32 + 0.5) / N as f32 * 2.0 - 1.0;
                let v = (y as f32 + 0.5) / N as f32 * 2.0 - 1.0;
                let d = match face {
                    0 => Vec3::new(1.0, -v, -u),
                    1 => Vec3::new(-1.0, -v, u),
                    2 => Vec3::new(u, 1.0, v),
                    3 => Vec3::new(u, -1.0, -v),
                    4 => Vec3::new(u, -v, 1.0),
                    _ => Vec3::new(-u, -v, -1.0),
                }
                .normalize();
                let mut c = if d.y >= 0.0 { horizon.lerp(zenith, smoothstep(0.0, 0.6, d.y)) } else { horizon.lerp(ground, smoothstep(0.0, 0.15, -d.y)) };
                let s = d.dot(to_sun).max(0.0);
                c += sun * (s.powf(400.0) * 40.0 + s.powf(12.0) * 0.6);
                for ch in [c.x, c.y, c.z, 1.0] {
                    data.extend_from_slice(&f16_bits(ch).to_le_bytes());
                }
            }
        }
    }
    let mut img = Image::new(
        Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 6 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.texture_view_descriptor = Some(TextureViewDescriptor { dimension: Some(TextureViewDimension::Cube), ..default() });
    img
}

/// Grayscale detail (grass blades, pebbles) that tiles; multiplied with the terrain colors.
pub fn ground_detail() -> Image {
    const N: usize = 256;
    let mut lum = vec![0.0f32; N * N];
    let wrap = |x: i32| x.rem_euclid(N as i32) as usize;
    for y in 0..N {
        for x in 0..N {
            // Tileable noise: sample on a torus-like wrap by blending.
            let (fx, fy) = (x as f32 / N as f32, y as f32 / N as f32);
            let n = |sx: f32, sy: f32| fbm(sx * 16.0, sy * 16.0, 3);
            let a = n(fx, fy) * (1.0 - fx) * (1.0 - fy)
                + n(fx - 1.0, fy) * fx * (1.0 - fy)
                + n(fx, fy - 1.0) * (1.0 - fx) * fy
                + n(fx - 1.0, fy - 1.0) * fx * fy;
            lum[y * N + x] = 0.82 + (a - 0.5) * 0.35;
        }
    }
    // Short strokes: blades of grass / scratches.
    for k in 0..2600 {
        let x0 = (rnd(k, 1, 77) * N as f32) as i32;
        let y0 = (rnd(k, 2, 77) * N as f32) as i32;
        let len = 3 + (rnd(k, 3, 77) * 5.0) as i32;
        let lean = rnd(k, 4, 77) - 0.5;
        let bright = if rnd(k, 5, 77) > 0.5 { 0.16 } else { -0.14 };
        for s in 0..len {
            let px = wrap(x0 + (lean * s as f32) as i32);
            let py = wrap(y0 - s);
            let l = &mut lum[py * N + px];
            *l = (*l + bright * (1.0 - s as f32 / len as f32 * 0.5)).clamp(0.4, 1.2);
        }
    }
    // Mip levels (box filtered) so the detail doesn't shimmer in the distance.
    let mut data = Vec::new();
    let mut size = N;
    let mut level = lum;
    let mut levels = 0;
    loop {
        for &l in &level {
            let b = (l.min(1.0) * 255.0) as u8;
            data.extend_from_slice(&[b, b, b, 255]);
        }
        levels += 1;
        if size == 1 {
            break;
        }
        let half = size / 2;
        let mut next = vec![0.0; half * half];
        for y in 0..half {
            for x in 0..half {
                let s = |dx: usize, dy: usize| level[(y * 2 + dy) * size + x * 2 + dx];
                next[y * half + x] = (s(0, 0) + s(1, 0) + s(0, 1) + s(1, 1)) / 4.0;
            }
        }
        level = next;
        size = half;
    }
    let mut img = Image::new_uninit(
        Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.data = Some(data);
    img.texture_descriptor.mip_level_count = levels;
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        anisotropy_clamp: 8,
        ..ImageSamplerDescriptor::linear()
    });
    img
}
