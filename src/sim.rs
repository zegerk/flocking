//! Simulation core: positions, friend/enemy forces, centre pull, trail ring
//! buffer, and the functional-graph analyse (components / cycle / depth).

use crate::rng::Rng;
use crate::tour::Tour;

pub const MIN_TRAIL_FRAMES: usize = 2;
pub const DEFAULT_TRAIL_FRAMES: usize = 30;
pub const MAX_TRAIL_FRAMES: usize = 120;
pub const TRAIL_HISTORY_BYTE_BUDGET: usize = 256 * 1024 * 1024;
const MIN_POPULATION: usize = 3;
const MAX_POPULATION: usize = 1_000_000;
pub const MIN_DIMENSION: u32 = 2;
pub const MAX_DIMENSION: u32 = 24;

pub fn max_population_for_trail_length(frames: usize, dim: u32) -> usize {
    let frames = frames.clamp(MIN_TRAIL_FRAMES, MAX_TRAIL_FRAMES);
    let dim = normalize_dimension(dim) as usize;
    (TRAIL_HISTORY_BYTE_BUDGET / (frames * dim * size_of::<f32>()))
        .clamp(MIN_POPULATION, MAX_POPULATION)
}

fn normalize_population(n: usize, trail_capacity: usize, dim: u32) -> usize {
    n.max(MIN_POPULATION)
        .min(max_population_for_trail_length(trail_capacity, dim))
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

    // Row-major positions: agent i occupies pos[i * dim..(i + 1) * dim].
    pub pos: Vec<f32>,
    next_pos: Vec<f32>,

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
    /// Bumped only on re-pick / rebuild. JS compares this to decide when the
    /// friend/enemy views need re-creating — it must NOT change on a plain
    /// step, or that cache never hits while running.
    pub graph_version: u32,

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
    /// Scratch in-degree counts reused across analyse() calls (no per-call alloc).
    indeg: Vec<u32>,
    /// Max cycle length from the latest analyse() (mode 6 legend).
    pub cyc_max: u8,

    // Trail ring buffer: trail_capacity slots × n × dim.
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
        let dim = normalize_dimension(dim);
        let n = normalize_population(n, trail_capacity, dim);
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
            pos: vec![0.0; n * dim as usize],
            next_pos: vec![0.0; n * dim as usize],
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
            graph_version: 0,
            spd: vec![0.0; n],
            spd_max: 0.0,
            cyc_len: vec![0; n],
            comp_size: Vec::new(),
            ind: vec![0; n],
            ind_max: 0,
            indeg: vec![0; n],
            cyc_max: 0,
            hist: vec![0.0; n * trail_capacity * dim as usize],
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
        self.n = normalize_population(n, self.trail_capacity, self.dim);
        self.build();
    }

    pub fn set_trail_capacity(&mut self, frames: usize) {
        let capacity = frames.clamp(MIN_TRAIL_FRAMES, MAX_TRAIL_FRAMES);
        let max_population = max_population_for_trail_length(capacity, self.dim);
        if self.n > max_population {
            self.n = max_population;
            self.trail_capacity = capacity;
            self.build();
            return;
        }
        self.trail_capacity = capacity;
        self.hist = vec![0.0; self.n * self.trail_capacity * self.dim as usize];
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
        self.graph_version = self.graph_version.wrapping_add(1);
    }

    pub fn repick_all(&mut self) {
        for i in 0..self.n {
            self.pick(i);
        }
    }

    pub fn build(&mut self) {
        let n = self.n;
        let dim = self.dim as usize;
        self.pos = vec![0.0; n * dim];
        self.next_pos = vec![0.0; n * dim];
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
        self.indeg = vec![0; n];
        self.cyc_max = 0;
        self.hist = vec![0.0; n * self.trail_capacity * dim];
        self.head = 0;
        self.hlen = 0;

        let pal_len = 5usize; // PAL.length in JS
        for i in 0..n {
            for value in &mut self.pos[i * dim..(i + 1) * dim] {
                *value = self.rng.next_f32() - 0.5;
            }
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
        self.n = normalize_population(self.n, self.trail_capacity, self.dim);
        self.tour.reset(self.dim);
        self.build();
    }

    fn meas(&mut self) {
        let s = self.pos.iter().map(|value| value * value).sum::<f32>();
        self.spread = (s / self.n as f32).sqrt();
    }

    pub fn step(&mut self) {
        let (a, b, c, n) = (self.f, self.e, self.c, self.n);
        let dim = self.dim as usize;
        let prop = self.law_prop;
        // Per-frame maximum resets each step so colour mode 4 reflects the
        // current frame's speed distribution rather than a growing envelope.
        let mut frame_spd_max = 0.0f32;

        // Field-level borrows keep the hot loop free of repeated `self.`
        // member access and let the optimizer see that pos/next_pos don't
        // alias.
        let pos = &self.pos;
        let next_pos = &mut self.next_pos;
        let fr = &self.fr;
        let en = &self.en;
        let spd = &mut self.spd;
        let flash = &mut self.flash;

        for i in 0..n {
            let f = fr[i] as usize;
            let e = en[i] as usize;
            let io = i * dim;
            let fo = f * dim;
            let eo = e * dim;
            let (friend_scale, enemy_scale) = if prop {
                (a, b)
            } else {
                let mut friend_dist_sq = 0.0;
                let mut enemy_dist_sq = 0.0;
                for k in 0..dim {
                    let friend_gap = pos[fo + k] - pos[io + k];
                    let enemy_gap = pos[eo + k] - pos[io + k];
                    friend_dist_sq += friend_gap * friend_gap;
                    enemy_dist_sq += enemy_gap * enemy_gap;
                }
                let friend_dist = friend_dist_sq.sqrt();
                let enemy_dist = enemy_dist_sq.sqrt();
                (
                    if friend_dist > 1e-9 { a.min(friend_dist) / friend_dist } else { 0.0 },
                    if enemy_dist > 1e-9 { b / enemy_dist } else { 0.0 },
                )
            };

            let mut s = 0.0;
            for k in 0..dim {
                let value = pos[io + k];
                let friend_gap = pos[fo + k] - value;
                let enemy_gap = pos[eo + k] - value;
                let delta = -c * value + friend_scale * friend_gap - enemy_scale * enemy_gap;
                next_pos[io + k] = value + delta;
                s += delta * delta;
            }
            spd[i] = s;
            if s > frame_spd_max {
                frame_spd_max = s;
            }
            flash[i] *= 0.93;
        }
        self.spd_max = frame_spd_max;

        // NOTE: measured against fusing this clamp into the compute loop +
        // swapping buffers — the separate pass is ~20-80% faster because it
        // stays a trivially vectorizable streaming loop, while clamping in
        // the gather-bound compute loop de-optimizes it (native A/B, 2026-08).
        for (position, &next) in self.pos.iter_mut().zip(&self.next_pos) {
            *position = clamp_pos(next);
        }

        if self.rng.next_f64() < 1.0 / self.iv as f64 {
            let i = self.rng.below(n);
            self.pick(i);
        }

        self.steps += 1;
    }

    pub fn capture_trail_frame(&mut self) {
        let frame_len = self.n * self.dim as usize;
        let base = self.head * frame_len;
        self.hist[base..base + frame_len].copy_from_slice(&self.pos);
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
        // Scratch buffer is reused across calls — analyse runs up to 10x/sec
        // while a graph-derived legend is visible.
        if self.indeg.len() != n {
            self.indeg = vec![0u32; n];
        } else {
            self.indeg.fill(0);
        }
        {
            let indeg = &mut self.indeg;
            let fr = &self.fr;
            for i in 0..n {
                let f = fr[i] as usize;
                if f < n {
                    indeg[f] = indeg[f].saturating_add(1);
                }
            }
        }
        let mut ind_max = 0u8;
        for (&degree, bucket) in self.indeg.iter().zip(self.ind.iter_mut()) {
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
        let mut cyc_max = 0u8;

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
                if cycle_len > cyc_max {
                    cyc_max = cycle_len;
                }
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
        self.cyc_max = cyc_max;
        // Component sizes indexed by comp id; rebuilt each analyse. clear()+
        // resize() keeps the allocation instead of dropping it every call.
        self.comp_size.clear();
        self.comp_size.resize(self.ncomp as usize, 0);
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

    /// Grand tour is active in any dimension above 3 when enabled.
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

        let mut sim = Sim::new(8, 99, 42);
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
        assert_eq!(sim.hist.len(), sim.n * MAX_TRAIL_FRAMES * sim.dim as usize);
    }

    #[test]
    fn trail_history_budget_limits_population() {
        assert_eq!(max_population_for_trail_length(2, 5), 1_000_000);
        assert_eq!(max_population_for_trail_length(30, 5), 447_392);
        assert_eq!(max_population_for_trail_length(120, 5), 111_848);
        assert_eq!(max_population_for_trail_length(30, 24), 93_206);
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

        assert_eq!(sim.pos.len(), sim.n * sim.dim as usize);
        assert!(sim.pos.iter().all(|value| value.is_finite()));
        assert_eq!(sim.hist.len(), sim.n * DEFAULT_TRAIL_FRAMES * sim.dim as usize);
        assert_eq!(sim.spd.len(), sim.n);
        assert!(sim.spd.iter().all(|value| value.is_finite()));
        assert!(sim.spread.is_finite());
        assert_graph_invariants(&sim);
    }

    #[test]
    fn stepping_keeps_24d_state_finite() {
        let mut sim = Sim::new(9, 24, 1);
        for _ in 0..200 {
            sim.step();
        }
        sim.capture_trail_frame();

        assert_eq!(sim.pos.len(), sim.n * 24);
        assert!(sim.pos.iter().all(|value| value.is_finite()));
        assert_eq!(sim.trail_len(), 1);
    }
}
