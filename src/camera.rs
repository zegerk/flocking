//! Camera: centroid-fit framing, perspective params, rock/spin, yaw/pit, and
//! the manual 4D/5D rotation chain (isoclinic double turn, wa/wb/wc plane
//! rotations, w-perspective). Produces the uniform struct consumed by the
//! vertex shader each frame.

use crate::sim::Sim;
use crate::tour::{MAX_DIM, MAX_SLICE_DIMS, PDIM};

pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    phase: f32,
    iso: f32,
    init: bool,

    // Centroid-fit state.
    ccx: f32,
    ccy: f32,
    ccz: f32,
    radius: f32,

    // Outputs (updated by update()).
    pub sy: f32,
    pub cyw: f32,
    pub sp: f32,
    pub cp: f32,
    pub dist: f32,
    pub fov: f32,
    pub half: f32,
}

/// Uniform block handed to the vertex shader each frame. Laid out as a flat
/// f32 vector; see lib.rs `uniforms()` for the exact order.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Uniforms {
    pub sy: f32,
    pub cy: f32,
    pub sp: f32,
    pub cp: f32,
    pub dist: f32,
    pub fov: f32,
    pub half: f32,
    pub ccx: f32,
    pub ccy: f32,
    pub ccz: f32,
    // manual 4D/5D chain
    pub sin_a: f32,
    pub cos_a: f32,
    pub sin_b: f32,
    pub cos_b: f32,
    pub sin_c: f32,
    pub cos_c: f32,
    pub iso_c: f32,
    pub iso_s: f32,
    pub slab_h: f32,
    pub slab_c: f32,
    pub touring: f32,
    pub dim: f32,
    pub width: f32,
    pub height: f32,
    pub nslice: f32,
    pub fog: f32,
    pub wp: f32,   // w-perspective on
    pub iso: f32,  // isoclinic on
    pub base: f32, // dot base size
    pub round: f32,
    pub tour_f: [[f32; MAX_DIM]; PDIM],
    pub tour_n: [[f32; MAX_DIM]; MAX_SLICE_DIMS],
}

pub struct CamParams {
    pub rock: bool,
    pub spin: bool,
    pub fit: bool,
    pub lens: f32,
    pub fog: bool,
    pub round: bool,
    pub running: bool,
    // 4D/5D manual chain
    pub wa: f32,
    pub wb: f32,
    pub wc: f32,
    pub iso_on: bool,
    pub wp: bool,
}

impl Camera {
    pub fn new() -> Self {
        Camera {
            yaw: 0.6,
            pitch: 0.32,
            zoom: 1.0,
            phase: 0.0,
            iso: 0.0,
            init: false,
            ccx: 0.0,
            ccy: 0.0,
            ccz: 0.0,
            radius: 1.0,
            sy: 0.0,
            cyw: 1.0,
            sp: 0.0,
            cp: 1.0,
            dist: 4.0,
            fov: 900.0,
            half: 1.0,
        }
    }

    pub fn reset_view(&mut self, dim: u32) {
        if dim == 2 {
            self.yaw = 0.0;
            self.pitch = 0.0;
        } else if self.yaw == 0.0 && self.pitch == 0.0 {
            self.yaw = 0.6;
            self.pitch = 0.32;
        }
    }

    /// Advance camera state for this frame.
    pub fn update(&mut self, sim: &mut Sim, p: &CamParams, width: f32, height: f32) {
        let dim = sim.dim;
        if p.spin && p.running {
            self.yaw += 0.0022;
        }
        if p.rock {
            self.phase += 0.011;
        }
        let yaw = self.yaw
            + if p.rock && dim == 3 {
                0.20 * self.phase.sin()
            } else {
                0.0
            };
        self.sy = yaw.sin();
        self.cyw = yaw.cos();
        self.sp = self.pitch.sin();
        self.cp = self.pitch.cos();
        if p.iso_on && dim >= 4 && p.running {
            self.iso += 0.006;
        }

        // Centroid + radius fit over the live cloud.
        let (mut tx, mut ty, mut tz) = (0.0f32, 0.0f32, 0.0f32);
        let mut r = 0.87f32;
        if p.fit {
            let dim = dim as usize;
            let mut mins = [0.0f32; MAX_DIM];
            let mut maxs = [0.0f32; MAX_DIM];
            for k in 0..dim.min(3) {
                mins[k] = -0.5;
                maxs[k] = 0.5;
            }
            for i in 0..sim.n {
                let offset = i * dim;
                for k in 0..dim {
                    mins[k] = mins[k].min(sim.pos[offset + k]);
                    maxs[k] = maxs[k].max(sim.pos[offset + k]);
                }
            }
            tx = (mins[0] + maxs[0]) / 2.0;
            ty = (mins[1] + maxs[1]) / 2.0;
            tz = if dim >= 3 { (mins[2] + maxs[2]) / 2.0 } else { 0.0 };
            r = mins[..dim]
                .iter()
                .zip(&maxs[..dim])
                .map(|(&min, &max)| max - min)
                .fold(0.2f32, f32::max)
                * 0.62;
            if sim.touring_active() {
                tx = 0.0;
                ty = 0.0;
                tz = 0.0;
            }
        }
        let k = if self.init { 0.08 } else { 1.0 };
        self.ccx += (tx - self.ccx) * k;
        self.ccy += (ty - self.ccy) * k;
        self.ccz += (tz - self.ccz) * k;
        self.radius += (r - self.radius) * k;
        self.init = true;

        let base_half = width.min(height) * 0.36;
        self.half = base_half * self.zoom;
        self.fov = width.min(height) * 0.9 * p.lens;
        self.dist = self.radius * (self.fov / base_half.max(1.0) + 1.0);

        // Advance grand tour if active.
        if sim.touring_active() && p.running {
            let leg = sim.leg;
            sim.tour.advance(&mut sim.rng, leg);
        }
    }

    pub fn base_size(&self, n: usize, round: bool) -> f32 {
        let mut b: f32 = if n > 4000 {
            1.0
        } else if n > 1500 {
            1.3
        } else if n > 600 {
            1.6
        } else if n > 300 {
            2.0
        } else {
            2.4
        };
        if round {
            b = b.max(1.25);
        }
        b
    }

    pub fn build_uniforms(&self, sim: &Sim, p: &CamParams, width: f32, height: f32) -> Uniforms {
        Uniforms {
            sy: self.sy,
            cy: self.cyw,
            sp: self.sp,
            cp: self.cp,
            dist: self.dist,
            fov: self.fov,
            half: self.half,
            ccx: self.ccx,
            ccy: self.ccy,
            ccz: self.ccz,
            sin_a: p.wa.sin(),
            cos_a: p.wa.cos(),
            sin_b: p.wb.sin(),
            cos_b: p.wb.cos(),
            sin_c: p.wc.sin(),
            cos_c: p.wc.cos(),
            iso_c: self.iso.cos(),
            iso_s: self.iso.sin(),
            slab_h: sim.slab_h,
            slab_c: sim.slab_c,
            touring: if sim.touring_active() { 1.0 } else { 0.0 },
            dim: sim.dim as f32,
            width,
            height,
            nslice: sim.tour.nslice as f32,
            fog: if p.fog && sim.dim == 3 { 1.0 } else { 0.0 },
            wp: if p.wp { 1.0 } else { 0.0 },
            iso: if p.iso_on { 1.0 } else { 0.0 },
            base: self.base_size(sim.n, p.round),
            round: if p.round { 1.0 } else { 0.0 },
            tour_f: sim.tour.f,
            tour_n: sim.tour.n,
        }
    }
}
