//! Prints the fixed-seed parity hashes. The assertions live in
//! `tests/parity.rs`; this binary exists to read the numbers when a hash moves
//! and to re-bless the expected table after a deliberate semantic change.
//!
//! Run: cargo run --release --example parity

use flocking::parity::{fnv1a, run, FNV_OFFSET, SCENARIOS};

fn main() {
    let mut all = FNV_OFFSET;
    let mut hashes = Vec::new();
    for s in SCENARIOS {
        let outcome = run(s);
        println!(
            "{:<13} n={:<5} dim={:<3} prop={:<6} steps={:<4} hash={:016x} spread={:.9}",
            s.name, s.n, s.dim, s.law_prop, s.steps, outcome.hash, outcome.spread
        );
        all = fnv1a(&outcome.hash.to_le_bytes(), all);
        hashes.push((s.name, outcome.hash));
    }
    println!("COMBINED: {all:016x}");
    println!("\n// paste into tests/parity.rs EXPECTED:");
    for (name, hash) in hashes {
        println!("    (\"{name}\", 0x{hash:016x}),");
    }
}

