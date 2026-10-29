// First-order scattering by one surface in the Kirchhoff (physical optics)
// approximation, in the time domain: Rayleigh II for the rigid part and
// Rayleigh I for the pressure-release part, exact for an infinite plane
// (class B for finite surfaces; Cox 2015 JASA EL for the surface-element form).
// A point source adds the spherical wave's near term to the normal velocity,
// cos θ1 (jk + 1/r1), and spreads as r0/r1 (Sakurai 2017 for the same far and
// near terms in a rigid panel's time-domain response).

use crate::geom::{dot, norm, scale, sub, Polygon, V3};
use crate::scene::Scene;
use std::f64::consts::PI;

pub const OVERSAMPLING: usize = 2;
const SHORTEST_ELEMENT_METRES: f64 = 0.25e-3;
const LONGEST_ELEMENT_METRES: f64 = 0.5;
const ELEMENT_PER_DISTANCE: f64 = 1.0 / 12.0;
const CURVATURE_LENGTH_METRES: f64 = 0.016;
const SHORTEST_EDGE_ELEMENT_METRES: f64 = 2e-3;
const EDGE_ELEMENT_PER_DISTANCE: f64 = 1.0 / 50.0;
const NEAREST_DISTANCE_METRES: f64 = 0.5e-3;
const MOST_SUBPOINTS: usize = 16;
const DEEPEST_LEVEL: u32 = 40;
const JOIN_TOLERANCE_METRES: f64 = 1e-3;
const LONGEST_EXTENSION_METRES: f64 = 5.0;

// One group of four histograms per listener set (rigid far and near, soft far
// and near), each element weighted by the set's weight for its arrival.
pub struct Histograms {
    pub sets: Vec<[Vec<f64>; 4]>,
}

impl Histograms {
    pub fn new(sets: usize, length: usize) -> Histograms {
        Histograms { sets: (0..sets).map(|_| std::array::from_fn(|_| vec![0.0; length])).collect() }
    }

    pub fn len(&self) -> usize {
        self.sets[0][0].len()
    }

    pub fn add_scaled(&mut self, other: &Histograms, scale: f64) {
        for (mine, theirs) in self.sets.iter_mut().zip(&other.sets) {
            for (a, b) in mine.iter_mut().zip(theirs) {
                for (x, y) in a.iter_mut().zip(b) {
                    *x += scale * y;
                }
            }
        }
    }
}

struct Context<'a> {
    scene: &'a Scene,
    index: usize,
    domain: Polygon,
    lit_normal: V3,
    rate: f64,
    histograms: Histograms,
}

fn element_size(distance: f64) -> f64 {
    (distance * ELEMENT_PER_DISTANCE)
        .min((CURVATURE_LENGTH_METRES * distance).sqrt())
        .clamp(SHORTEST_ELEMENT_METRES, LONGEST_ELEMENT_METRES)
}

fn edge_element_size(distance: f64) -> f64 {
    element_size(distance).min((distance * EDGE_ELEMENT_PER_DISTANCE).max(SHORTEST_EDGE_ELEMENT_METRES))
}

pub fn scatter(scene: &Scene, index: usize, length: usize) -> Option<Histograms> {
    let surface = &scene.surfaces[index];
    let facing = scene.source_side(&surface.polygon);
    if facing.abs() < 1e-9 {
        return None;
    }
    let lit_normal = if facing > 0.0 { surface.polygon.normal } else { scale(surface.polygon.normal, -1.0) };
    let rate = scene.sample_rate * OVERSAMPLING as f64;
    let extension = (scene.speed_of_sound * length as f64 / rate).min(LONGEST_EXTENSION_METRES);
    let mut context = Context {
        scene,
        index,
        domain: domain(scene, index, extension),
        lit_normal,
        rate,
        histograms: Histograms::new(scene.sets(), length),
    };
    let (lo, hi) = context.domain.bounds();
    let size = (hi[0] - lo[0]).max(hi[1] - lo[1]);
    visit(&mut context, [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0], size, 0);
    Some(context.histograms)
}

// An edge shared with another surface at a right angle, where the other
// surface stands on the microphone's side (a concave join), is extended past
// the join, so that a right-angled wedge or corner keeps its exact image
// solution instead of diffracting from the join. Convex joins, such as the
// outside of a box, and other angles keep their edges (class B diffraction).
fn domain(scene: &Scene, index: usize, extension: f64) -> Polygon {
    let polygon = &scene.surfaces[index].polygon;
    let distances: Vec<f64> = (0..polygon.points.len())
        .map(|i| if joined(scene, index, polygon.edge(i)) { extension } else { 0.0 })
        .collect();
    polygon.offset(&distances)
}

const RIGHT_ANGLE_COSINE: f64 = 0.2;

const JOIN_PROBE_METRES: f64 = 5e-3;

pub fn extended_edges(scene: &Scene, index: usize) -> usize {
    let polygon = &scene.surfaces[index].polygon;
    (0..polygon.points.len()).filter(|&i| joined(scene, index, polygon.edge(i))).count()
}

fn joined(scene: &Scene, index: usize, (a, b): (V3, V3)) -> bool {
    let this = &scene.surfaces[index].polygon;
    let middle = scale(crate::geom::add(a, b), 0.5);
    let edge = sub(b, a);
    let microphone_side = this.signed_distance(scene.microphone).signum();
    scene.surfaces.iter().enumerate().any(|(j, other)| {
        let p = &other.polygon;
        if j == index
            || dot(this.normal, p.normal).abs() >= RIGHT_ANGLE_COSINE
            || p.signed_distance(a).abs() >= JOIN_TOLERANCE_METRES
            || p.signed_distance(b).abs() >= JOIN_TOLERANCE_METRES
        {
            return false;
        }
        let to_edge = sub(middle, scene.microphone);
        let reach = norm(to_edge);
        if reach > 1e-6 && scene.occluded(&[index, j], scene.microphone, scale(to_edge, 1.0 / reach), reach) {
            return false;
        }
        let across = crate::geom::normalize(crate::geom::cross(p.normal, edge));
        [1.0, -1.0].iter().any(|&sign| {
            let probe = crate::geom::add(middle, scale(across, sign * JOIN_PROBE_METRES));
            let q = p.to_plane(probe);
            other.is_solid(q) && this.signed_distance(probe).signum() == microphone_side
        })
    })
}

