//! Simulation core: positions, friend/enemy forces, centre pull, trail ring
//! buffer, and the functional-graph analyse (components / cycle / depth).

use crate::rng::Rng;
use crate::tour::{Tour, AMB};

pub const STR: usize = 5;
pub const MIN_TRAIL_FRAMES: usize = 2;
pub const DEFAULT_TRAIL_FRAMES: usize = 30;
pub const MAX_TRAIL_FRAMES: usize = 120;
pub const TRAIL_HISTORY_BYTE_BUDGET: usize = 256 * 1024 * 1024;
const MIN_POPULATION: usize = 3;
const MAX_POPULATION: usize = 1_000_000;
const MIN_DIMENSION: u32 = 2;
const MAX_DIMENSION: u32 = AMB as u32;

pub fn max_population_for_trail_length(frames: usize) -> usize {
    let frames = frames.clamp(MIN_TRAIL_FRAMES, MAX_TRAIL_FRAMES);
    (TRAIL_HISTORY_BYTE_BUDGET / (frames * STR * size_of::<f32>()))
        .clamp(MIN_POPULATION, MAX_POPULATION)
}

fn normalize_population(n: usize, trail_capacity: usize) -> usize {
    n.max(MIN_POPULATION)
        .min(max_population_for_trail_length(trail_capacity))
}

fn normalize_dimension(dim: u32) -> u32 {
    dim.clamp(MIN_DIMENSION, MAX_DIMENSION)
}

#[inline]
fn clamp_pos(t: f32) -> f32 {
    if t.is_finite() {
        t.clamp(-1e6, 1e6)
    } else {
        0.0
    }
}

pub struct Sim {
    pub n: usize,
    pub dim: u32,

    // Forces / params
    pub f: f32,   // to friend (capped)
    pub e: f32,   // from enemy (constant magnitude)
    pub c: f32,   // to centre
    pub iv: u32,  // re-pick interval (frames)
    pub leg: f32, // tour leg length (seconds)
    pub slab_h: f32,
    pub slab_c: f32,
    pub tour_on: bool,
    pub law_prop: bool, // true = proportional step, false = fixed-length (unit)
    pub speed: u32,     // steps per frame

    // Position arrays (5 dims, only first `dim` used meaningfully).
    pub xs: Vec<f32>,
    pub ys: Vec<f32>,
    pub zs: Vec<f32>,
    pub ws: Vec<f32>,
    pub vs: Vec<f32>,

    // Accel / next-position temps.
    ax: Vec<f32>,
    ay: Vec<f32>,
    az: Vec<f32>,
    aw: Vec<f32>,
    av: Vec<f32>,

    // Graph.
    pub fr: Vec<i32>,
    pub en: Vec<i32>,
    hu: Vec<u8>,
    pub flash: Vec<f32>,

    // Analyse outputs.
    comp: Vec<i32>,
    dep: Vec<i32>,
    onc: Vec<u8>,
    seen: Vec<u8>,
    pth: Vec<i32>,
    pub ncomp: u32,
    pub dmax: u32,
    /// Set by any re-pick; cleared by analyse(). Flock watches this to know
    /// when the cached colour buffer is stale.
    pub dirty: bool,

    // New "colour by" signals (computed in step/analyse).
    /// Per-dot squared displacement this frame (xs/ys/zs/ws/vs deltas²).
    /// Recomputed every Sim::step; consumed by mode 4 (speed).
    pub spd: Vec<f32>,
    /// Max of `spd` over the last step; used to normalize mode-4 bucketing.
    pub spd_max: f32,
    /// Cycle length of the component each dot belongs to. Filled in analyse.
    /// Off-cycle nodes inherit their component's cycle length.
    pub cyc_len: Vec<u8>,
    /// Number of nodes per component id (sized ncomp; rebuilt in analyse).
    pub comp_size: Vec<u32>,
    /// Per-dot in-degree bucket from `fr` (0 / 1 / 2 / ≥3). Filled in analyse.
    pub ind: Vec<u8>,
    /// Maximum in-degree bucket currently observed (for the legend). 0..=3.
    pub ind_max: u8,

