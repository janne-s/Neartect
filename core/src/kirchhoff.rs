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
const SHORTEST_ELEMENT_METRES: f64 = 2e-6;
const LONGEST_ELEMENT_METRES: f64 = 0.5;
const ELEMENT_PER_DISTANCE: f64 = 1.0 / 12.0;
const CURVATURE_LENGTH_METRES: f64 = 0.016;
const SHORTEST_EDGE_ELEMENT_METRES: f64 = 2e-3;
const EDGE_ELEMENT_PER_DISTANCE: f64 = 1.0 / 50.0;
const NEAREST_DISTANCE_METRES: f64 = 2e-5;
const MOST_SUBPOINTS: usize = 16;
const DEEPEST_LEVEL: u32 = 40;
const JOIN_TOLERANCE_METRES: f64 = 1e-3;
const LONGEST_EXTENSION_METRES: f64 = 5.0;

// One of `count` fixed shares of the Kirchhoff work: every traversal's
// quadtree is cut into cells at a fixed level, and the cells are dealt to the
// parts in turn. The parts' spectra sum to the whole one, and the cut does not
// depend on how many workers compute them.
#[derive(Clone, Copy, Debug)]
pub struct Part {
    pub index: usize,
    pub count: usize,
}

impl Part {
    pub const WHOLE: Part = Part { index: 0, count: 1 };
}

const CELL_LEVEL: u32 = 3;
const CELLS: usize = 1 << (2 * CELL_LEVEL);

pub const RIGID_FAR: usize = 0;
pub const RIGID_NEAR: usize = 1;
pub const SOFT_FAR: usize = 2;
pub const SOFT_NEAR: usize = 3;
pub const LISTS: usize = 4;

// Four histograms per listener set (rigid far and near, soft far and near),
// each element weighted by the set's weight for its arrival, stored bin by
// bin so that one element's deposit into every set is contiguous.
pub struct Histograms {
    sets: usize,
    bins: usize,
    values: Vec<f64>,
}

impl Histograms {
    pub fn new(sets: usize, bins: usize) -> Histograms {
        Histograms { sets, bins, values: vec![0.0; bins * sets * LISTS] }
    }

    pub fn len(&self) -> usize {
        self.bins
    }

    pub fn sets(&self) -> usize {
        self.sets
    }

    fn width(&self) -> usize {
        self.sets * LISTS
    }

    pub fn list(&self, set: usize, list: usize) -> impl Iterator<Item = f64> + '_ {
        self.values[set * LISTS + list..].iter().step_by(self.width()).copied()
    }
}

// A second reflection on a flat surface `j` joined at a right angle to a
// surface with relief: the relief is lit from the source's image in `j`, and
// its extension across the join (the relief mirrored) holds the other order,
// so both second-order paths take the relief's lobes, exactly so as the relief
// flattens. Other pairs and higher orders keep the images with Eckart's factor
// (a finite unjoined `j` would need its own diffraction).
pub fn via_pairs(scene: &Scene) -> Vec<(usize, usize)> {
    if scene.max_order < 2 || scene.pipe.is_some() {
        return Vec::new();
    }
    let mut list = Vec::new();
    for (i, s) in scene.surfaces.iter().enumerate() {
        if s.strength <= 0.0 || s.relief.is_flat() {
            continue;
        }
        for (j, other) in scene.surfaces.iter().enumerate() {
            if j == i || other.strength <= 0.0 || !other.relief.is_flat() {
                continue;
            }
            if joined_with(scene, i, j) {
                list.push((i, j));
            }
        }
    }
    list
}

fn joined_with(scene: &Scene, index: usize, other: usize) -> bool {
    let polygon = &scene.surfaces[index].polygon;
    (0..polygon.points.len()).any(|e| joined(scene, index, polygon.edge(e), Some(other)))
}

struct Context<'a> {
    scene: &'a Scene,
    index: usize,
    via: Option<usize>,
    domain: Polygon,
    lit_normal: V3,
    relief_element: f64,
    rate: f64,
    scale: f64,
    histograms: &'a mut Histograms,
    kernel: Vec<f64>,
    elements: usize,
    part: Part,
    traversal: usize,
}

fn element_size(distance: f64) -> f64 {
    (distance * ELEMENT_PER_DISTANCE)
        .min((CURVATURE_LENGTH_METRES * distance).sqrt())
        .clamp(SHORTEST_ELEMENT_METRES, LONGEST_ELEMENT_METRES)
}

fn edge_element_size(distance: f64) -> f64 {
    element_size(distance).min((distance * EDGE_ELEMENT_PER_DISTANCE).max(SHORTEST_EDGE_ELEMENT_METRES))
}

