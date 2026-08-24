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
use sim::{Sim, STR, TRAIL};
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
        let packed = vec![0.0; sim.n * 5];
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
        self.packed = vec![0.0; self.sim.n * 5];
        self.colors_dirty = true;
    }

    pub fn set_n(&mut self, n: usize) {
        self.sim.resize(n);
        self.packed = vec![0.0; self.sim.n * 5];
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
        self.cam.zoom = v.clamp(0.15, 8.0);
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
        TRAIL
    }
    pub fn trail_stride(&self) -> usize {
        STR
    }

    /// Build trail line-segment geometry into the persistent scratch buffers.
    /// Faithful port of the JS buildTrails(): same depth schedule, dot stride,
    /// alpha ramp, and segment pairing. `palette` is the active mode's colours
    /// as packed [r,g,b] f32 triples (up to 8 entries). `pal_len` is the active
    /// swatch count, passed by the JS caller (which knows the active palette
    /// set's width). Returns the vertex count.
    pub fn build_trail_geometry(&mut self, palette: &[f32], pal_len: usize) -> u32 {
        let n = self.sim.n;
        let hlen = self.sim.trail_len();
        let head = self.sim.trail_head();
        let depth = if n > 20000 {
            8
        } else if n > 5000 {
            12
        } else if n > 1000 {
            22
        } else {
            TRAIL
        }
        .min(hlen);
        if depth <= 2 {
            return 0;
        }
        let stride = n.div_ceil(3000);
        let max_dots = n.div_ceil(stride);
        let max_verts = max_dots * depth * 2;
        if self.trail_verts.len() < max_verts * 5 {
            self.trail_verts = vec![0.0; max_verts * 5];
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
        let mut i = 0usize;
        while i < n {
            let ci = (colors[i] as usize) % pal_len;
            let (r, g, b) = (palette[ci * 3], palette[ci * 3 + 1], palette[ci * 3 + 2]);
            let mut prev = [0.0f32; 5];
            let mut have = false;
            for k in 0..depth {
                let slot = (head + TRAIL * 2 - depth + k) % TRAIL;
                let o = slot * n * STR + i * STR;
                let cur = [hist[o], hist[o + 1], hist[o + 2], hist[o + 3], hist[o + 4]];
                let t = (k + 1) as f32 / depth as f32;
                let alpha = 0.03 + 0.32 * t * t;
                if have {
                    self.trail_verts[vp..vp + 5].copy_from_slice(&prev);
                    self.trail_cols[cp] = r;
                    self.trail_cols[cp + 1] = g;
                    self.trail_cols[cp + 2] = b;
                    self.trail_cols[cp + 3] = alpha;
                    vp += 5;
                    cp += 4;
                    self.trail_verts[vp..vp + 5].copy_from_slice(&cur);
                    self.trail_cols[cp] = r;
                    self.trail_cols[cp + 1] = g;
                    self.trail_cols[cp + 2] = b;
                    self.trail_cols[cp + 3] = alpha;
                    vp += 5;
                    cp += 4;
                }
                prev = cur;
                have = true;
            }
            i += stride;
        }
        (vp / 5) as u32
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
    ///  tourF(15) tourN(10)]
    pub fn uniforms(&self, width: f32, height: f32) -> Vec<f32> {
        let u: Uniforms = self.cam.build_uniforms(&self.sim, &self.p, width, height);
        let mut v = Vec::with_capacity(30 + 15 + 10);
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
        for i in 0..3 {
            for k in 0..5 {
                v.push(u.tour_f[i][k]);
            }
        }
        for i in 0..2 {
            for k in 0..5 {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapper_uses_normalized_simulation_state() {
        let mut flock = Flock::new(0, 99, 42);
        assert_eq!(flock.n(), 3);
        assert_eq!(flock.dim(), 5);
        assert_eq!(flock.positions_len(), 15);

        flock.set_n(1);
        flock.set_dim(0);
        assert_eq!(flock.n(), 3);
        assert_eq!(flock.dim(), 2);
        assert_eq!(flock.positions_len(), 15);
    }

    #[test]
    fn exported_inputs_are_guarded() {
        let mut flock = Flock::new(3, 3, 42);
        flock.set_repick(0);
        assert_eq!(flock.sim.iv, 1);

        for _ in 0..4 {
            flock.step();
        }
        assert_eq!(flock.build_trail_geometry(&[], 1), 0);
        assert_eq!(flock.build_trail_geometry(&[1.0, 0.0], 1), 0);
    }

    #[test]
    fn orbit_rotates_every_spatial_view_except_2d() {
        for dim in 2..=5 {
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
}

/// The wasm linear memory, so JS can build zero-copy typed-array views over
/// the pointer/length accessors above. (Trunk's generated init script does
/// not publish the memory object; named `wasm_memory` because `memory`
/// collides with the module's built-in memory export.)
#[wasm_bindgen(js_name = wasm_memory)]
pub fn wasm_memory() -> js_sys::WebAssembly::Memory {
    wasm_bindgen::memory().into()
}
