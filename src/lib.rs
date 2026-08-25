//! wasm-bindgen entry point. Exposes the Sim + Camera to JS.
//!
//! JS owns the rAF loop and DOM. Each frame:
//!   if running { for _ in 0..speed { sim.step() } }
//!   flock.update_camera(w, h)          // centroid fit + rock/spin + tour
//!   read packed positions + uniforms   // zero-copy views into wasm memory
//!   -> JS uploads to GL and draws.

mod camera;
pub mod parity;
mod rng;
pub mod sim;
mod tour;

use camera::{CamParams, Camera, Uniforms};
use sim::Sim;
use tour::{MAX_DIM, MAX_SLICE_DIMS, PDIM};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Flock {
    sim: Sim,
    cam: Camera,
    p: CamParams,
    col_mode: u32,
    shadow: bool,
    /// Persistent per-dot colour indices for the active mode. Recomputed only
    /// when `colors_dirty` is set (re-pick / mode change / rebuild / resize).
    colors: Vec<u8>,
    colors_dirty: bool,
    /// Bumped every step so JS can skip no-op GL re-uploads while paused.
    frame: u32,
    /// Persistent scratch for trail line vertices [x,y,z,w,v] and per-vertex
    /// RGBA colours. Filled by `build_trail_geometry` each frame when trails
    /// are on; JS uploads straight from wasm memory.
    trail_verts: Vec<f32>,
    trail_cols: Vec<f32>,
    /// Line-segment indices into the vertex scratch. Depends only on the trail
    /// count and depth, so it survives across frames.
    trail_indices: Vec<u32>,
    trail_index_shape: (usize, usize),
    trail_index_version: u32,
}

#[wasm_bindgen]
impl Flock {
    #[wasm_bindgen(constructor)]
    pub fn new(n: usize, dim: u32, seed: u64) -> Flock {
        let sim = Sim::new(n, dim, seed);
        Flock {
            sim,
            cam: Camera::new(),
            p: CamParams {
                rock: true,
                spin: false,
                fit: true,
                lens: 0.45,
                fog: true,
                round: true,
                running: true,
                wa: 0.0,
                wb: 0.0,
                wc: 0.0,
                iso_on: false,
                wp: true,
            },
            col_mode: 0,
            shadow: true,
            colors: Vec::new(),
            colors_dirty: true,
            frame: 0,
            trail_verts: Vec::new(),
            trail_cols: Vec::new(),
            trail_indices: Vec::new(),
            trail_index_shape: (0, 0),
            trail_index_version: 0,
        }
    }

    pub fn step(&mut self) {
        self.sim.step();
        self.frame = self.frame.wrapping_add(1);
        if self.sim.dirty {
            self.colors_dirty = true;
        }
        // Mode 4 (speed) recomputes per-frame, so the cached colour buffer
        // must be re-uploaded every step. sync_colors() already drives the
        // JS re-upload when this returns true.
        if self.col_mode == 4 {
            self.colors_dirty = true;
        }
        // Modes that read graph-derived signals (cycle-length, comp-size,
        // in-degree) would otherwise render stale cyc_len/comp_size/ind for
        // a few frames between a graph-changing repick and the next analyse
        // call (legend only triggers analyse every 6 frames). Re-analyse
        // eagerly when the graph is dirty in those modes so the bucket
        // matches the user's currently-visible graph.
        if self.sim.dirty && matches!(self.col_mode, 5..=7) {
            self.sim.analyse();
            self.colors_dirty = true;
        }
    }

    pub fn capture_trail_frame(&mut self) {
        self.sim.capture_trail_frame();
    }

    /// Clear recorded trail history. JS calls this when trail capture resumes
    /// after being switched off, so the ring cannot join positions across the
    /// gap into one long streak.
    pub fn reset_trails(&mut self) {
        self.sim.reset_trails();
    }

    pub fn set_running(&mut self, v: bool) {
        self.p.running = v;
    }

    /// Advance camera (fit, rock/spin, tour). Call once per frame.
    pub fn update_camera(&mut self, width: f32, height: f32) {
        let p = clone_params(&self.p);
        self.cam.update(&mut self.sim, &p, width, height);
    }

    /// Recompute the spread readout on demand. It's a full n*dim pass, so JS
    /// calls it on the readout cadence (every 6 frames) instead of per frame.
    pub fn measure_spread(&mut self) {
        self.sim.meas_public();
    }

