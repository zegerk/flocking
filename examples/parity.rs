//! Fixed-seed parity harness. Steps the sim through a deterministic script
//! and prints an FNV-1a hash of all observable state. Used to prove that
//! performance refactors do not change simulation semantics.
//!
//! Run: cargo run --release --example parity

use flocking::sim::Sim;

fn fnv1a(bytes: &[u8], mut h: u64) -> u64 {
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h
}

fn hash_f32s(h: u64, xs: &[f32]) -> u64 {
    xs.iter().fold(h, |acc, x| fnv1a(&x.to_bits().to_le_bytes(), acc))
}

fn scenario(n: usize, dim: u32, seed: u64, law_prop: bool, steps: u32) -> (u64, f32) {
    let mut sim = Sim::new(n, dim, seed);
    sim.law_prop = law_prop;
    for s in 0..steps {
        sim.step();
        // Exercise graph mutation + analysis mid-run.
        if s == steps / 3 {
            sim.repick_all();
        }
        if s == steps / 2 {
            sim.analyse();
        }
        if s % 10 == 0 {
            sim.capture_trail_frame();
        }
    }
    sim.analyse();
    sim.meas_public();

    let mut h = 0xcbf2_9ce4_8422_2325u64;
    h = hash_f32s(h, &sim.pos);
    h = hash_f32s(h, &sim.spd);
    h = fnv1a(&sim.spread.to_bits().to_le_bytes(), h);
    h = fnv1a(&sim.spd_max.to_bits().to_le_bytes(), h);
    h = fnv1a(&(sim.steps as u64).to_le_bytes(), h);
    h = fnv1a(&(sim.ncomp as u64).to_le_bytes(), h);
    h = fnv1a(&(sim.dmax as u64).to_le_bytes(), h);
    h = fnv1a(&sim.ind_max.to_le_bytes(), h);
    // Colour signals feed the renderer; include them.
    h = fnv1a(&sim.cyc_len, h);
    h = fnv1a(&sim.ind, h);
    (h, sim.spread)
}

fn main() {
    let mut all = 0xcbf2_9ce4_8422_2325u64;
    for &(n, dim, seed, prop, steps) in &[
        (500usize, 2u32, 42u64, false, 300u32),
        (500, 3, 42, false, 300),
        (500, 3, 42, true, 300),
        (500, 5, 7, false, 200),
        (500, 24, 7, false, 100),
        (64, 4, 1, true, 500),
    ] {
        let (h, spread) = scenario(n, dim, seed, prop, steps);
        println!("n={n} dim={dim} seed={seed} prop={prop} steps={steps}: hash={h:016x} spread={spread:.9}");
        all = fnv1a(&h.to_le_bytes(), all);
    }
    println!("COMBINED: {all:016x}");
}
