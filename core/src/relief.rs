// A surface's relief: a height field along its normal over its plane
// coordinates, from a repeating shape and from random roughness. The
// Kirchhoff sum moves each element to its height and tilts it to the local
// slope, so the coherent loss, the diffuse scatter, and the lobes of a
// repeating shape all come from the same sum (class B while slopes are
// gentle). The repeating shape shades itself along each ray; roughness shades
// statistically; nothing scatters twice within the surface. Roughness is a
// sum of cosines with random directions and phases around one grain size,
// fixed by a seed. The offset slides the whole field along its direction, so
// moving it in time moves the sound.

use crate::trig::{sin, sin_cos};
use std::f64::consts::PI;

const ROUGH_COMPONENTS: usize = 32;
const SHADOW_STEPS_PER_PERIOD: f64 = 16.0;
const MOST_SHADOW_STEPS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Flat,
    Waves,
    Arches,
    Scallops,
    Egg,
}

#[derive(Clone, Debug)]
pub struct Relief {
    pub shape: Shape,
    pub height: f64,
    pub period: f64,
    pub direction: [f64; 2],
    pub start: [f64; 2],
    pub offset: f64,
    pub rough_height: f64,
    pub grain: f64,
    components: Vec<([f64; 2], f64)>,
    rough_slope: f64,
}

fn random(seed: &mut u64) -> f64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    ((*seed >> 11) as f64) / ((1u64 << 53) as f64)
}

impl Relief {
    pub fn flat() -> Relief {
        Relief {
            shape: Shape::Flat,
            height: 0.0,
            period: 1.0,
            direction: [1.0, 0.0],
            start: [0.0, 0.0],
            offset: 0.0,
            rough_height: 0.0,
            grain: 1.0,
            components: Vec::new(),
            rough_slope: 0.0,
        }
    }

    // [shape, height, period, direction radians, offset, rough height, grain,
    // seed], with the direction in the surface's own plane coordinates.
    pub fn from_numbers(values: &[f64]) -> Relief {
        let angle = values[3];
        Relief::new(values[0], values[1], values[2], [angle.cos(), angle.sin()], [0.0, 0.0], values[4], values[5], values[6], values[7])
    }

    // `start` is where the surface's plane origin sits along the direction
    // and across it, so that every face of an object shares one field.
    #[allow(clippy::too_many_arguments)]
    pub fn new(shape: f64, height: f64, period: f64, direction: [f64; 2], start: [f64; 2], offset: f64, rough_height: f64, grain: f64, seed: f64) -> Relief {
        let shape_kind = match shape.round() as i64 {
            1 => Shape::Waves,
            2 => Shape::Arches,
            3 => Shape::Scallops,
            4 => Shape::Egg,
            _ => Shape::Flat,
        };
        let period = period.max(1e-3);
        let grain = grain.max(1e-3);
        let rough_height = rough_height.max(0.0);
        let mut state = (seed.abs() as u64).wrapping_add(0x9e3779b97f4a7c15);
        let components = if rough_height > 0.0 {
            (0..ROUGH_COMPONENTS)
                .map(|_| {
                    let k = 2.0 * PI / grain * (0.5 + random(&mut state));
                    let a = 2.0 * PI * random(&mut state);
                    ([k * a.cos(), k * a.sin()], 2.0 * PI * random(&mut state))
                })
                .collect()
        } else {
            Vec::new()
        };
        let mean_square_k = components.iter().map(|(k, _): &([f64; 2], f64)| k[0] * k[0] + k[1] * k[1]).sum::<f64>() / ROUGH_COMPONENTS as f64;
        Relief {
            shape: if height == 0.0 { Shape::Flat } else { shape_kind },
            height: height.abs(),
            period,
            direction,
            start,
            offset,
            rough_height,
            grain,
            components,
            rough_slope: rough_height * (mean_square_k / 2.0).sqrt(),
        }
    }

    pub fn is_flat(&self) -> bool {
        self.shape == Shape::Flat && self.components.is_empty()
    }

    // The shortest length the relief changes over, for the element size.
    pub fn finest(&self) -> f64 {
        let shape = if self.shape == Shape::Flat { f64::INFINITY } else { self.period };
        let rough = if self.components.is_empty() { f64::INFINITY } else { self.grain / 1.5 };
        shape.min(rough)
    }

    pub fn bound(&self) -> f64 {
        self.height + 3.0 * self.rough_height
    }

    pub fn rms(&self) -> f64 {
        let shape = self.height
            * match self.shape {
                Shape::Flat => 0.0,
                Shape::Waves => std::f64::consts::FRAC_1_SQRT_2,
                Shape::Arches | Shape::Scallops => (0.5 - 4.0 / (PI * PI)).sqrt(),
                Shape::Egg => 0.5,
            };
        (shape * shape + self.rough_height * self.rough_height).sqrt()
    }

