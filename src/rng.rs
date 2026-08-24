//! Deterministic RNG so JS and Rust can be driven by the same seed for
//! behavioral parity testing. SplitMix64 -> f64/f32.
//!
//! The original JS uses `Math.random()`. For parity tests we inject the same
//! seed both sides; for production the sim can be seeded from entropy.

pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        // Avoid a zero state which would stick for some generators.
        Rng {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1). Matches the semantics of Math.random().
    #[inline]
    pub fn next_f64(&mut self) -> f64 {
        // 53 bits of mantissa.
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    #[inline]
    pub fn next_f32(&mut self) -> f32 {
        self.next_f64() as f32
    }

    /// Uniform integer in [0, n). Mirrors `(Math.random()*n)|0`.
    #[inline]
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_f64() * n as f64) as usize
    }

    /// Standard normal via Box-Muller, matching the JS `gs()` helper:
    /// sqrt(-2 ln u) * cos(2π v).
    pub fn gaussian(&mut self) -> f32 {
        let mut u = 0.0f64;
        while u == 0.0 {
            u = self.next_f64();
        }
        let v = self.next_f64();
        ((-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()) as f32
    }
}
