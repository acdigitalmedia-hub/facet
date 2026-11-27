//! Small global PRNG (xorshift64*) so the simulation doesn't depend on a graphics library.
//! Seedable for reproducible bot runs (tests that seed it take turns through a lock in
//! `game::tests`).

use std::sync::atomic::{AtomicU64, Ordering};

static STATE: AtomicU64 = AtomicU64::new(0x2545_F491_4F6C_DD1D);

pub fn srand(seed: u64) {
    STATE.store(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1, Ordering::Relaxed);
}

fn next() -> u64 {
    // Only the simulation uses this, from one thread at a time, so a plain load/store is fine.
    let mut x = STATE.load(Ordering::Relaxed);
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    STATE.store(x, Ordering::Relaxed);
    x.wrapping_mul(0x2545_F491_4F6C_DD1D)
}

/// Uniform in [0, 1).
pub fn unit() -> f32 {
    (next() >> 40) as f32 / (1u64 << 24) as f32
}

pub trait RandomRange: Sized {
    fn gen_range(low: Self, high: Self) -> Self;
}

impl RandomRange for f32 {
    fn gen_range(low: f32, high: f32) -> f32 {
        low + (high - low) * unit()
    }
}

macro_rules! int_range {
    ($($t:ty),*) => {$(
        impl RandomRange for $t {
            fn gen_range(low: $t, high: $t) -> $t {
                if high <= low { return low; }
                let span = (high as i128 - low as i128) as u64;
                (low as i128 + (next() % span) as i128) as $t
            }
        }
    )*};
}
int_range!(i32, u32, usize, u64);

/// Uniform in [low, high) (matches macroquad's semantics).
pub fn gen_range<T: RandomRange>(low: T, high: T) -> T {
    T::gen_range(low, high)
}
