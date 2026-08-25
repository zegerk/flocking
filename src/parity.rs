//! Fixed-seed parity harness: deterministic scripted runs reduced to an FNV-1a
//! hash of all observable state. Shared by `examples/parity.rs` (prints the
//! hashes) and `tests/parity.rs` (asserts them), so a performance refactor that
//! quietly changes simulation semantics fails the build instead of relying on
//! someone eyeballing stdout.

use crate::sim::Sim;

pub struct Scenario {
    pub name: &'static str,
    pub n: usize,
    pub dim: u32,
    pub seed: u64,
    pub law_prop: bool,
    pub steps: u32,
}

/// One entry per dimension that the hot loops specialize on, in both force
/// laws. Sizes are deliberately small: `cargo test` builds in debug.
pub const SCENARIOS: &[Scenario] = &[
    Scenario { name: "2d_unit", n: 256, dim: 2, seed: 42, law_prop: false, steps: 200 },
    Scenario { name: "2d_prop", n: 256, dim: 2, seed: 42, law_prop: true, steps: 200 },
    Scenario { name: "3d_unit", n: 256, dim: 3, seed: 42, law_prop: false, steps: 200 },
    Scenario { name: "3d_prop", n: 256, dim: 3, seed: 42, law_prop: true, steps: 200 },
    Scenario { name: "4d_unit", n: 256, dim: 4, seed: 7, law_prop: false, steps: 200 },
    Scenario { name: "4d_prop", n: 256, dim: 4, seed: 7, law_prop: true, steps: 200 },
    Scenario { name: "5d_unit", n: 256, dim: 5, seed: 7, law_prop: false, steps: 200 },
    Scenario { name: "5d_prop", n: 256, dim: 5, seed: 7, law_prop: true, steps: 200 },
    Scenario { name: "8d_unit", n: 128, dim: 8, seed: 1, law_prop: false, steps: 120 },
    Scenario { name: "8d_prop", n: 128, dim: 8, seed: 1, law_prop: true, steps: 120 },
    Scenario { name: "24d_unit", n: 128, dim: 24, seed: 1, law_prop: false, steps: 120 },
    Scenario { name: "24d_prop", n: 128, dim: 24, seed: 1, law_prop: true, steps: 120 },
    // Tiny population exercises the MIN_POPULATION clamp and dense graphs.
    Scenario { name: "tiny_4d_prop", n: 4, dim: 4, seed: 3, law_prop: true, steps: 300 },
];

pub const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;

pub fn fnv1a(bytes: &[u8], mut h: u64) -> u64 {
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

pub fn hash_f32s(h: u64, xs: &[f32]) -> u64 {
    xs.iter().fold(h, |acc, x| fnv1a(&x.to_bits().to_le_bytes(), acc))
}

pub fn hash_i32s(h: u64, xs: &[i32]) -> u64 {
    xs.iter().fold(h, |acc, x| fnv1a(&x.to_le_bytes(), acc))
}

pub struct Outcome {
    pub hash: u64,
    pub spread: f32,
}

/// Drive one scenario through a fixed script and hash everything observable.
pub fn run(s: &Scenario) -> Outcome {
    let mut sim = Sim::new(s.n, s.dim, s.seed);
    sim.law_prop = s.law_prop;
    for step in 0..s.steps {
        sim.step();
        // Exercise graph mutation + analysis mid-run.
        if step == s.steps / 3 {
            sim.repick_all();
        }
        if step == s.steps / 2 {
            sim.analyse();
        }
        if step % 10 == 0 {
            sim.capture_trail_frame();
        }
    }
    sim.analyse();
    sim.meas_public();

    let mut h = FNV_OFFSET;
    h = hash_f32s(h, &sim.pos);
    h = hash_f32s(h, &sim.spd);
    h = hash_f32s(h, &sim.hist);
    h = hash_f32s(h, &sim.flash);
    h = hash_i32s(h, &sim.fr);
    h = hash_i32s(h, &sim.en);
    h = fnv1a(&sim.spread.to_bits().to_le_bytes(), h);
    h = fnv1a(&sim.spd_max.to_bits().to_le_bytes(), h);
    h = fnv1a(&sim.steps.to_le_bytes(), h);
    h = fnv1a(&sim.ncomp.to_le_bytes(), h);
    h = fnv1a(&sim.dmax.to_le_bytes(), h);
    h = fnv1a(&sim.graph_version.to_le_bytes(), h);
    h = fnv1a(&sim.ind_max.to_le_bytes(), h);
    h = fnv1a(&sim.cyc_max.to_le_bytes(), h);
    // Colour signals feed the renderer; include them.
    h = fnv1a(&sim.cyc_len, h);
    h = fnv1a(&sim.ind, h);
    for size in &sim.comp_size {
        h = fnv1a(&size.to_le_bytes(), h);
    }
    Outcome { hash: h, spread: sim.spread }
}

/// Fold every scenario hash into one number — the value to quote in a commit.
pub fn combined() -> u64 {
    SCENARIOS
        .iter()
        .fold(FNV_OFFSET, |acc, s| fnv1a(&run(s).hash.to_le_bytes(), acc))
}
