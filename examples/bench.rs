//! Native micro-benchmarks for the sim + wasm-facing hot paths. Min-of-runs
//! timing, which is robust against scheduling noise.
//!
//!   cargo run --release --example bench           # human readable
//!   cargo run --release --example bench -- --json # machine readable
//!
//! Every case declares a CONTROL it is normalized against. Absolute
//! milliseconds vary several-fold between machines and even between sessions
//! on one machine; the ratio to an untouched control does not. `perf.test.mjs`
//! asserts on the ratios, so the thresholds stay portable.

use std::hint::black_box;
use std::time::Instant;

use flocking::sim::Sim;
use flocking::Flock;

const ROUNDS: usize = 9;
/// Each timed segment is scaled to this long so short cases clear timer noise.
const TARGET_ROUND_MS: f64 = 15.0;

/// Controls must never be touched by an optimization phase — that is the whole
/// point. `Memcpy` tracks memory bandwidth, `Alu` a latency-bound float chain.
#[derive(Clone, Copy, PartialEq)]
enum Control {
    Memcpy,
    Alu,
}

impl Control {
    fn name(self) -> &'static str {
        match self {
            Control::Memcpy => "control_memcpy",
            Control::Alu => "control_alu",
        }
    }
}

struct Case {
    name: &'static str,
    control: Control,
    ms: f64,
    ratio: f64,
}

fn time_iters<F: FnMut()>(f: &mut F, iters: usize) -> f64 {
    let t0 = Instant::now();
    for _ in 0..iters {
        f();
    }
    t0.elapsed().as_secs_f64() * 1000.0 / iters as f64
}

/// Pick an iteration count that makes one round last ~TARGET_ROUND_MS.
fn calibrate<F: FnMut()>(f: &mut F, warmup: usize) -> usize {
    for _ in 0..warmup {
        f();
    }
    let one = time_iters(f, warmup.max(1));
    if one <= 0.0 {
        return 100_000;
    }
    ((TARGET_ROUND_MS / one).ceil() as usize).clamp(1, 5_000_000)
}

/// Latency-bound float chain: a machine-speed unit that no phase touches.
fn alu_control(xs: &[f32]) -> f32 {
    let mut acc = 0.0f32;
    for &x in xs {
        acc = acc.mul_add(0.999, x * 1.0001);
    }
    acc
}

struct Bench<'a> {
    cases: Vec<Case>,
    json: bool,
    memcpy: Box<dyn FnMut() + 'a>,
    alu: Box<dyn FnMut() + 'a>,
    memcpy_iters: usize,
    alu_iters: usize,
}

impl<'a> Bench<'a> {
    fn new(json: bool, src: &'a [f32], dst: &'a mut Vec<f32>) -> Self {
        let mut memcpy: Box<dyn FnMut() + 'a> =
            Box::new(move || black_box(&mut *dst).copy_from_slice(black_box(src)));
        let mut alu: Box<dyn FnMut() + 'a> = Box::new(move || {
            black_box(alu_control(black_box(src)));
        });
        let memcpy_iters = calibrate(&mut memcpy, 200);
        let alu_iters = calibrate(&mut alu, 20);
        Bench { cases: Vec::new(), json, memcpy, alu, memcpy_iters, alu_iters }
    }

    fn time_control(&mut self, control: Control) -> f64 {
        match control {
            Control::Memcpy => time_iters(&mut self.memcpy, self.memcpy_iters),
            Control::Alu => time_iters(&mut self.alu, self.alu_iters),
        }
    }

    /// Bracket every timed case between two control measurements and keep the
    /// best ratio. Timing a case and its control minutes apart let CPU
    /// frequency drift leak straight into the ratio, which fired the gate on an
    /// unchanged tree; bracketing cancels drift that is linear across a round.
    fn run<F: FnMut()>(&mut self, name: &'static str, control: Control, mut f: F, warmup: usize) {
        let iters = calibrate(&mut f, warmup);
        let mut best_ms = f64::INFINITY;
        let mut best_ratio = f64::INFINITY;
        for _ in 0..ROUNDS {
            let before = self.time_control(control);
            let ms = time_iters(&mut f, iters);
            let after = self.time_control(control);
            best_ms = best_ms.min(ms);
            best_ratio = best_ratio.min(ms / ((before + after) / 2.0));
        }
        if !self.json {
            println!("{name:<28} {best_ms:>10.4} ms {best_ratio:>10.4}x {}", control.name());
        }
        self.cases.push(Case { name, control, ms: best_ms, ratio: best_ratio });
    }

    fn emit_json(&self) {
        let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
        println!("{{");
        println!("  \"schema\": 2,");
        println!("  \"profile\": \"{profile}\",");
        println!("  \"cases\": [");
        for (i, c) in self.cases.iter().enumerate() {
            let comma = if i + 1 == self.cases.len() { "" } else { "," };
            println!(
                "    {{ \"name\": \"{}\", \"control\": \"{}\", \"ms\": {:.6}, \"ratio\": {:.6} }}{comma}",
                c.name,
                c.control.name(),
                c.ms,
                c.ratio
            );
        }
        println!("  ]");
        println!("}}");
    }
}