    // Trail ring buffer: trail_capacity slots × n × STR.
    pub hist: Vec<f32>,
    trail_capacity: usize,
    head: usize,
    hlen: usize,

    pub steps: u64,
    pub spread: f32,

    pub rng: Rng,
    pub tour: Tour,
}

impl Sim {
    pub fn new(n: usize, dim: u32, seed: u64) -> Self {
        let trail_capacity = DEFAULT_TRAIL_FRAMES;
        let n = normalize_population(n, trail_capacity);
        let dim = normalize_dimension(dim);
        let mut s = Sim {
            n,
            dim,
            f: 0.012,
            e: 0.005,
            c: 0.004,
            iv: 60,
            leg: 7.0,
            slab_h: 2.0,
            slab_c: 0.0,
            tour_on: false,
            law_prop: false,
            speed: 1,
            xs: vec![0.0; n],
            ys: vec![0.0; n],
            zs: vec![0.0; n],
            ws: vec![0.0; n],
            vs: vec![0.0; n],
            ax: vec![0.0; n],
            ay: vec![0.0; n],
            az: vec![0.0; n],
            aw: vec![0.0; n],
            av: vec![0.0; n],
            fr: vec![0; n],
            en: vec![0; n],
            hu: vec![0; n],
            flash: vec![0.0; n],
            comp: vec![0; n],
            dep: vec![0; n],
            onc: vec![0; n],
            seen: vec![0; n],
            pth: vec![0; n],
            ncomp: 0,
            dmax: 1,
            dirty: true,
            spd: vec![0.0; n],
            spd_max: 0.0,
            cyc_len: vec![0; n],
            comp_size: Vec::new(),
            ind: vec![0; n],
            ind_max: 0,
            hist: vec![0.0; n * trail_capacity * STR],
            trail_capacity,
            head: 0,
            hlen: 0,
            steps: 0,
            spread: 0.0,
            rng: Rng::new(seed),
            tour: Tour::new(),
        };
        s.tour.reset(dim);
        s.build();
        s
    }

    pub fn resize(&mut self, n: usize) {
        self.n = normalize_population(n, self.trail_capacity);
        self.build();
    }

    pub fn set_trail_capacity(&mut self, frames: usize) {
        let capacity = frames.clamp(MIN_TRAIL_FRAMES, MAX_TRAIL_FRAMES);
        let max_population = max_population_for_trail_length(capacity);
        if self.n > max_population {
            self.n = max_population;
            self.trail_capacity = capacity;
            self.build();
            return;
        }
        self.trail_capacity = capacity;
        self.hist = vec![0.0; self.n * self.trail_capacity * STR];
        self.head = 0;
        self.hlen = 0;
    }

    pub fn pick(&mut self, i: usize) {
        let n = self.n;
        let mut f = i;
        while f == i {
            f = self.rng.below(n);
        }
        let mut e = i;
        while e == i || e == f {
            e = self.rng.below(n);
        }
        self.fr[i] = f as i32;
        self.en[i] = e as i32;
        self.flash[i] = 1.0;
        self.dirty = true;
    }

    pub fn repick_all(&mut self) {
        for i in 0..self.n {
            self.pick(i);
        }
    }

    pub fn build(&mut self) {
        let n = self.n;
        self.xs = vec![0.0; n];
        self.ys = vec![0.0; n];
        self.zs = vec![0.0; n];
        self.ws = vec![0.0; n];
        self.vs = vec![0.0; n];
        self.ax = vec![0.0; n];
        self.ay = vec![0.0; n];
        self.az = vec![0.0; n];
        self.aw = vec![0.0; n];
        self.av = vec![0.0; n];
        self.fr = vec![0; n];
        self.en = vec![0; n];
        self.hu = vec![0; n];
        self.flash = vec![0.0; n];
        self.comp = vec![0; n];
        self.dep = vec![0; n];
        self.onc = vec![0; n];
        self.seen = vec![0; n];
        self.pth = vec![0; n];
        self.spd = vec![0.0; n];
        self.spd_max = 0.0;
        self.cyc_len = vec![0; n];
        self.comp_size = Vec::new();
        self.ind = vec![0; n];
        self.ind_max = 0;
        self.hist = vec![0.0; n * self.trail_capacity * STR];
        self.head = 0;
        self.hlen = 0;

        let pal_len = 5usize; // PAL.length in JS
        for i in 0..n {
            self.xs[i] = self.rng.next_f32() - 0.5;
            self.ys[i] = self.rng.next_f32() - 0.5;
            self.zs[i] = if self.dim < 3 {
                0.0
            } else {
                self.rng.next_f32() - 0.5
            };
            self.ws[i] = if self.dim < 4 {
                0.0
            } else {
                self.rng.next_f32() - 0.5
            };
            self.vs[i] = if self.dim < 5 {
                0.0
            } else {
                self.rng.next_f32() - 0.5
            };
            self.hu[i] = self.rng.below(pal_len) as u8;
            self.pick(i);
            self.flash[i] = 0.0;
        }
        self.dirty = true;
        self.steps = 0;
        self.meas();
    }