    /// sim.pos is already row-major interleaved — exactly the GL vertex
    /// layout — so JS reads it in place (zero-copy, no repack pass). The
    /// pointer is re-read every frame, so realloc on set_n/set_dim is safe.
    pub fn positions_ptr(&self) -> *const f32 {
        self.sim.pos.as_ptr()
    }

    pub fn positions_len(&self) -> usize {
        self.sim.pos.len()
    }

    pub fn n(&self) -> usize {
        self.sim.n
    }
    pub fn steps(&self) -> u64 {
        self.sim.steps
    }
    pub fn spread(&self) -> f32 {
        self.sim.spread
    }
    pub fn dim(&self) -> u32 {
        self.sim.dim
    }

    // --- view angles for readout ---
    pub fn yaw_deg(&self) -> i32 {
        ((self.cam.yaw.to_degrees().round() as i32) % 360 + 360) % 360
    }
    pub fn pitch_deg(&self) -> i32 {
        self.cam.pitch.to_degrees().round() as i32
    }

    // --- setters from the UI ---
    pub fn set_friend(&mut self, v: f32) {
        self.sim.f = v;
    }
    pub fn set_enemy(&mut self, v: f32) {
        self.sim.e = v;
    }
    pub fn set_centre(&mut self, v: f32) {
        self.sim.c = v;
    }
    pub fn set_repick(&mut self, v: u32) {
        self.sim.iv = v.max(1);
    }
    pub fn set_speed(&mut self, v: u32) {
        self.sim.speed = v.max(1);
    }
    pub fn set_law_prop(&mut self, v: bool) {
        self.sim.law_prop = v;
    }
    pub fn set_leg(&mut self, v: f32) {
        self.sim.leg = v;
    }
    pub fn set_slab_h(&mut self, v: f32) {
        self.sim.slab_h = v;
    }
    pub fn set_slab_c(&mut self, v: f32) {
        self.sim.slab_c = v;
    }
    pub fn set_lens(&mut self, v: f32) {
        self.p.lens = v;
    }
    pub fn set_fog(&mut self, v: bool) {
        self.p.fog = v;
    }
    pub fn set_rock(&mut self, v: bool) {
        self.p.rock = v;
    }
    pub fn set_spin(&mut self, v: bool) {
        self.p.spin = v;
    }
    pub fn set_fit(&mut self, v: bool) {
        self.p.fit = v;
    }
    pub fn set_round(&mut self, v: bool) {
        self.p.round = v;
    }
    pub fn set_shadow(&mut self, v: bool) {
        self.shadow = v;
    }
    pub fn set_wa(&mut self, v: f32) {
        self.p.wa = v;
    }
    pub fn set_wb(&mut self, v: f32) {
        self.p.wb = v;
    }
    pub fn set_wc(&mut self, v: f32) {
        self.p.wc = v;
    }
    pub fn set_iso(&mut self, v: bool) {
        self.p.iso_on = v;
    }
    pub fn set_wp(&mut self, v: bool) {
        self.p.wp = v;
    }
    pub fn set_tour(&mut self, v: bool) {
        self.sim.tour_on = v;
        if v {
            self.sim.tour.reset(self.sim.dim);
        }
    }
    pub fn set_col_mode(&mut self, v: u32) {
        if v != self.col_mode {
            self.col_mode = v;
            self.colors_dirty = true;
        }
    }

    pub fn set_dim(&mut self, dim: u32) {
        self.sim.set_dim(dim);
        self.cam.reset_view(self.sim.dim);
        if self.sim.dim == 2 {
            self.p.spin = false;
        }
        self.colors_dirty = true;
    }

    pub fn set_n(&mut self, n: usize) {
        self.sim.resize(n);
        self.colors_dirty = true;
    }

    pub fn set_trail_length(&mut self, frames: usize) {
        self.sim.set_trail_capacity(frames);
        self.colors_dirty = true;
    }

    pub fn repick_all(&mut self) {
        self.sim.repick_all();
        self.colors_dirty = true;
    }
    pub fn reset(&mut self) {
        self.sim.build();
        self.colors_dirty = true;
    }
    pub fn reset_view(&mut self) {
        self.cam.zoom = 1.0;
        if self.sim.dim == 3 {
            self.cam.yaw = 0.6;
            self.cam.pitch = 0.32;
        } else {
            self.cam.yaw = 0.0;
            self.cam.pitch = 0.0;
        }
    }
    pub fn set_zoom(&mut self, v: f32) {
        self.cam.zoom = v.clamp(0.15, 64.0);
    }
    pub fn zoom(&self) -> f32 {
        self.cam.zoom
    }