    fn shape_height(&self, q: [f64; 2]) -> f64 {
        let d = self.direction;
        let s = q[0] * d[0] + q[1] * d[1] + self.start[0] - self.offset;
        let t = -q[0] * d[1] + q[1] * d[0] + self.start[1];
        let w = 2.0 * PI / self.period;
        match self.shape {
            Shape::Flat => 0.0,
            Shape::Waves => self.height * sin(w * s),
            Shape::Arches => self.height * sin(PI * s / self.period).abs(),
            Shape::Scallops => -self.height * sin(PI * s / self.period).abs(),
            Shape::Egg => self.height * sin(w * s) * sin(w * t),
        }
    }

    fn shape_steepest(&self) -> f64 {
        match self.shape {
            Shape::Flat => 0.0,
            Shape::Waves | Shape::Egg => self.height * 2.0 * PI / self.period,
            Shape::Arches | Shape::Scallops => self.height * PI / self.period,
        }
    }

    // Whether the repeating shape rises above a ray leaving the surface at
    // plane point `q` and height `height`, with `along` its step in plane
    // coordinates, `rise` its step along the normal per unit length, and
    // `side` +1 or −1 for the side it leaves from. Rays steeper than the
    // shape's steepest slope cannot be shaded and are not marched.
    pub fn shades(&self, q: [f64; 2], height: f64, along: [f64; 2], rise: f64, side: f64, inside: impl Fn([f64; 2]) -> bool) -> bool {
        let level = (along[0] * along[0] + along[1] * along[1]).sqrt();
        let rise = rise * side;
        if level < 1e-9 || rise >= self.shape_steepest() * level {
            return false;
        }
        let step = self.period / SHADOW_STEPS_PER_PERIOD / level;
        let reach = if rise > 0.0 { 2.0 * self.height / rise } else { f64::INFINITY };
        let mut travelled = step;
        for _ in 0..MOST_SHADOW_STEPS {
            if travelled > reach {
                return false;
            }
            let p = [q[0] + along[0] * travelled, q[1] + along[1] * travelled];
            if !inside(p) {
                return false;
            }
            if side * self.shape_height(p) > side * height + rise * travelled {
                return true;
            }
            travelled += step;
        }
        false
    }

    // The share of rough surface a ray at `cos_angle` from the normal sees
    // unshaded, for Gaussian slopes of the roughness's rms slope (Smith
    // 1967, class B; averaged over heights).
    pub fn rough_shadowing(&self, cos_angle: f64) -> f64 {
        if self.components.is_empty() || cos_angle >= 1.0 {
            return 1.0;
        }
        let slope = self.rough_slope;
        let mu = cos_angle.max(1e-6) / (1.0 - cos_angle * cos_angle).sqrt();
        let x = mu / (std::f64::consts::SQRT_2 * slope);
        let lambda = ((-x * x).exp() / (x * PI.sqrt()) - erfc(x)) / 2.0;
        1.0 / (1.0 + lambda)
    }

    // Height and its gradient in plane coordinates.
    pub fn at(&self, q: [f64; 2]) -> (f64, [f64; 2]) {
        let d = self.direction;
        let across = [-d[1], d[0]];
        let s = q[0] * d[0] + q[1] * d[1] + self.start[0] - self.offset;
        let t = q[0] * across[0] + q[1] * across[1] + self.start[1];
        let a = self.height;
        let w = 2.0 * PI / self.period;
        let (h, ds, dt) = match self.shape {
            Shape::Flat => (0.0, 0.0, 0.0),
            Shape::Waves => {
                let (sin_s, cos_s) = sin_cos(w * s);
                (a * sin_s, a * w * cos_s, 0.0)
            }
            Shape::Arches | Shape::Scallops => {
                let (sin_phase, cos_phase) = sin_cos(PI * s / self.period);
                let sign = if sin_phase >= 0.0 { 1.0 } else { -1.0 };
                let flip = if self.shape == Shape::Scallops { -1.0 } else { 1.0 };
                (flip * a * sin_phase.abs(), flip * a * PI / self.period * cos_phase * sign, 0.0)
            }
            Shape::Egg => {
                let (sin_s, cos_s) = sin_cos(w * s);
                let (sin_t, cos_t) = sin_cos(w * t);
                (a * sin_s * sin_t, a * w * cos_s * sin_t, a * w * sin_s * cos_t)
            }
        };
        let mut height = h;
        let mut gradient = [ds * d[0] + dt * across[0], ds * d[1] + dt * across[1]];
        if !self.components.is_empty() {
            let amplitude = self.rough_height * (2.0 / ROUGH_COMPONENTS as f64).sqrt();
            for (k, phase) in &self.components {
                let (sin, cos) = sin_cos(k[0] * s + k[1] * t + phase);
                let slope = -amplitude * sin;
                height += amplitude * cos;
                gradient[0] += slope * (k[0] * d[0] + k[1] * across[0]);
                gradient[1] += slope * (k[0] * d[1] + k[1] * across[1]);
            }
        }
        (height, gradient)
    }
}

// Abramowitz and Stegun 7.1.26, absolute error below 1.5e-7.
fn erfc(x: f64) -> f64 {
    if x < 0.0 {
        return 2.0 - erfc(-x);
    }
    let t = 1.0 / (1.0 + 0.3275911 * x);
    let poly = t * (0.254829592 + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))));
    poly * (-x * x).exp()
}