    pub fn set_dim(&mut self, dim: u32) {
        self.dim = normalize_dimension(dim);
        self.tour.reset(self.dim);
        self.build();
    }

    fn meas(&mut self) {
        let mut s = 0.0f32;
        for i in 0..self.n {
            s += self.xs[i] * self.xs[i]
                + self.ys[i] * self.ys[i]
                + self.zs[i] * self.zs[i]
                + self.ws[i] * self.ws[i]
                + self.vs[i] * self.vs[i];
        }
        self.spread = (s / self.n as f32).sqrt();
    }

    pub fn step(&mut self) {
        let (a, b, c, n) = (self.f, self.e, self.c, self.n);
        let prop = self.law_prop;
        let flat = self.dim == 2;
        let k4 = self.dim >= 4;
        let k5 = self.dim >= 5;
        // Per-frame maximum resets each step so colour mode 4 reflects the
        // current frame's speed distribution rather than a growing envelope.
        let mut frame_spd_max = 0.0f32;

        for i in 0..n {
            let x = self.xs[i];
            let y = self.ys[i];
            let z = self.zs[i];
            let w = self.ws[i];
            let v = self.vs[i];
            let f = self.fr[i] as usize;
            let e = self.en[i] as usize;

            let mut dx = -c * x;
            let mut dy = -c * y;
            let mut dz = -c * z;
            let mut dw = -c * w;
            let mut dvv = -c * v;

            let fx = self.xs[f] - x;
            let fy = self.ys[f] - y;
            let fz = self.zs[f] - z;
            let fw = self.ws[f] - w;
            let fv = self.vs[f] - v;
            let ex = self.xs[e] - x;
            let ey = self.ys[e] - y;
            let ez = self.zs[e] - z;
            let ew = self.ws[e] - w;
            let ev = self.vs[e] - v;

            if !prop {
                // fixed-length: friend capped at min(a,d), enemy constant b
                let d = (fx * fx + fy * fy + fz * fz + fw * fw + fv * fv).sqrt();
                if d > 1e-9 {
                    let s1 = a.min(d) / d;
                    dx += s1 * fx;
                    dy += s1 * fy;
                    dz += s1 * fz;
                    dw += s1 * fw;
                    dvv += s1 * fv;
                }
                let d = (ex * ex + ey * ey + ez * ez + ew * ew + ev * ev).sqrt();
                if d > 1e-9 {
                    let s2 = b / d;
                    dx -= s2 * ex;
                    dy -= s2 * ey;
                    dz -= s2 * ez;
                    dw -= s2 * ew;
                    dvv -= s2 * ev;
                }
            } else {
                // proportional: step scales with gap (linear, unbounded)
                dx += a * fx - b * ex;
                dy += a * fy - b * ey;
                dz += a * fz - b * ez;
                dw += a * fw - b * ew;
                dvv += a * fv - b * ev;
            }

            self.ax[i] = x + dx;
            self.ay[i] = y + dy;
            self.az[i] = if flat { 0.0 } else { z + dz };
            self.aw[i] = if k4 { w + dw } else { 0.0 };
            self.av[i] = if k5 { v + dvv } else { 0.0 };
            // Squared step magnitude (only active dims contribute: dim 2/4/5
            // force dz/dvv to 0 above). Consumed by colour mode 4 (speed).
            let s = dx * dx + dy * dy + dz * dz + dw * dw + dvv * dvv;
            self.spd[i] = s;
            if s > frame_spd_max {
                frame_spd_max = s;
            }
            self.flash[i] *= 0.93;
        }
        self.spd_max = frame_spd_max;

        for i in 0..n {
            self.xs[i] = clamp_pos(self.ax[i]);
            self.ys[i] = clamp_pos(self.ay[i]);
            self.zs[i] = clamp_pos(self.az[i]);
            self.ws[i] = clamp_pos(self.aw[i]);
            self.vs[i] = clamp_pos(self.av[i]);
        }

        if self.rng.next_f64() < 1.0 / self.iv as f64 {
            let i = self.rng.below(n);
            self.pick(i);
        }

        self.steps += 1;
    }

