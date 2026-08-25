//! Grand tour: smoothly rotate the 3D viewing plane through `amb`-dimensional
//! space. Faithful port of the JS tour in index.html (T object, tourReset,
//! newLeg, tourAdvance, compl_).
//!
//! Maximum ambient dimension is 24; projected dimension PDIM = 3. The frame T.F is
//! PDIM orthonormal vectors in `amb` space; T.N are the `nslice = amb-PDIM`
//! slice normals used by the slice test.

use crate::rng::Rng;

pub const MAX_DIM: usize = 24;
pub const PDIM: usize = 3;
pub const MAX_SLICE_DIMS: usize = MAX_DIM - PDIM;

type VecN = [f32; MAX_DIM];

fn dv(a: &VecN, b: &VecN) -> f32 {
    let mut s = 0.0;
    for k in 0..MAX_DIM {
        s += a[k] * b[k];
    }
    s
}

/// Normalize in place; returns false if the vector is degenerate.
fn nz(a: &mut VecN) -> bool {
    let n = dv(a, a).sqrt();
    if n < 1e-12 {
        return false;
    }
    for value in a.iter_mut() {
        *value /= n;
    }
    true
}

/// Gram-Schmidt: subtract the projection of `a` onto each vector in `l`.
fn og(a: &mut VecN, l: &[VecN]) {
    for li in l {
        let d = dv(a, li);
        for k in 0..MAX_DIM {
            a[k] -= d * li[k];
        }
    }
}

pub struct Tour {
    /// Current frame: PDIM orthonormal vectors in amb space.
    pub f: [VecN; PDIM],
    /// Anchors (frame at start of leg).
    a: [VecN; PDIM],
    /// Perpendicular sweep direction per projected axis.
    u: [VecN; PDIM],
    /// Goal frame (random orthonormal basis in amb space).
    g: [VecN; PDIM],
    /// Working frame during interpolation.
    h: [VecN; PDIM],
    /// Slice normals (nslice of them used).
    pub n: [VecN; MAX_SLICE_DIMS],
    /// Rotation angle per axis for this leg.
    tau: [f32; PDIM],
    /// Orthonormal coefficient matrix relating H to F (kept from new_leg).
    v: [[f32; PDIM]; PDIM],
    /// Interpolation parameter 0..1.
    pub t: f32,
    /// Frames per leg.
    leg_f: u32,
    /// Leg counter (for readouts).
    pub leg: u32,
    /// Ambient dims actually in play (max(PDIM+1, min(MAX_DIM, dim))).
    pub amb: usize,
    /// Number of slice normals in use (amb - PDIM).
    pub nslice: usize,
}

impl Tour {
    pub fn new() -> Self {
        Tour {
            f: [[0.0; MAX_DIM]; PDIM],
            a: [[0.0; MAX_DIM]; PDIM],
            u: [[0.0; MAX_DIM]; PDIM],
            g: [[0.0; MAX_DIM]; PDIM],
            h: [[0.0; MAX_DIM]; PDIM],
            n: [[0.0; MAX_DIM]; MAX_SLICE_DIMS],
            tau: [0.0; PDIM],
            v: [[0.0; PDIM]; PDIM],
            t: 1.0,
            leg_f: 420,
            leg: 0,
            amb: MAX_DIM,
            nslice: MAX_SLICE_DIMS,
        }
    }

    /// Reset to the axis-aligned frame for a given dimensionality.
    pub fn reset(&mut self, dim: u32) {
        self.amb = (PDIM as u32 + 1).max(dim.min(MAX_DIM as u32)) as usize;
        self.nslice = self.amb - PDIM;
        for i in 0..PDIM {
            self.f[i] = [0.0; MAX_DIM];
            self.f[i][i] = 1.0;
        }
        self.n.fill([0.0; MAX_DIM]);
        self.t = 1.0;
        self.leg = 0;
        self.compl();
    }

    /// Complete the slice normals so {F..., N...} spans the ambient space.
    pub fn compl(&mut self) {
        let mut found = 0usize;
        for e in 0..self.amb {
            if found >= self.nslice {
                break;
            }
            let mut c = [0.0f32; MAX_DIM];
            c[e] = 1.0;
            og(&mut c, &self.f);
            og(&mut c, &self.n[..found]);
            if dv(&c, &c).sqrt() > 0.25 && nz(&mut c) {
                self.n[found] = c;
                found += 1;
            }
        }
        while found < self.nslice {
            self.n[found] = [0.0; MAX_DIM];
            found += 1;
        }
    }