fn visit(context: &mut Context, centre: [f64; 2], size: f64, level: u32) {
    let surface = &context.scene.surfaces[context.index];
    let h = size / 2.0;
    let samples = [
        centre,
        [centre[0] - h, centre[1] - h],
        [centre[0] + h, centre[1] - h],
        [centre[0] - h, centre[1] + h],
        [centre[0] + h, centre[1] + h],
    ];
    let domain = &context.domain;
    let is_solid = |q: [f64; 2]| domain.contains(q) && surface.pattern.is_solid(q);
    let solid = samples.iter().filter(|&&q| is_solid(q)).count();
    let vertex_inside = domain.points.iter().any(|p| (p[0] - centre[0]).abs() <= h && (p[1] - centre[1]).abs() <= h);
    let x = domain.to_space(centre);
    let to_microphone = sub(context.scene.microphone, x);
    let distance = norm(to_microphone);
    let c = context.scene.speed_of_sound;
    let incident = context.scene.incident(x);
    let earliest = (incident.extra_metres + distance - size * std::f64::consts::SQRT_2) / c;
    if earliest * context.rate > context.histograms.len() as f64 {
        return;
    }
    let pattern_edge = surface.pattern.may_cross(centre, size);
    if solid == 0 && !vertex_inside && !pattern_edge {
        return;
    }
    let uniform = solid == samples.len() && !vertex_inside && !pattern_edge;
    let reach = distance * incident.source_distance / (distance + incident.source_distance);
    let reach = if reach.is_finite() { reach } else { distance };
    let target = if uniform { element_size(reach) } else { edge_element_size(reach) };
    if size <= target || level >= DEEPEST_LEVEL {
        if is_solid(centre) {
            element(context, centre, size);
        }
        return;
    }
    let q = size / 4.0;
    for (du, dv) in [(-q, -q), (q, -q), (-q, q), (q, q)] {
        visit(context, [centre[0] + du, centre[1] + dv], size / 2.0, level + 1);
    }
}

fn element(context: &mut Context, centre: [f64; 2], size: f64) {
    let scene = context.scene;
    let real = &scene.surfaces[context.index].polygon;
    let polygon = &context.domain;
    let x = polygon.to_space(centre);
    let skip = [context.index];
    let virtual_element = !real.contains(centre);
    let incident = scene.incident(x);
    if !virtual_element && scene.occluded(&skip, x, incident.towards_source, incident.source_distance) {
        return;
    }
    let to_microphone = sub(scene.microphone, x);
    let distance = norm(to_microphone).max(NEAREST_DISTANCE_METRES);
    let direction = scale(to_microphone, 1.0 / distance);
    let cos_scatter = dot(context.lit_normal, direction);
    if cos_scatter > 0.0 && !virtual_element && scene.occluded(&skip, x, direction, distance) {
        return;
    }
    let c = scene.speed_of_sound;
    let delay = (incident.extra_metres + distance) / c * context.rate;
    let (listener_delay, weights) = scene.split(scale(direction, -1.0), distance);
    let listener_shift = listener_delay * context.rate;
    let length = context.histograms.len();
    if delay >= (length - 2) as f64 {
        return;
    }
    let area = size * size * incident.amplitude;
    let rigid_far = cos_scatter * area / (2.0 * PI * distance);
    let rigid_near = rigid_far / distance;
    let cos_incidence = dot(context.lit_normal, incident.towards_source).max(0.0);
    let soft = cos_incidence * area / (2.0 * PI * distance);
    let soft_near = soft / incident.source_distance;
    let towards = incident.towards_source;
    let gradient = scale([
        -towards[0] - direction[0],
        -towards[1] - direction[1],
        -towards[2] - direction[2],
    ], size * context.rate / c);
    let spread_u = dot(gradient, polygon.u);
    let spread_v = dot(gradient, polygon.v);
    let count_u = ((spread_u.abs().ceil() as usize) + 1).min(MOST_SUBPOINTS);
    let count_v = ((spread_v.abs().ceil() as usize) + 1).min(MOST_SUBPOINTS);
    let share = 1.0 / (count_u * count_v) as f64;
    for i in 0..count_u {
        let offset_u = ((i as f64 + 0.5) / count_u as f64 - 0.5) * spread_u;
        for j in 0..count_v {
            let offset_v = ((j as f64 + 0.5) / count_v as f64 - 0.5) * spread_v;
            let t = (delay + listener_shift + offset_u + offset_v).max(0.0);
            for (set, weight) in weights.iter().enumerate().take(context.histograms.sets.len()) {
                let w = share * weight;
                deposit(&mut context.histograms.sets[set], t, [rigid_far * w, rigid_near * w, soft * w, soft_near * w]);
            }
        }
    }
}

fn deposit(lists: &mut [Vec<f64>; 4], t: f64, values: [f64; 4]) {
    let i = t.floor() as usize;
    if i + 1 >= lists[0].len() {
        return;
    }
    let f = t - i as f64;
    for (list, value) in lists.iter_mut().zip(values) {
        list[i] += value * (1.0 - f);
        list[i + 1] += value * f;
    }
}
