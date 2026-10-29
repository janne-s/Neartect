use crate::geom::{Polygon, V3};
use crate::pattern::Pattern;
use crate::pipe::PipeInside;
use crate::spectrum::BAND_COUNT;
use rustfft::num_complex::Complex64;

const NEAREST_SOURCE_METRES: f64 = 1e-3;

#[derive(Clone)]
pub struct Surface {
    pub polygon: Polygon,
    pub strength: f64,
    pub reflection: [f64; BAND_COUNT],
    pub pattern: Pattern,
}

impl Surface {
    pub fn is_solid(&self, q: [f64; 2]) -> bool {
        self.polygon.contains(q) && self.pattern.is_solid(q)
    }
}

#[derive(Clone)]
pub struct Scene {
    pub surfaces: Vec<Surface>,
    pub microphone: V3,
    pub arrival: V3,
    pub speed_of_sound: f64,
    pub sample_rate: f64,
    pub length: usize,
    pub max_order: usize,
    pub pipe: Option<PipeInside>,
    pub source: Option<V3>,
    pub gain: f64,
    pub delay_seconds: f64,
    pub listener: Listener,
}

// What hears the field. A first-order microphone takes a + (1 − a) cos θ of
// each arrival, and its gradient part grows by 1 + 1/(jkr) for a wave from a
// distance r (the proximity effect). An ear on a spherical head (Brown and
// Duda 1998) takes a head shadow (1 + jαω/2ω0)/(1 + jω/2ω0), ω0 = c/a, with α
// set by the angle from the ear's axis, and Woodworth's delay.
#[derive(Clone, Copy, Debug)]
pub enum Listener {
    Microphone { pattern: f64, facing: V3 },
    Ear { axis: V3, radius: f64 },
}

const PROXIMITY_FLOOR_HZ: f64 = 10.0;
const SHADOW_DEEPEST: f64 = 0.1;
const SHADOW_DEEPEST_RADIANS: f64 = 150.0 * std::f64::consts::PI / 180.0;

impl Listener {
    pub fn omni() -> Listener {
        Listener::Microphone { pattern: 1.0, facing: [0.0, 1.0, 0.0] }
    }
}

// The incident wave at a point, relative to the direct sound at the
// microphone: amplitude, extra path in metres, and the unit vector towards
// the source. A far source is a plane wave from `arrival`; a point source
// spreads spherically.
pub struct Incident {
    pub amplitude: f64,
    pub extra_metres: f64,
    pub towards_source: V3,
    pub source_distance: f64,
}

impl Scene {
    // Weights a listener puts on an arrival `from` (a unit vector towards where
    // it comes from) at `distance` metres, one per spectral set, and the extra
    // delay in seconds. Sets: a microphone's plain pickup and its proximity
    // part; an ear's two halves of the head-shadow filter.
    pub fn sets(&self) -> usize {
        match self.listener {
            Listener::Microphone { pattern, .. } if pattern >= 1.0 => 1,
            _ => 2,
        }
    }

    pub fn split(&self, from: V3, distance: f64) -> (f64, [f64; 2]) {
        match self.listener {
            Listener::Microphone { pattern, facing } => {
                let gradient = (1.0 - pattern) * crate::geom::dot(facing, from);
                let near = if distance.is_finite() { gradient / distance.max(1e-3) } else { 0.0 };
                (0.0, [pattern + gradient, near])
            }
            Listener::Ear { axis, radius } => {
                let theta = crate::geom::dot(axis, from).clamp(-1.0, 1.0).acos();
                let c = self.speed_of_sound;
                let half = std::f64::consts::FRAC_PI_2;
                let delay = if theta < half { -radius / c * theta.cos() } else { radius / c * (theta - half) } + radius / c;
                let alpha = (1.0 + SHADOW_DEEPEST / 2.0) + (1.0 - SHADOW_DEEPEST / 2.0) * (theta / SHADOW_DEEPEST_RADIANS * std::f64::consts::PI).cos();
                (delay, [1.0, alpha])
            }
        }
    }

    pub fn set_factor(&self, set: usize, omega: f64) -> Complex64 {
        let j = Complex64::new(0.0, 1.0);
        match self.listener {
            Listener::Microphone { .. } => {
                if set == 0 {
                    Complex64::new(1.0, 0.0)
                } else {
                    Complex64::new(self.speed_of_sound, 0.0) / (j * omega + 2.0 * std::f64::consts::PI * PROXIMITY_FLOOR_HZ)
                }
            }
            Listener::Ear { radius, .. } => {
                let x = j * omega * radius / (2.0 * self.speed_of_sound);
                if set == 0 { Complex64::new(1.0, 0.0) / (1.0 + x) } else { x / (1.0 + x) }
            }
        }
    }

    pub fn response(&self, from: V3, distance: f64, omega: f64) -> Complex64 {
        let (delay, weights) = self.split(from, distance);
        let sum: Complex64 = (0..self.sets()).map(|i| weights[i] * self.set_factor(i, omega)).sum();
        sum * Complex64::from_polar(1.0, -omega * delay)
    }

    pub fn direct_distance(&self) -> f64 {
        match self.source {
            None => f64::INFINITY,
            Some(source) => crate::geom::norm(crate::geom::sub(source, self.microphone)),
        }
    }

    pub fn direct_from(&self) -> V3 {
        match self.source {
            None => self.arrival,
            Some(source) => crate::geom::normalize(crate::geom::sub(source, self.microphone)),
        }
    }

    pub fn incident(&self, x: V3) -> Incident {
        match self.source {
            None => Incident {
                amplitude: 1.0,
                extra_metres: crate::geom::dot(self.arrival, crate::geom::sub(self.microphone, x)),
                towards_source: self.arrival,
                source_distance: f64::INFINITY,
            },
            Some(source) => {
                let to_source = crate::geom::sub(source, x);
                let r1 = crate::geom::norm(to_source).max(NEAREST_SOURCE_METRES);
                let r0 = crate::geom::norm(crate::geom::sub(source, self.microphone)).max(NEAREST_SOURCE_METRES);
                Incident {
                    amplitude: r0 / r1,
                    extra_metres: r1 - r0,
                    towards_source: crate::geom::scale(to_source, 1.0 / r1),
                    source_distance: r1,
                }
            }
        }
    }

    pub fn source_side(&self, polygon: &Polygon) -> f64 {
        match self.source {
            None => crate::geom::dot(polygon.normal, self.arrival),
            Some(source) => polygon.signed_distance(source),
        }
    }

    pub fn polygons(&self) -> Vec<&Polygon> {
        self.surfaces.iter().map(|s| &s.polygon).collect()
    }

    pub fn occluded(&self, skip: &[usize], from: V3, direction: V3, max_length: f64) -> bool {
        self.surfaces.iter().enumerate().any(|(i, s)| {
            if skip.contains(&i) {
                return false;
            }
            match s.polygon.ray_hit(from, direction, max_length) {
                Some(t) => {
                    let hit = crate::geom::add(from, crate::geom::scale(direction, t));
                    s.pattern.is_solid(s.polygon.to_plane(hit))
                }
                None => false,
            }
        })
    }
}
