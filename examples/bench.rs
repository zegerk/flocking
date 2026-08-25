//! Native micro-benchmarks for the sim hot paths. Min-of-runs timing, which is
//! robust against scheduling noise. Run: cargo run --release --example bench

use std::hint::black_box;
use std::time::Instant;

use flocking::sim::Sim;

fn bench<F: FnMut()>(label: &str, mut f: F, rounds: usize, iters: usize) {
    // Warmup
    for _ in 0..iters {
        f();
    }
    let mut best = f64::INFINITY;
    for _ in 0..rounds {
        let t0 = Instant::now();
        for _ in 0..iters {
            f();
        }
        let per = t0.elapsed().as_secs_f64() * 1000.0 / iters as f64;
        if per < best {
            best = per;
        }
    }
    println!("{label}: {best:.3} ms");
}

fn main() {
    let n = 100_000;

    let mut sim = Sim::new(n, 3, 12345);
    sim.law_prop = false;
    bench("step_unit_100k_3d", || black_box(&mut sim).step(), 8, 30);

    let mut sim = Sim::new(n, 3, 12345);
    sim.law_prop = true;
    bench("step_prop_100k_3d", || black_box(&mut sim).step(), 8, 30);

    let mut sim = Sim::new(n, 3, 12345);
    bench("analyse_100k_3d", || black_box(&mut sim).analyse(), 8, 10);

    let mut sim = Sim::new(n, 3, 12345);
    bench("meas_100k_3d", || black_box(&mut sim).meas_public(), 8, 30);

    let mut sim = Sim::new(n, 3, 12345);
    bench("capture_trail_100k_3d", || black_box(&mut sim).capture_trail_frame(), 8, 30);

    let mut sim = Sim::new(20_000, 24, 7);
    sim.law_prop = false;
    bench("step_unit_20k_24d", || black_box(&mut sim).step(), 8, 15);

    let mut sim = Sim::new(1_000_000, 3, 42);
    sim.law_prop = false;
    bench("step_unit_1M_3d", || black_box(&mut sim).step(), 5, 5);
}
