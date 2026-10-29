use crate::geom::{Polygon, V3};
use crate::pattern::Pattern;
use crate::pipe::PipeInside;
use crate::spectrum::BAND_COUNT;

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
}

impl Scene {
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
