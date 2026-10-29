use crate::geom::{Polygon, V3};
use crate::pattern::Pattern;
use crate::pipe::PipeInside;
use crate::spectrum::BAND_COUNT;

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
    pub pattern: f64,
    pub facing: V3,
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
    // First-order pickup a + (1 − a) cos θ for sound arriving from `from`
    // (a unit vector from the microphone towards where the sound comes from):
    // 1 omni, 0.5 cardioid, 0 figure of eight with its rear lobe inverted.
    pub fn pickup(&self, from: V3) -> f64 {
        self.pattern + (1.0 - self.pattern) * crate::geom::dot(self.facing, from)
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