    pub fn capture_trail_frame(&mut self) {
        let base = self.head * self.n * STR;
        for i in 0..self.n {
            let o = base + i * STR;
            self.hist[o] = self.xs[i];
            self.hist[o + 1] = self.ys[i];
            self.hist[o + 2] = self.zs[i];
            self.hist[o + 3] = self.ws[i];
            self.hist[o + 4] = self.vs[i];
        }
        self.head = (self.head + 1) % self.trail_capacity;
        if self.hlen < self.trail_capacity {
            self.hlen += 1;
        }
    }

    pub fn meas_public(&mut self) {
        self.meas();
    }

    /// Functional-graph analysis: components, on-cycle flag, depth-to-cycle,
    /// cycle length per component, component size counts, and in-degree
    /// buckets. All populated in one pass over the friend graph (`fr`).
    pub fn analyse(&mut self) {
        let n = self.n;
        self.seen.fill(0);
        // In-degree buckets from fr (overwritten before the traversal loop).
        // fr[i] = friend of i; we count how many dots cite each friend.
        let mut indeg = vec![0u32; n];
        for i in 0..n {
            let f = self.fr[i] as usize;
            if f < n {
                indeg[f] = indeg[f].saturating_add(1);
            }
        }
        let mut ind_max = 0u8;
        for (&degree, bucket) in indeg.iter().zip(self.ind.iter_mut()) {
            let b = match degree {
                0 => 0u8,
                1 => 1u8,
                2 => 2u8,
                _ => 3u8,
            };
            *bucket = b;
            if b > ind_max {
                ind_max = b;
            }
        }
        self.ind_max = ind_max;

        let mut nc = 0i32;
        let mut dm = 0i32;

        for s0 in 0..n {
            if self.seen[s0] != 0 {
                continue;
            }
            let mut len = 0usize;
            let mut v = s0;
            while self.seen[v] == 0 {
                self.seen[v] = 1;
                self.pth[len] = v as i32;
                len += 1;
                v = self.fr[v] as usize;
            }
            let mut start = len;
            let mut cycle_len = 0u8;
            if self.seen[v] == 1 {
                let mut k = 0usize;
                while self.pth[k] as usize != v {
                    k += 1;
                }
                let c0 = nc;
                nc += 1;
                cycle_len = (len - k).min(255) as u8;
                for j in k..len {
                    let u0 = self.pth[j] as usize;
                    self.comp[u0] = c0;
                    self.dep[u0] = 0;
                    self.onc[u0] = 1;
                    self.cyc_len[u0] = cycle_len;
                }
                start = k;
            }
            let cid = self.comp[if start < len {
                self.pth[start] as usize
            } else {
                v
            }];
            // Off-cycle nodes inherit their component's cycle length so the
            // whole chain carries one colour under mode 6.
            for j in (0..start).rev() {
                let u = self.pth[j] as usize;
                let nx2 = if j + 1 < start {
                    self.pth[j + 1] as usize
                } else if start < len {
                    self.pth[start] as usize
                } else {
                    v
                };
                self.comp[u] = cid;
                self.onc[u] = 0;
                self.cyc_len[u] = cycle_len;
                self.dep[u] = self.dep[nx2] + 1;
                if self.dep[u] > dm {
                    dm = self.dep[u];
                }
            }
            for j in 0..len {
                self.seen[self.pth[j] as usize] = 2;
            }
        }
        self.ncomp = nc as u32;
        self.dmax = dm.max(1) as u32;
        // Component sizes indexed by comp id; rebuilt each analyse.
        self.comp_size = vec![0u32; self.ncomp as usize];
        for i in 0..n {
            let c = self.comp[i];
            if c >= 0 && (c as usize) < self.comp_size.len() {
                self.comp_size[c as usize] = self.comp_size[c as usize].saturating_add(1);
            }
        }
        self.dirty = false;
    }