fn filled_flock(n: usize, dim: u32, frames: usize) -> Flock {
    let mut flock = Flock::new(n, dim, 12345);
    flock.set_trail_length(frames);
    for _ in 0..frames {
        flock.step();
        flock.capture_trail_frame();
    }
    flock
}

fn main() {
    let json = std::env::args().any(|a| a == "--json");
    let src = vec![1.0f32; 300_000];
    let mut dst = vec![0.0f32; 300_000];
    let mut b = Bench::new(json, &src, &mut dst);

    // Each case is scoped so its sim is dropped before the next one starts.
    // Left in one scope, the 1M case alone keeps ~260 MB of trail ring alive
    // and every later case measures under different memory pressure — that
    // made the two trail_geom cases drift 27% between runs.

    // --- sim step: the dominant per-frame cost ---
    {
        let mut sim = Sim::new(100_000, 3, 12345);
        sim.law_prop = false;
        b.run("step_unit_100k_3d", Control::Alu, || black_box(&mut sim).step(), 20);
    }
    {
        let mut sim = Sim::new(100_000, 3, 12345);
        sim.law_prop = true;
        b.run("step_prop_100k_3d", Control::Alu, || black_box(&mut sim).step(), 20);
    }
    {
        let mut sim = Sim::new(20_000, 24, 7);
        sim.law_prop = false;
        b.run("step_unit_20k_24d", Control::Alu, || black_box(&mut sim).step(), 15);
    }
    {
        // At 1M x 3 floats the working set is far past L3, so this one is
        // bandwidth-bound and belongs against the memcpy control.
        let mut sim = Sim::new(1_000_000, 3, 42);
        sim.law_prop = false;
        b.run("step_unit_1M_3d", Control::Memcpy, || black_box(&mut sim).step(), 3);
    }

    // --- graph analysis: pointer chasing, runs up to 10x/sec from the legend ---
    {
        let mut sim = Sim::new(100_000, 3, 12345);
        b.run(
            "analyse_dirty_100k_3d",
            Control::Memcpy,
            || {
                let s = black_box(&mut sim);
                s.repick_all();
                s.analyse();
            },
            5,
        );
    }
    {
        let mut sim = Sim::new(100_000, 3, 12345);
        sim.analyse();
        b.run("analyse_clean_100k_3d", Control::Memcpy, || black_box(&mut sim).analyse(), 5);
    }

    // --- streaming passes ---
    {
        let mut sim = Sim::new(100_000, 3, 12345);
        b.run("meas_100k_3d", Control::Memcpy, || black_box(&mut sim).meas_public(), 30);
    }
    {
        let mut sim = Sim::new(100_000, 3, 12345);
        b.run(
            "capture_trail_100k_3d",
            Control::Memcpy,
            || black_box(&mut sim).capture_trail_frame(),
            30,
        );
    }

    // --- camera: full n*dim min/max fit, every frame ---
    {
        let mut flock = Flock::new(100_000, 3, 12345);
        b.run(
            "camera_fit_100k_3d",
            Control::Memcpy,
            || black_box(&mut flock).update_camera(1920.0, 1080.0),
            30,
        );
    }
    {
        let mut flock = Flock::new(1_000_000, 3, 12345);
        b.run(
            "camera_fit_1M_3d",
            Control::Memcpy,
            || black_box(&mut flock).update_camera(1920.0, 1080.0),
            5,
        );
    }

    // --- colour cache: mode 4 dirties the whole buffer every step ---
    {
        let mut flock = Flock::new(100_000, 3, 12345);
        flock.set_col_mode(0);
        b.run(
            "step_sync_mode0_100k_3d",
            Control::Alu,
            || {
                let f = black_box(&mut flock);
                f.step();
                f.sync_colors();
            },
            20,
        );
    }
    {
        let mut flock = Flock::new(100_000, 3, 12345);
        flock.set_col_mode(4);
        b.run(
            "step_sync_mode4_100k_3d",
            Control::Alu,
            || {
                let f = black_box(&mut flock);
                f.step();
                f.sync_colors();
            },
            20,
        );
    }

    // --- trail geometry: rebuilt from the ring buffer every frame ---
    let palette = vec![0.5f32; 24];
    {
        let mut flock = filled_flock(20_000, 3, 30);
        b.run(
            "trail_geom_2k_of_20k_d30",
            Control::Memcpy,
            || {
                black_box(&mut flock).build_trail_geometry(black_box(&palette), 8, 2_000);
            },
            10,
        );
    }
    {
        let mut flock = filled_flock(20_000, 3, 120);
        b.run(
            "trail_geom_2k_of_20k_d120",
            Control::Memcpy,
            || {
                black_box(&mut flock).build_trail_geometry(black_box(&palette), 8, 2_000);
            },
            5,
        );
    }

    // --- uniform block: allocates a 606-float Vec every frame today ---
    {
        let flock = Flock::new(1_000, 24, 1);
        b.run(
            "uniforms_24d",
            Control::Alu,
            || {
                black_box(black_box(&flock).uniforms(1920.0, 1080.0));
            },
            500,
        );
    }

    if json {
        b.emit_json();
    }
}
