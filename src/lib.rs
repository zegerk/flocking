//! wasm-bindgen entry point. Exposes the Sim + Camera to JS.
//!
//! JS owns the rAF loop and DOM. Each frame:
//!   if running { for _ in 0..speed { sim.step() } }
//!   flock.update_camera(w, h)          // centroid fit + rock/spin + tour
//!   read packed positions + uniforms   // zero-copy views into wasm memory
//!   -> JS uploads to GL and draws.

mod camera;
mod rng;
mod sim;
mod tour;

use camera::{CamParams, Camera, Uniforms};
use sim::Sim;
use tour::{MAX_DIM, MAX_SLICE_DIMS, PDIM};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Flock {
    sim: Sim,
    cam: Camera,
    packed: Vec<f32>,
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
}

#[wasm_bindgen]
impl Flock {
    #[wasm_bindgen(constructor)]
    pub fn new(n: usize, dim: u32, seed: u64) -> Flock {
        let sim = Sim::new(n, dim, seed);
        let packed = vec![0.0; sim.n * sim.dim as usize];
        Flock {
            sim,
            cam: Camera::new(),
            packed,
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

    pub fn set_running(&mut self, v: bool) {
        self.p.running = v;
    }

    /// Advance camera (fit, rock/spin, tour). Call once per frame.
    pub fn update_camera(&mut self, width: f32, height: f32) {
        let p = clone_params(&self.p);
        self.cam.update(&mut self.sim, &p, width, height);
        self.sim.meas_public();
    }

    /// Repack positions interleaved into the persistent scratch buffer.
    /// JS reads them through the pointer/length accessors below (zero-copy).
    pub fn repack_positions(&mut self) {
        sim::pack_positions(&self.sim, &mut self.packed);
    }
    pub fn positions_ptr(&self) -> *const f32 {
        self.packed.as_ptr()
    }

    pub fn positions_len(&self) -> usize {
        self.packed.len()
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
        self.packed = vec![0.0; self.sim.n * self.sim.dim as usize];
        self.colors_dirty = true;
    }

    pub fn set_n(&mut self, n: usize) {
        self.sim.resize(n);
        self.packed = vec![0.0; self.sim.n * self.sim.dim as usize];
        self.colors_dirty = true;
    }

    pub fn set_trail_length(&mut self, frames: usize) {
        self.sim.set_trail_capacity(frames);
        self.packed = vec![0.0; self.sim.n * self.sim.dim as usize];
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
    /// Max cycle length currently observed (colour mode 6 legend).
    pub fn cyc_len_max(&self) -> u8 {
        (0..self.sim.n)
            .map(|i| self.sim.cyc_len[i])
            .max()
            .unwrap_or(0)
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
        for i in 0..n {
            self.colors[i] = self.sim.color_of(i, mode);
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
    pub fn on_cycle_count(&self) -> u32 {
        let mut k = 0;
        for i in 0..self.sim.n {
            if self.sim.color_of(i, 3) == 1 {
                k += 1;
            }
        }
        k
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
    /// Bumped on every re-pick so JS can refresh the friend/enemy views
    /// (their pointers may move when the graph arrays are rebuilt).
    pub fn graph_version(&self) -> u32 {
        self.frame
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
        let max_verts = selected * (depth - 1) * 2;
        if self.trail_verts.len() < max_verts * dim {
            self.trail_verts = vec![0.0; max_verts * dim];
            self.trail_cols = vec![0.0; max_verts * 4];
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
        for _ in 0..selected {
            let ci = (colors[i] as usize) % pal_len;
            let (r, g, b) = (palette[ci * 3], palette[ci * 3 + 1], palette[ci * 3 + 2]);
            let mut prev = [0.0f32; MAX_DIM];
            let mut have = false;
            for k in 0..depth {
                let slot = (head + capacity * 2 - depth + k) % capacity;
                let o = slot * n * dim + i * dim;
                let mut cur = [0.0f32; MAX_DIM];
                cur[..dim].copy_from_slice(&hist[o..o + dim]);
                let t = (k + 1) as f32 / depth as f32;
                let alpha = 0.03 + 0.32 * t * t;
                if have {
                    self.trail_verts[vp..vp + dim].copy_from_slice(&prev[..dim]);
                    self.trail_cols[cp] = r;
                    self.trail_cols[cp + 1] = g;
                    self.trail_cols[cp + 2] = b;
                    self.trail_cols[cp + 3] = alpha;
                    vp += dim;
                    cp += 4;
                    self.trail_verts[vp..vp + dim].copy_from_slice(&cur[..dim]);
                    self.trail_cols[cp] = r;
                    self.trail_cols[cp + 1] = g;
                    self.trail_cols[cp + 2] = b;
                    self.trail_cols[cp + 3] = alpha;
                    vp += dim;
                    cp += 4;
                }
                prev = cur;
                have = true;
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
    fn trail_budget_builds_complete_stable_samples() {
        let mut flock = Flock::new(11, 3, 42);
        let capacity = flock.trail_slots();
        for _ in 0..capacity {
            flock.step();
            flock.capture_trail_frame();
        }
        let palette = [1.0, 0.5, 0.25];
        let vertices_per_trail = 2 * (capacity - 1);

        assert_eq!(flock.build_trail_geometry(&palette, 1, 0), 0);
        assert_eq!(
            flock.build_trail_geometry(&palette, 1, 4),
            (4 * vertices_per_trail) as u32
        );
        assert_eq!(
            flock.build_trail_geometry(&palette, 1, flock.n()),
            (flock.n() * vertices_per_trail) as u32
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
        assert_eq!(flock.build_trail_geometry(&palette, 1, 2), 12);
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