    /// Colour index for a dot under a colour mode.
    /// mode: 0=random 1=component 2=depth 3=cycle 4=speed 5=comp-size
    ///       6=cycle-length 7=in-degree 8=birth-order
    /// All arms clamp to the caller's palette length via `% pal_len` semantics;
    /// the Saturating/`min(idx, len-1)` guards below mean a wide palette vs a
    /// narrow one both render safely. Bucket counts match the JS palette
    /// definitions: speed/depth/ramp ⇒ 7 (RAMP), comps ⇒ 8, comp-size ⇒ 3,
    /// cycle-length ⇒ 4, in-degree ⇒ 4, cycle binary ⇒ 2, random ⇒ 5.
    pub fn color_of(&self, i: usize, mode: u32) -> u8 {
        match mode {
            1 => (self.comp[i] % 8) as u8, // COMPS.len()==8
            2 => {
                // RAMP.len()==7
                let t = (self.dep[i] as f32 / self.dmax as f32).sqrt();
                let idx = (t * (7.0 - 0.001)).floor() as i32;
                idx.clamp(0, 6) as u8
            }
            3 => self.onc[i],
            4 => {
                // Speed: normalize by per-frame max, square-root bucket scale
                // (so the dot's *magnitude*, not its square, reads linearly),
                // 7 buckets like RAMP. If the sim is frozen (spd_max≈0) pin
                // to index 0 so the UI doesn't strobe NaN.
                let t = if self.spd_max > 1e-12 {
                    (self.spd[i] / self.spd_max).sqrt()
                } else {
                    0.0
                };
                let idx = (t * (7.0 - 0.001)).floor() as i32;
                idx.clamp(0, 6) as u8
            }
            5 => {
                // Component size: log-scale by component id's node count.
                // Bands: small (≤ ~4) / medium / large. 3 buckets.
                let c = self.comp[i].max(0) as usize;
                let s = if c < self.comp_size.len() {
                    self.comp_size[c] as f32
                } else {
                    0.0
                };
                // log2(1+s) ≤ ~2 → small, ≤ ~4 → medium, else large.

                if s <= 2.0 {
                    0u8
                } else if s <= 5.0 {
                    1u8
                } else {
                    2u8
                }
            }
            6 => {
                // Cycle length: one swatch per distinct length, capped at 8.
                // Lengths 2..=8 map to indices 0..=6 (length-2 offset by 2),
                // length ≥9 caps at 7. With COMPS palette of 8 colors every
                // distinct length ≤8 reads as its own colour; longer cycles
                // all share the last swatch (in a 2000-dot graph nearly the
                // whole population is in one long cycle anyway). Length 0
                // (no analyse has run yet) pins to 0.
                let l = self.cyc_len[i];
                if l < 2 {
                    0u8
                } else if l <= 8 {
                    l - 2
                } else {
                    7u8
                }
            }
            7 => {
                // In-degree bucket 0..=3. ind_max ∈ 0..3.
                self.ind[i].min(3)
            }
            8 => {
                // Birth order: gradient of initial dot index across 7 bands.
                let idx = if self.n > 0 {
                    (i as f32 * 7.0 / self.n as f32).floor() as i32
                } else {
                    0
                };
                idx.clamp(0, 6) as u8
            }
            _ => self.hu[i],
        }
    }

    /// Mirrors JS `touring()`: grand tour active only in 4D/5D with tour on.
    pub fn touring_active(&self) -> bool {
        self.tour_on && self.dim >= 4
    }

    // --- Accessors used by the JS glue / renderer ---

    /// Trail history slot count currently populated.
    pub fn trail_len(&self) -> usize {
        self.hlen
    }
    pub fn trail_head(&self) -> usize {
        self.head
    }
    pub fn trail_capacity(&self) -> usize {
        self.trail_capacity
    }
}

