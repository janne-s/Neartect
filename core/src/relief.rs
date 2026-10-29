// A surface's relief: a height field along its normal over its plane
// coordinates, from a repeating shape and from random roughness. The
// Kirchhoff sum moves each element to its height and tilts it to the local
// slope, so the coherent loss, the diffuse scatter, and the lobes of a
// repeating shape all come from the same sum (class B while slopes are
// gentle; the surface does not shade itself or scatter twice). Roughness is a
// sum of cosines with random directions and phases around one grain size,
// fixed by a seed. The offset slides the whole field along its direction, so
// moving it in time moves the sound.

use std::f64::consts::PI;

const ROUGH_COMPONENTS: usize = 32;

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
}

fn random(seed: &mut u64) -> f64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    ((*seed >> 11) as f64) / ((1u64 << 53) as f64)
}

impl Relief {
    pub fn flat() -> Relief {
        Relief { shape: Shape::Flat, height: 0.0, period: 1.0, direction: [1.0, 0.0], start: [0.0, 0.0], offset: 0.0, rough_height: 0.0, grain: 1.0, components: Vec::new() }
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
        let shape = self.height * match self.shape {
            Shape::Flat => 0.0,
            Shape::Waves => std::f64::consts::FRAC_1_SQRT_2,
            Shape::Arches | Shape::Scallops => (0.5 - 4.0 / (PI * PI)).sqrt(),
            Shape::Egg => 0.5,
        };
        (shape * shape + self.rough_height * self.rough_height).sqrt()
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
            Shape::Waves => (a * (w * s).sin(), a * w * (w * s).cos(), 0.0),
            Shape::Arches | Shape::Scallops => {
                let phase = PI * s / self.period;
                let sign = if phase.sin() >= 0.0 { 1.0 } else { -1.0 };
                let flip = if self.shape == Shape::Scallops { -1.0 } else { 1.0 };
                (flip * a * phase.sin().abs(), flip * a * PI / self.period * phase.cos() * sign, 0.0)
            }
            Shape::Egg => (
                a * (w * s).sin() * (w * t).sin(),
                a * w * (w * s).cos() * (w * t).sin(),
                a * w * (w * s).sin() * (w * t).cos(),
            ),
        };
        let mut height = h;
        let mut gradient = [ds * d[0] + dt * across[0], ds * d[1] + dt * across[1]];
        if !self.components.is_empty() {
            let amplitude = self.rough_height * (2.0 / ROUGH_COMPONENTS as f64).sqrt();
            for (k, phase) in &self.components {
                let arg = k[0] * s + k[1] * t + phase;
                let slope = -amplitude * arg.sin();
                height += amplitude * arg.cos();
                gradient[0] += slope * (k[0] * d[0] + k[1] * across[0]);
                gradient[1] += slope * (k[0] * d[1] + k[1] * across[1]);
            }
        }
        (height, gradient)
    }
}