    pub fn orbit(&mut self, dx: f32, dy: f32) {
        if self.sim.dim == 2 {
            return;
        }
        self.cam.yaw += dx * 0.006;
        self.cam.pitch = (self.cam.pitch + dy * 0.006).clamp(-1.5, 1.5);
    }

    // --- analyse / colouring ---
    pub fn analyse(&mut self) {
        self.sim.analyse();
    }
    pub fn ncomp(&self) -> u32 {
        self.sim.ncomp
    }
    pub fn dmax(&self) -> u32 {
        self.sim.dmax
    }
    /// Per-frame maximum squared speed (colour mode 4 legend).
    pub fn spd_max(&self) -> f32 {
        self.sim.spd_max
    }
    /// Max cycle length currently observed (colour mode 6 legend). Tracked
    /// during analyse() — no per-call scan.
    pub fn cyc_len_max(&self) -> u8 {
        self.sim.cyc_max
    }
    /// Largest component size currently observed (colour mode 5 legend).
    pub fn comp_size_max(&self) -> u32 {
        self.sim.comp_size.iter().copied().max().unwrap_or(0)
    }
    /// Largest in-degree bucket currently observed (colour mode 7 legend).
    pub fn ind_max(&self) -> u8 {
        self.sim.ind_max
    }
    /// Active colour mode (lets JS read back the current id).
    pub fn col_mode(&self) -> u32 {
        self.col_mode
    }
    /// Recompute the cached per-dot colour indices if stale, and report
    /// whether the buffer changed since the last call. JS re-uploads the
    /// colour GL buffer only when this returns true.
    pub fn sync_colors(&mut self) -> bool {
        if !self.colors_dirty {
            return false;
        }
        let mode = self.col_mode;
        let n = self.sim.n;
        if self.colors.len() != n {
            self.colors = vec![0u8; n];
        }
        let sim = &self.sim;
        let colors = &mut self.colors;
        // The mode is fixed for the whole buffer, so the match belongs outside
        // the loop. Modes 0 and 4 are the ones that get re-run per frame.
        match mode {
            0 => colors.copy_from_slice(&sim.hu),
            4 => {
                let max = sim.spd_max;
                let live = max > 1e-12;
                for (colour, &speed) in colors.iter_mut().zip(&sim.spd) {
                    let t = if live { (speed / max).sqrt() } else { 0.0 };
                    let idx = (t * (7.0 - 0.001)).floor() as i32;
                    *colour = idx.clamp(0, 6) as u8;
                }
            }
            _ => {
                for (i, colour) in colors.iter_mut().enumerate() {
                    *colour = sim.color_of(i, mode);
                }
            }
        }
        self.colors_dirty = false;
        true
    }
    pub fn colors_ptr(&self) -> *const u8 {
        self.colors.as_ptr()
    }
    pub fn colors_len(&self) -> usize {
        self.colors.len()
    }
    pub fn frame(&self) -> u32 {
        self.frame
    }
    /// Counted during analyse rather than rescanned; the mode-3 legend asks for
    /// this every few frames.
    pub fn on_cycle_count(&self) -> u32 {
        self.sim.on_cycle
    }
    pub fn touring_active(&self) -> bool {
        self.sim.touring_active()
    }
    pub fn tour_leg(&self) -> u32 {
        self.sim.tour.leg
    }
    pub fn tour_t(&self) -> f32 {
        self.sim.tour.t
    }

    // --- links + flash ---
    /// Bumped only on re-pick / rebuild, so JS refreshes the friend/enemy
    /// views exactly when the graph changed — not every step.
    pub fn graph_version(&self) -> u32 {
        self.sim.graph_version
    }
    pub fn friends_ptr(&self) -> *const i32 {
        self.sim.fr.as_ptr()
    }
    pub fn enemies_ptr(&self) -> *const i32 {
        self.sim.en.as_ptr()
    }
    pub fn graph_len(&self) -> usize {
        self.sim.fr.len()
    }