impl Context<'_> {
    fn owns(&self, cell: usize, level: u32) -> bool {
        let first = cell << (2 * CELL_LEVEL.saturating_sub(level));
        self.part.count <= 1 || (self.traversal * CELLS + first) % self.part.count == self.part.index
    }

    fn incident(&self, x: V3) -> crate::scene::Incident {
        match self.via {
            Some(j) => {
                let mirror = &self.scene.surfaces[j].polygon;
                let mut incident = self.scene.incident(mirror.mirror_point(x));
                incident.towards_source = mirror.mirror_direction(incident.towards_source);
                incident
            }
            None => self.scene.incident(x),
        }
    }

    // Whether the leg from `x` towards the source's image passes the via
    // surface where it is solid and is otherwise clear on both sides.
    fn through_via(&self, j: usize, x: V3, direction: V3, length: f64) -> bool {
        let surface = &self.scene.surfaces[j];
        let Some(t) = surface.polygon.ray_hit(x, direction, length) else { return false };
        let hit = crate::geom::add(x, scale(direction, t));
        if !surface.is_solid(surface.polygon.to_plane(hit)) || self.scene.occluded(&[self.index, j], x, direction, t) {
            return false;
        }
        let onward = surface.polygon.mirror_direction(direction);
        !self.scene.occluded(&[j], hit, onward, length - t)
    }
}

// Each surface's own scattering, then the second-order pairs that `via_pairs`
// gives to the Kirchhoff sum.
pub fn traversals(scene: &Scene) -> Vec<(usize, Option<usize>)> {
    let single = scene.surfaces.iter().enumerate().filter(|(_, s)| s.strength > 0.0).map(|(i, _)| (i, None));
    single.chain(via_pairs(scene).into_iter().map(|(i, j)| (i, Some(j)))).collect()
}

fn extension(scene: &Scene, bins: usize) -> f64 {
    (scene.speed_of_sound * bins as f64 / (scene.sample_rate * OVERSAMPLING as f64)).min(LONGEST_EXTENSION_METRES)
}

// The smallest relief element that keeps all traversals of surfaces with
// relief, extensions past joined edges included, within the element limit.
// Where it binds, the quadtree's root is sized so that leaves take exactly
// this size; halving the bounding square instead gave up to four times the
// limit.
pub fn relief_floor(scene: &Scene, traversals: &[(usize, Option<usize>)], bins: usize) -> f64 {
    let reach = extension(scene, bins);
    let area: f64 = traversals
        .iter()
        .filter(|&&(i, _)| !scene.surfaces[i].relief.is_flat())
        .map(|&(i, via)| domain(scene, i, reach, via).area)
        .sum();
    (area / scene.quality.element_limit).sqrt()
}

// Adds surface `index`'s scattering, times `strength`, to `histograms` and
// returns the number of elements it took; none when the surface is seen
// edge-on from the source.
#[allow(clippy::too_many_arguments)]
pub fn scatter_into(scene: &Scene, traversal: usize, index: usize, via: Option<usize>, strength: f64, relief_floor: f64, part: Part, histograms: &mut Histograms) -> Option<usize> {
    let surface = &scene.surfaces[index];
    let facing = match via {
        Some(j) => {
            let mirror = &scene.surfaces[j].polygon;
            match scene.source {
                None => dot(surface.polygon.normal, mirror.mirror_direction(scene.arrival)),
                Some(source) => surface.polygon.signed_distance(mirror.mirror_point(source)),
            }
        }
        None => scene.source_side(&surface.polygon),
    };
    if facing.abs() < 1e-9 {
        return None;
    }
    let lit_normal = if facing > 0.0 { surface.polygon.normal } else { scale(surface.polygon.normal, -1.0) };
    let rate = scene.sample_rate * OVERSAMPLING as f64;
    let reach = extension(scene, histograms.len());
    let relief = &surface.relief;
    let relief_element = if relief.is_flat() {
        0.0
    } else {
        (relief.finest() / scene.quality.relief_detail).max(relief_floor)
    };
    let mut context = Context {
        scene,
        index,
        via,
        domain: domain(scene, index, reach, via),
        lit_normal,
        relief_element,
        rate,
        scale: strength,
        histograms,
        kernel: Vec::new(),
        elements: 0,
        part,
        traversal,
    };
    let (lo, hi) = context.domain.bounds();
    let extent = (hi[0] - lo[0]).max(hi[1] - lo[1]);
    let limited = relief_element > 0.0 && relief_element == relief_floor;
    let size = if limited { relief_element * 2.0_f64.powi((extent / relief_element).log2().ceil().max(0.0) as i32) } else { extent };
    visit(&mut context, [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0], size, 0, 0);
    Some(context.elements)
}

