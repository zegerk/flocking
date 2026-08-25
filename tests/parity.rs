//! Guards simulation semantics against performance refactors. Any change to
//! `Sim` that alters observable state — including float operation order — moves
//! a hash and fails here.
//!
//! To re-bless after a deliberate change: `cargo run --release --example parity`
//! and paste the printed table below, explaining the change in the commit.

use flocking::parity::{run, SCENARIOS};

const EXPECTED: &[(&str, u64)] = &[
    ("2d_unit", 0xbf2ad79377075f0c),
    ("2d_prop", 0x3a26578073b3b5b1),
    ("3d_unit", 0x6cb502db42606ec5),
    ("3d_prop", 0x762492657c488049),
    ("4d_unit", 0xf57137409839bbeb),
    ("4d_prop", 0x1dad09e74b690469),
    ("5d_unit", 0xd98385b60ea07e3b),
    ("5d_prop", 0x4da6346d96b27f1b),
    ("8d_unit", 0x22b0a3005cc87f9a),
    ("8d_prop", 0xa2cc8ad5de10d057),
    ("24d_unit", 0x46b8b7eefca63116),
    ("24d_prop", 0x57d858ef6e786cff),
    ("tiny_4d_prop", 0x9a19741dbfd52c9d),
];

#[test]
fn every_scenario_is_covered_by_an_expected_hash() {
    let names: Vec<&str> = SCENARIOS.iter().map(|s| s.name).collect();
    let expected: Vec<&str> = EXPECTED.iter().map(|(name, _)| *name).collect();
    assert_eq!(names, expected, "SCENARIOS and EXPECTED are out of sync");
}

#[test]
fn simulation_state_is_bit_identical_to_the_blessed_baseline() {
    for (scenario, (name, want)) in SCENARIOS.iter().zip(EXPECTED) {
        let got = run(scenario).hash;
        assert_eq!(
            got, *want,
            "parity drift in scenario {name}: got {got:016x}, want {want:016x}"
        );
    }
}

#[test]
fn scenarios_are_deterministic_across_repeated_runs() {
    for scenario in SCENARIOS {
        assert_eq!(run(scenario).hash, run(scenario).hash, "{}", scenario.name);
    }
}