    fn new_leg(&mut self, rng: &mut Rng, leg_secs: f32) {
        for i in 0..PDIM {
            self.a[i] = self.f[i];
        }

        // Random orthonormal PDIM basis in PDIM coefficient space (Gram-Schmidt).
        let mut v = [[0.0f32; PDIM]; PDIM];
        for row in &mut v {
            for value in row.iter_mut() {
                *value = rng.gaussian();
            }
        }
        for i in 0..PDIM {
            let (previous, current) = v.split_at_mut(i);
            let row = &mut current[0];
            for prior in previous {
                let mut d = 0.0;
                for (&value, &prior_value) in row.iter().zip(prior.iter()) {
                    d += value * prior_value;
                }
                for (value, &prior_value) in row.iter_mut().zip(prior.iter()) {
                    *value -= d * prior_value;
                }
            }
            let n = (row[0] * row[0] + row[1] * row[1] + row[2] * row[2])
                .sqrt()
                .max(1e-12);
            for value in row.iter_mut() {
                *value /= n;
            }
        }
        self.v = v;

        // Goal frame G = combination of anchors by V.
        for (i, row) in v.iter().enumerate() {
            let mut gi = [0.0f32; MAX_DIM];
            for (&coefficient, anchor) in row.iter().zip(self.a.iter()) {
                for (value, &anchor_value) in gi.iter_mut().zip(anchor.iter()) {
                    *value += coefficient * anchor_value;
                }
            }
            self.g[i] = gi;
        }

        // Perpendicular sweep directions + angles.
        for i in 0..PDIM {
            if i < self.nslice {
                let mut u = [0.0f32; MAX_DIM];
                for value in &mut u {
                    *value = rng.gaussian();
                }
                og(&mut u, &self.a);
                og(&mut u, &self.u[..i]);
                if !nz(&mut u) {
                    u = [0.0; MAX_DIM];
                }
                self.u[i] = u;
                self.tau[i] = 0.35 + rng.next_f32() * (std::f32::consts::FRAC_PI_2 - 0.35);
            } else {
                self.u[i] = [0.0; MAX_DIM];
                self.tau[i] = 0.0;
            }
        }

        self.t = 0.0;
        self.leg += 1;
        self.leg_f = (leg_secs * 60.0).round().max(30.0) as u32;
    }

    /// Advance the tour one frame.
    pub fn advance(&mut self, rng: &mut Rng, leg_secs: f32) {
        if self.t >= 1.0 {
            self.new_leg(rng, leg_secs);
        }
        self.t = (self.t + 1.0 / self.leg_f as f32).min(1.0);

        // H = G*cos(tau*t) + U*sin(tau*t)
        for i in 0..PDIM {
            let c = (self.tau[i] * self.t).cos();
            let s = (self.tau[i] * self.t).sin();
            for k in 0..self.amb {
                self.h[i][k] = self.g[i][k] * c + self.u[i][k] * s;
            }
        }
        // F = V^T * H  (V recomputed implicitly: F[i] = sum_j V[j][i]*H[j])
        // The JS keeps V from new_leg; we recompute it the same way by storing
        // it. To stay faithful we keep V in the struct.
        for i in 0..PDIM {
            let mut f = [0.0f32; MAX_DIM];
            for j in 0..PDIM {
                let c2 = self.v[j][i];
                let h2 = self.h[j];
                for k in 0..self.amb {
                    f[k] += c2 * h2[k];
                }
            }
            self.f[i] = f;
        }
        // Re-orthonormalize.
        for i in 0..PDIM {
            let mut fi = self.f[i];
            og(&mut fi, &self.f[..i]);
            nz(&mut fi);
            self.f[i] = fi;
        }
        self.compl();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_24_uses_full_orthonormal_frame() {
        let mut tour = Tour::new();
        tour.reset(24);

        assert_eq!(tour.amb, 24);
        assert_eq!(tour.nslice, 21);
        for i in 0..PDIM {
            for j in 0..PDIM {
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!((dv(&tour.f[i], &tour.f[j]) - expected).abs() < 1e-6);
            }
        }
    }
}