// An edge shared with another surface at a right angle, where the other
// surface stands on the microphone's side (a concave join), is extended past
// the join, so that a right-angled wedge or corner keeps its exact image
// solution instead of diffracting from the join. Convex joins, such as the
// outside of a box, and other angles keep their edges (class B diffraction).
fn domain(scene: &Scene, index: usize, extension: f64, via: Option<usize>) -> Polygon {
    let polygon = &scene.surfaces[index].polygon;
    let distances: Vec<f64> = (0..polygon.points.len())
        .map(|i| if joined(scene, index, polygon.edge(i), via) { extension } else { 0.0 })
        .collect();
    polygon.offset(&distances)
}

const RIGHT_ANGLE_COSINE: f64 = 0.2;

const JOIN_PROBE_METRES: f64 = 5e-3;

pub fn extended_edges(scene: &Scene, index: usize) -> usize {
    let polygon = &scene.surfaces[index].polygon;
    (0..polygon.points.len()).filter(|&i| joined(scene, index, polygon.edge(i), None)).count()
}

fn joined(scene: &Scene, index: usize, (a, b): (V3, V3), only: Option<usize>) -> bool {
    let this = &scene.surfaces[index].polygon;
    let middle = scale(crate::geom::add(a, b), 0.5);
    let edge = sub(b, a);
    let microphone_side = this.signed_distance(scene.microphone).signum();
    scene.surfaces.iter().enumerate().any(|(j, other)| {
        let p = &other.polygon;
        if j == index
            || only.is_some_and(|o| o != j)
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

fn visit(context: &mut Context, centre: [f64; 2], size: f64, level: u32, cell: usize) {
    if level == CELL_LEVEL && !context.owns(cell, level) {
        return;
    }
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
    let incident = context.incident(x);
    let earliest = (incident.extra_metres + distance - size * std::f64::consts::SQRT_2 - 2.0 * surface.relief.bound()) / c;
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
    let target = if context.relief_element > 0.0 { target.min(context.relief_element) } else { target }.max(SHORTEST_ELEMENT_METRES);
    if size <= target || level >= DEEPEST_LEVEL {
        if is_solid(centre) && (level >= CELL_LEVEL || context.owns(cell, level)) {
            element(context, centre, size);
        }
        return;
    }
    let q = size / 4.0;
    for (quarter, (du, dv)) in [(-q, -q), (q, -q), (-q, q), (q, q)].into_iter().enumerate() {
        let child = if level < CELL_LEVEL { cell * 4 + quarter } else { cell };
        visit(context, [centre[0] + du, centre[1] + dv], size / 2.0, level + 1, child);
    }
}

fn element(context: &mut Context, centre: [f64; 2], size: f64) {
    context.elements += 1;
    let scene = context.scene;
    let real = &scene.surfaces[context.index].polygon;
    let polygon = &context.domain;
    let skip = [context.index];
    let virtual_element = !real.contains(centre);
    let relief = &scene.surfaces[context.index].relief;
    let shaped = !relief.is_flat();
    // An element past a joined edge stands for the surface's mirror image in
    // the joining surface, so it carries the relief mirrored across that edge.
    let (height, slope) = if !shaped {
        (0.0, [0.0, 0.0])
    } else if virtual_element {
        let (mirrored, edge_normal) = real.mirror_across_nearest_edge(centre);
        let (h, g) = relief.at(mirrored);
        let across = g[0] * edge_normal[0] + g[1] * edge_normal[1];
        (h, [g[0] - 2.0 * across * edge_normal[0], g[1] - 2.0 * across * edge_normal[1]])
    } else {
        relief.at(centre)
    };
    let normal = real.normal;
    let x = crate::geom::add(polygon.to_space(centre), scale(normal, height));
    let side = dot(context.lit_normal, normal);
    let tilted = scale(sub(normal, crate::geom::add(scale(polygon.u, slope[0]), scale(polygon.v, slope[1]))), side);
    let tangent_u = crate::geom::add(polygon.u, scale(normal, slope[0]));
    let tangent_v = crate::geom::add(polygon.v, scale(normal, slope[1]));
    let incident = context.incident(x);
    if shaped && dot(tilted, incident.towards_source) <= 0.0 {
        return;
    }
    let shades = |towards: V3| {
        let rise = dot(towards, normal);
        shaped && relief.shades(centre, height, [dot(towards, polygon.u), dot(towards, polygon.v)], rise, side.signum(), |q| real.contains(q))
    };
    if shades(incident.towards_source) {
        return;
    }
    let towards_source_clear = virtual_element
        || match context.via {
            Some(j) => context.through_via(j, x, incident.towards_source, incident.source_distance),
            None => !scene.occluded(&skip, x, incident.towards_source, incident.source_distance),
        };
    if !towards_source_clear {
        return;
    }
    let to_microphone = sub(scene.microphone, x);
    let distance = norm(to_microphone).max(NEAREST_DISTANCE_METRES);
    let direction = scale(to_microphone, 1.0 / distance);
    let cos_scatter = dot(tilted, direction);
    if cos_scatter > 0.0 && !virtual_element && scene.occluded(&skip, x, direction, distance) {
        return;
    }
    if cos_scatter > 0.0 && shades(direction) {
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
    let unshaded = if shaped {
        relief.rough_shadowing(dot(normal, incident.towards_source).abs()) * relief.rough_shadowing(dot(normal, direction).abs())
    } else {
        1.0
    };
    let area = size * size * incident.amplitude * unshaded;
    let rigid_far = cos_scatter * area / (2.0 * PI * distance);
    let rigid_near = rigid_far / distance;
    let cos_incidence = dot(tilted, incident.towards_source).max(0.0);
    let soft = cos_incidence * area / (2.0 * PI * distance);
    let soft_near = soft / incident.source_distance;
    let sets = context.histograms.sets();
    let mut values = [[0.0; LISTS]; crate::scene::MOST_SETS];
    for (set, weight) in weights.iter().enumerate().take(sets) {
        values[set] = [rigid_far * weight, rigid_near * weight, soft * weight, soft_near * weight];
    }
    // A first-order microphone's gradient part hears the rigid element's
    // dipole field exactly: the radial part takes 2/(jkR) where a spherical
    // wave takes 1/(jkR), and the field across the radius adds
    // (n·f − (n·u)(u·f)) (1 + 1/(jkR)) / R².
    if let crate::scene::Listener::Microphone { pattern, facing } = scene.listener {
        if pattern < 1.0 {
            let toward_facing = -dot(direction, facing);
            let across = (dot(tilted, facing) + cos_scatter * toward_facing) * area / (2.0 * PI * distance * distance);
            let radial = toward_facing * rigid_near;
            values[1][RIGID_FAR] = (1.0 - pattern) * (radial + across);
            values[1][RIGID_NEAR] = (1.0 - pattern) * (2.0 * radial + across) / distance;
        }
    }
    let towards = incident.towards_source;
    let gradient = scale([
        -towards[0] - direction[0],
        -towards[1] - direction[1],
        -towards[2] - direction[2],
    ], size * context.rate / c);
    let spread_u = dot(gradient, tangent_u);
    let spread_v = dot(gradient, tangent_v);
    let count_u = ((spread_u.abs().ceil() as usize) + 1).min(MOST_SUBPOINTS);
    let count_v = ((spread_v.abs().ceil() as usize) + 1).min(MOST_SUBPOINTS);
    let share = 1.0 / (count_u * count_v) as f64;
    let centre_time = delay + listener_shift;
    let reach = (spread_u.abs() + spread_v.abs()) / 2.0;
    let first = (centre_time - reach).max(0.0).floor() as usize;
    let last = (centre_time + reach).max(0.0).floor() as usize + 1;
    let kernel = &mut context.kernel;
    kernel.clear();
    kernel.resize(last - first + 1, 0.0);
    for i in 0..count_u {
        let offset_u = ((i as f64 + 0.5) / count_u as f64 - 0.5) * spread_u;
        for j in 0..count_v {
            let offset_v = ((j as f64 + 0.5) / count_v as f64 - 0.5) * spread_v;
            let t = (centre_time + offset_u + offset_v).max(0.0);
            let bin = t.floor() as usize;
            if bin + 1 >= length {
                continue;
            }
            let f = t - bin as f64;
            kernel[bin - first] += share * (1.0 - f);
            kernel[bin + 1 - first] += share * f;
        }
    }
    let width = context.histograms.width();
    let mut scaled = [0.0; LISTS * crate::scene::MOST_SETS];
    for (cell, value) in scaled.iter_mut().zip(values[..sets].iter().flatten()) {
        *cell = value * context.scale;
    }
    for (offset, &weight) in kernel.iter().enumerate() {
        if weight == 0.0 {
            continue;
        }
        let row = (first + offset) * width;
        for (cell, value) in context.histograms.values[row..row + width].iter_mut().zip(&scaled[..width]) {
            *cell += weight * value;
        }
    }
}