    // --- trail ring access (pointer-based; JS views wasm memory) ---
    pub fn trail_ptr(&self) -> *const f32 {
        self.sim.hist.as_ptr()
    }
    pub fn trail_buf_len(&self) -> usize {
        self.sim.hist.len()
    }
    pub fn trail_len(&self) -> usize {
        self.sim.trail_len()
    }
    pub fn trail_head(&self) -> usize {
        self.sim.trail_head()
    }
    pub fn trail_slots(&self) -> usize {
        self.sim.trail_capacity()
    }
    pub fn trail_stride(&self) -> usize {
        self.sim.dim as usize
    }

    /// Build complete trails for at most `max_trails` dots into persistent
    /// scratch buffers. Sample prefixes are stable as the budget changes.
    /// Returns the vertex count; the segments themselves are described by
    /// `trail_indices_ptr` / `trail_index_count`.
    pub fn build_trail_geometry(
        &mut self,
        palette: &[f32],
        pal_len: usize,
        max_trails: usize,
    ) -> u32 {
        let n = self.sim.n;
        let capacity = self.sim.trail_capacity();
        let depth = self.sim.trail_len().min(capacity);
        let selected = max_trails.min(n);
        if depth < 2 || selected == 0 {
            return 0;
        }
        let head = self.sim.trail_head();
        let dim = self.sim.dim as usize;
        // One vertex per sample: the segment list shares the interior points
        // rather than storing each of them twice.
        let vert_count = selected * depth;
        // Each buffer is checked against its own requirement: the strides
        // differ, so one shared check leaves the other short when `dim` falls
        // and the trail budget rises at the same time.
        if self.trail_verts.len() < vert_count * dim {
            self.trail_verts = vec![0.0; vert_count * dim];
        }
        if self.trail_cols.len() < vert_count * 4 {
            self.trail_cols = vec![0.0; vert_count * 4];
        }
        if self.trail_index_shape != (selected, depth) {
            self.trail_indices.clear();
            self.trail_indices.reserve(selected * (depth - 1) * 2);
            for trail in 0..selected {
                let base = (trail * depth) as u32;
                for k in 0..depth as u32 - 1 {
                    self.trail_indices.push(base + k);
                    self.trail_indices.push(base + k + 1);
                }
            }
            self.trail_index_shape = (selected, depth);
            self.trail_index_version = self.trail_index_version.wrapping_add(1);
        }
        // colours must be fresh for this frame
        self.sync_colors();
        let hist = &self.sim.hist;
        let colors = &self.colors;
        let pal_len = pal_len.min(palette.len() / 3);
        if pal_len == 0 {
            return 0;
        }
        let mut vp = 0usize;
        let mut cp = 0usize;
        let stride = trail_sample_stride(n);
        let mut i = 0usize;
        let verts = &mut self.trail_verts;
        let cols = &mut self.trail_cols;
        for _ in 0..selected {
            let ci = (colors[i] as usize) % pal_len;
            let (r, g, b) = (palette[ci * 3], palette[ci * 3 + 1], palette[ci * 3 + 2]);
            // Walk the ring by wrapping increment instead of a modulo per slot.
            let mut slot = (head + capacity * 2 - depth) % capacity;
            for k in 0..depth {
                let o = slot * n * dim + i * dim;
                verts[vp..vp + dim].copy_from_slice(&hist[o..o + dim]);
                let t = (k + 1) as f32 / depth as f32;
                cols[cp] = r;
                cols[cp + 1] = g;
                cols[cp + 2] = b;
                cols[cp + 3] = 0.03 + 0.32 * t * t;
                vp += dim;
                cp += 4;
                slot += 1;
                if slot == capacity {
                    slot = 0;
                }
            }
            i = (i + stride) % n;
        }
        (vp / dim) as u32
    }
    pub fn trail_verts_ptr(&self) -> *const f32 {
        self.trail_verts.as_ptr()
    }
    pub fn trail_cols_ptr(&self) -> *const f32 {
        self.trail_cols.as_ptr()
    }
    pub fn trail_indices_ptr(&self) -> *const u32 {
        self.trail_indices.as_ptr()
    }
    pub fn trail_index_count(&self) -> usize {
        self.trail_indices.len()
    }
    /// Bumped when the index buffer is rebuilt, so JS re-uploads it only when
    /// the trail count or depth actually changed.
    pub fn trail_index_version(&self) -> u32 {
        self.trail_index_version
    }