/// Pack the 5 per-dim arrays into one interleaved [x,y,z,w,v] * n buffer for a
/// single GL upload. The renderer wants a stride-5 layout.
pub fn pack_positions(sim: &Sim, out: &mut [f32]) {
    for i in 0..sim.n {
        let o = i * AMB;
        out[o] = sim.xs[i];
        out[o + 1] = sim.ys[i];
        out[o + 2] = sim.zs[i];
        out[o + 3] = sim.ws[i];
        out[o + 4] = sim.vs[i];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_graph_invariants(sim: &Sim) {
        assert_eq!(sim.fr.len(), sim.n);
        assert_eq!(sim.en.len(), sim.n);
        for i in 0..sim.n {
            let friend = sim.fr[i] as usize;
            let enemy = sim.en[i] as usize;
            assert!(friend < sim.n);
            assert!(enemy < sim.n);
            assert_ne!(friend, i);
            assert_ne!(enemy, i);
            assert_ne!(friend, enemy);
        }
    }

    #[test]
    fn normalizes_population_and_dimension() {
        for n in 0..3 {
            let sim = Sim::new(n, 1, 42);
            assert_eq!(sim.n, MIN_POPULATION);
            assert_eq!(sim.dim, MIN_DIMENSION);
        }

        let mut sim = Sim::new(8, 9, 42);
        assert_eq!(sim.dim, MAX_DIMENSION);
        sim.resize(1);
        assert_eq!(sim.n, MIN_POPULATION);
        sim.set_dim(0);
        assert_eq!(sim.dim, MIN_DIMENSION);
        sim.set_dim(99);
        assert_eq!(sim.dim, MAX_DIMENSION);
    }

    #[test]
    fn trail_history_is_dynamic_and_captured_once_per_frame() {
        let mut sim = Sim::new(8, 3, 42);
        assert_eq!(sim.trail_capacity(), DEFAULT_TRAIL_FRAMES);
        assert_eq!(sim.trail_len(), 0);

        for _ in 0..5 {
            sim.step();
        }
        assert_eq!(sim.trail_len(), 0);
        sim.capture_trail_frame();
        assert_eq!(sim.trail_len(), 1);

        sim.set_trail_capacity(1);
        assert_eq!(sim.trail_capacity(), MIN_TRAIL_FRAMES);
        assert_eq!(sim.trail_len(), 0);
        for _ in 0..3 {
            sim.capture_trail_frame();
        }
        assert_eq!(sim.trail_len(), MIN_TRAIL_FRAMES);
        assert_eq!(sim.trail_head(), 1);

        sim.set_trail_capacity(999);
        assert_eq!(sim.trail_capacity(), MAX_TRAIL_FRAMES);
        assert_eq!(sim.hist.len(), sim.n * MAX_TRAIL_FRAMES * STR);
    }

    #[test]
    fn trail_history_budget_limits_population() {
        assert_eq!(max_population_for_trail_length(2), 1_000_000);
        assert_eq!(max_population_for_trail_length(30), 447_392);
        assert_eq!(max_population_for_trail_length(120), 111_848);
    }

    #[test]
    fn preserves_graph_invariants_when_repicking() {
        let mut sim = Sim::new(64, 5, 42);
        assert_graph_invariants(&sim);
        sim.repick_all();
        assert_graph_invariants(&sim);
    }

    #[test]
    fn stepping_keeps_state_finite_and_consistent() {
        let mut sim = Sim::new(64, 5, 42);
        for _ in 0..100 {
            sim.step();
        }

        for values in [&sim.xs, &sim.ys, &sim.zs, &sim.ws, &sim.vs] {
            assert_eq!(values.len(), sim.n);
            assert!(values.iter().all(|value| value.is_finite()));
        }
        assert_eq!(sim.hist.len(), sim.n * DEFAULT_TRAIL_FRAMES * STR);
        assert_eq!(sim.spd.len(), sim.n);
        assert!(sim.spd.iter().all(|value| value.is_finite()));
        assert!(sim.spread.is_finite());
        assert_graph_invariants(&sim);
    }
}