    /// Flat f32 uniform block for the vertex shader. Order:
    /// [sy cy sp cp dist fov half cx cy cz |
    ///  sinA cosA sinB cosB sinC cosC isoC isoS slabH slabC |
    ///  touring dim W H nslice fog wp iso base round |
    ///  tourF(72) tourN(504)]
    pub fn uniforms(&self, width: f32, height: f32) -> Vec<f32> {
        let u: Uniforms = self.cam.build_uniforms(&self.sim, &self.p, width, height);
        let mut v = Vec::with_capacity(30 + PDIM * MAX_DIM + MAX_SLICE_DIMS * MAX_DIM);
        v.push(u.sy);
        v.push(u.cy);
        v.push(u.sp);
        v.push(u.cp);
        v.push(u.dist);
        v.push(u.fov);
        v.push(u.half);
        v.push(u.ccx);
        v.push(u.ccy);
        v.push(u.ccz);
        v.push(u.sin_a);
        v.push(u.cos_a);
        v.push(u.sin_b);
        v.push(u.cos_b);
        v.push(u.sin_c);
        v.push(u.cos_c);
        v.push(u.iso_c);
        v.push(u.iso_s);
        v.push(u.slab_h);
        v.push(u.slab_c);
        v.push(u.touring);
        v.push(u.dim);
        v.push(u.width);
        v.push(u.height);
        v.push(u.nslice);
        v.push(u.fog);
        v.push(u.wp);
        v.push(u.iso);
        v.push(u.base);
        v.push(u.round);
        for i in 0..PDIM {
            for k in 0..MAX_DIM {
                v.push(u.tour_f[i][k]);
            }
        }
        for i in 0..MAX_SLICE_DIMS {
            for k in 0..MAX_DIM {
                v.push(u.tour_n[i][k]);
            }
        }
        v
    }
}

fn clone_params(p: &CamParams) -> CamParams {
    CamParams {
        rock: p.rock,
        spin: p.spin,
        fit: p.fit,
        lens: p.lens,
        fog: p.fog,
        round: p.round,
        running: p.running,
        wa: p.wa,
        wb: p.wb,
        wc: p.wc,
        iso_on: p.iso_on,
        wp: p.wp,
    }
}

fn trail_sample_stride(n: usize) -> usize {
    let mut stride = ((n as f64 * 0.618_033_988_75).round() as usize).max(1);
    while gcd(stride, n) != 1 {
        stride += 1;
    }
    stride
}

fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapper_uses_normalized_simulation_state() {
        let mut flock = Flock::new(0, 99, 42);
        assert_eq!(flock.n(), 3);
        assert_eq!(flock.dim(), 24);
        assert_eq!(flock.positions_len(), 72);

        flock.set_n(1);
        flock.set_dim(0);
        assert_eq!(flock.n(), 3);
        assert_eq!(flock.dim(), 2);
        assert_eq!(flock.positions_len(), 6);
    }

    #[test]
    fn positions_view_aliases_sim_buffer() {
        // Zero-copy contract: JS reads sim.pos in place. A regression to a
        // packed copy would silently double per-frame memory traffic.
        let flock = Flock::new(8, 3, 42);
        assert_eq!(flock.positions_ptr(), flock.sim.pos.as_ptr());
        assert_eq!(flock.positions_len(), flock.sim.pos.len());
    }

    #[test]
    fn exported_inputs_are_guarded() {
        let mut flock = Flock::new(3, 3, 42);
        flock.set_repick(0);
        assert_eq!(flock.sim.iv, 1);

        for _ in 0..4 {
            flock.step();
            flock.capture_trail_frame();
        }
        assert_eq!(flock.build_trail_geometry(&[], 1, 3), 0);
        assert_eq!(flock.build_trail_geometry(&[1.0, 0.0], 1, 3), 0);
    }

    #[test]
    fn orbit_rotates_every_spatial_view_except_2d() {
        for dim in [2, 3, 4, 5, 8, 24] {
            let mut flock = Flock::new(3, dim, 42);
            let initial_yaw = flock.cam.yaw;
            let initial_pitch = flock.cam.pitch;

            flock.orbit(10.0, -5.0);

            if dim == 2 {
                assert_eq!(flock.cam.yaw, initial_yaw);
                assert_eq!(flock.cam.pitch, initial_pitch);
            } else {
                assert_ne!(flock.cam.yaw, initial_yaw);
                assert_ne!(flock.cam.pitch, initial_pitch);
            }
        }
    }

    #[test]
    fn zoom_supports_close_views_and_clamps_invalid_extremes() {
        let mut flock = Flock::new(3, 3, 42);

        flock.set_zoom(48.0);
        assert_eq!(flock.zoom(), 48.0);
        flock.set_zoom(100.0);
        assert_eq!(flock.zoom(), 64.0);
        flock.set_zoom(0.0);
        assert_eq!(flock.zoom(), 0.15);
    }

    #[test]
    fn zoom_scales_projection_equally_in_every_dimension() {
        for dim in [2, 3, 5, 8, 24] {
            let mut flock = Flock::new(3, dim, 42);
            flock.set_rock(false);
            flock.update_camera(1000.0, 800.0);
            let initial = flock.uniforms(1000.0, 800.0);
            let initial_scale = initial[5] / initial[4] * initial[6] / (800.0 * 0.36);

            flock.set_zoom(4.0);
            flock.update_camera(1000.0, 800.0);
            let zoomed = flock.uniforms(1000.0, 800.0);
            let zoomed_scale = zoomed[5] / zoomed[4] * zoomed[6] / (800.0 * 0.36);

            assert!((zoomed_scale / initial_scale - 4.0).abs() < 1e-5);
            assert!((zoomed[4] - initial[4]).abs() < 1e-5);
        }
    }

    #[test]
    fn trail_geometry_survives_a_dimension_change() {
        let palette = vec![0.5f32; 24];
        let mut flock = Flock::new(2178, 24, 42);
        flock.set_trail_length(30);
        for _ in 0..30 {
            flock.step();
            flock.capture_trail_frame();
        }
        // A smaller budget here, a larger one after the switch: the adaptive
        // trail quality does exactly this in the page.
        flock.build_trail_geometry(&palette, 8, flock.n() / 2);

        // Dropping to 3d shrinks the per-vertex stride 8x while the budget
        // grows, so a shared size check on the vertex buffer alone leaves the
        // colour buffer short.
        flock.set_dim(3);
        for _ in 0..30 {
            flock.step();
            flock.capture_trail_frame();
        }
        let vertices = flock.build_trail_geometry(&palette, 8, flock.n());
        assert_eq!(vertices, (flock.n() * 30) as u32);
    }

    #[test]
    fn trail_budget_builds_complete_stable_samples() {
        let mut flock = Flock::new(11, 3, 42);
        let capacity = flock.trail_slots();
        for _ in 0..capacity {
            flock.step();
            flock.capture_trail_frame();
        }
        let palette = [1.0, 0.5, 0.25];
        // One vertex per sample; the segments come from the index buffer.
        let segments_per_trail = capacity - 1;

        assert_eq!(flock.build_trail_geometry(&palette, 1, 0), 0);
        assert_eq!(
            flock.build_trail_geometry(&palette, 1, 4),
            (4 * capacity) as u32
        );
        assert_eq!(flock.trail_index_count(), 4 * segments_per_trail * 2);
        assert_eq!(
            flock.build_trail_geometry(&palette, 1, flock.n()),
            (flock.n() * capacity) as u32
        );
        assert_eq!(
            flock.trail_index_count(),
            flock.n() * segments_per_trail * 2
        );

        let stride = trail_sample_stride(flock.n());
        let order: Vec<_> = (0..flock.n())
            .scan(0, |index, _| {
                let current = *index;
                *index = (*index + stride) % flock.n();
                Some(current)
            })
            .collect();
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..flock.n()).collect::<Vec<_>>());
        assert_eq!(
            &order[..4],
            &[0, stride, stride * 2 % flock.n(), stride * 3 % flock.n()]
        );

        flock.set_trail_length(4);
        for _ in 0..4 {
            flock.capture_trail_frame();
        }
        assert_eq!(flock.trail_slots(), 4);
        // 2 trails x 4 samples, joined by 2 x 3 segments.
        assert_eq!(flock.build_trail_geometry(&palette, 1, 2), 8);
        assert_eq!(flock.trail_index_count(), 12);
    }
}

/// The wasm linear memory, so JS can build zero-copy typed-array views over
/// the pointer/length accessors above. (Trunk's generated init script does
/// not publish the memory object; named `wasm_memory` because `memory`
/// collides with the module's built-in memory export.)
#[wasm_bindgen(js_name = wasm_memory)]
pub fn wasm_memory() -> js_sys::WebAssembly::Memory {
    wasm_bindgen::memory().into()
}
