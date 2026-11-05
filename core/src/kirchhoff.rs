// Class B: time-domain Kirchhoff sum (Cox 2015), exact for an infinite plane.

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
const SHADOW_PROBES: usize = 16;

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

pub struct Histograms {
    sets: usize,
    bins: usize,
    values: Vec<f64>,
}

impl Histograms {
    pub fn new(sets: usize, bins: usize) -> Histograms {
        Histograms { sets, bins, values: vec![0.0; bins * sets * LISTS] }
    }

    pub fn bins(&self) -> usize {
        self.bins
    }

    pub fn sets(&self) -> usize {
        self.sets
    }

    fn width(&self) -> usize {
        self.sets * LISTS
    }

    pub fn edge_correct_rigid(&mut self, tau_bins: f64) {
        if !tau_bins.is_finite() {
            return;
        }
        let [b0, b1, b2, a1, a2] = crate::edge::biquad(2.0 * tau_bins);
        let width = self.width();
        for set in 0..self.sets {
            for list in [RIGID_FAR, RIGID_NEAR] {
                let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
                for value in self.values[set * LISTS + list..].iter_mut().step_by(width) {
                    let y = b0 * *value + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
                    (x2, x1, y2, y1) = (x1, *value, y1, y);
                    *value = y;
                }
            }
        }
    }

    pub fn list(&self, set: usize, list: usize) -> impl Iterator<Item = f64> + '_ {
        self.values[set * LISTS + list..].iter().step_by(self.width()).copied()
    }
}

// No cutoff_hz on a joined surface: the join extension holds it.
#[derive(Clone, Copy, Debug)]
pub struct Via {
    pub surface: usize,
    pub receiver: bool,
    pub cutoff_hz: Option<f64>,
    pub shadow: bool,
}

pub fn via_pairs(scene: &Scene) -> Vec<(usize, Via)> {
    if scene.max_order < 2 || scene.pipe.is_some() {
        return Vec::new();
    }
    let mut list = Vec::new();
    for (i, s) in scene.surfaces.iter().enumerate() {
        if s.strength <= 0.0 || s.relief.is_flat() {
            continue;
        }
        for (j, other) in scene.surfaces.iter().enumerate() {
            if j == i || other.strength <= 0.0 || !other.relief.is_flat() || other.is_bent() {
                continue;
            }
            if joined_with(scene, i, j) {
                list.push((i, Via { surface: j, receiver: false, cutoff_hz: None, shadow: false }));
                continue;
            }
            for (receiver, sequence) in [(false, [i, j]), (true, [j, i])] {
                let Some(path) = crate::images::path_through(scene, &sequence) else { continue };
                let cutoff_hz = path.reflections.iter().find(|r| r.surface == j).map(|r| r.cutoff_hz);
                list.push((i, Via { surface: j, receiver, cutoff_hz, shadow: false }));
            }
        }
    }
    list
}

fn hits(surface: &crate::scene::Surface, from: V3, direction: V3, length: f64) -> bool {
    surface.ray_hit(from, direction, length).is_some_and(|(_, q)| surface.pattern.is_solid(q))
}

fn screens_of(scene: &Scene, index: usize) -> Vec<(usize, bool)> {
    let wall = &scene.surfaces[index];
    if scene.pipe.is_some() || wall.strength <= 0.0 || wall.is_bent() {
        return Vec::new();
    }
    let polygon = &wall.polygon;
    let side = polygon.signed_distance(scene.microphone);
    if side * scene.source_side(polygon) <= 0.0 {
        return Vec::new();
    }
    let (lo, hi) = polygon.bounds();
    let probes: Vec<V3> = (0..SHADOW_PROBES * SHADOW_PROBES)
        .map(|n| [lo[0] + ((n / SHADOW_PROBES) as f64 + 0.5) / SHADOW_PROBES as f64 * (hi[0] - lo[0]), lo[1] + ((n % SHADOW_PROBES) as f64 + 0.5) / SHADOW_PROBES as f64 * (hi[1] - lo[1])])
        .filter(|&q| wall.is_solid(q))
        .map(|q| polygon.to_space(q))
        .collect();
    let blockers = |from: V3, direction: V3, length: f64| -> Vec<usize> { scene.surfaces.iter().enumerate().filter(|&(i, s)| i != index && hits(s, from, direction, length)).map(|(i, _)| i).collect() };
    let mut screens: Vec<(usize, bool)> = Vec::new();
    for &x in &probes {
        let incident = scene.incident(x);
        let to_microphone = sub(scene.microphone, x);
        let reach = norm(to_microphone);
        if reach <= 1e-9 {
            continue;
        }
        let source_leg = blockers(x, incident.towards_source, incident.source_distance);
        let microphone_leg = blockers(x, scale(to_microphone, 1.0 / reach), reach);
        let single = match (source_leg.as_slice(), microphone_leg.as_slice()) {
            ([i], []) => Some((*i, true)),
            ([], [i]) => Some((*i, false)),
            _ => None,
        };
        if let Some((i, receiver)) = single {
            if scene.surfaces[i].strength > 0.0 && !screens.contains(&(i, receiver)) && !touches(scene, i, index) && in_front(scene, i, index, side) {
                screens.push((i, receiver));
            }
        }
    }
    screens
}

fn in_front(scene: &Scene, screen: usize, wall: usize, side: f64) -> bool {
    let surface = &scene.surfaces[screen];
    let plane = &scene.surfaces[wall].polygon;
    let n = surface.polygon.points.len();
    (0..n).all(|e| {
        let (a, b) = (surface.polygon.points[e], surface.polygon.points[(e + 1) % n]);
        [a, [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]].iter().all(|&q| plane.signed_distance(surface.point(q)) * side.signum() > JOIN_TOLERANCE_METRES)
    })
}

fn touches(scene: &Scene, a: usize, b: usize) -> bool {
    let polygon = &scene.surfaces[a].polygon;
    (0..polygon.points.len()).any(|e| has_edge(&scene.surfaces[b].polygon, polygon.edge(e))) || joined_with(scene, a, b)
}

pub fn shadow_pairs(scene: &Scene) -> Vec<(usize, Via)> {
    (0..scene.surfaces.len()).flat_map(|j| screens_of(scene, j).into_iter().map(move |(i, receiver)| (i, Via { surface: j, receiver, cutoff_hz: None, shadow: true }))).collect()
}

fn joined_with(scene: &Scene, index: usize, other: usize) -> bool {
    let polygon = &scene.surfaces[index].polygon;
    (0..polygon.points.len()).any(|e| joined(scene, index, e, Some(other)))
}

struct Context<'a> {
    scene: &'a Scene,
    index: usize,
    via: Option<Via>,
    microphone: V3,
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
    gradients: Vec<(usize, f64, V3)>,
    source_skip: Vec<usize>,
    microphone_skip: Vec<usize>,
}

fn element_size(distance: f64) -> f64 {
    (distance * ELEMENT_PER_DISTANCE).min((CURVATURE_LENGTH_METRES * distance).sqrt()).clamp(SHORTEST_ELEMENT_METRES, LONGEST_ELEMENT_METRES)
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
            Some(via) if !via.receiver => {
                let mirror = &self.scene.surfaces[via.surface].polygon;
                let mut incident = self.scene.incident(mirror.mirror_point(x));
                incident.towards_source = mirror.mirror_direction(incident.towards_source);
                incident
            }
            _ => self.scene.incident(x),
        }
    }

    fn through_via(&self, via: Via, x: V3, direction: V3, length: f64) -> bool {
        let j = via.surface;
        let surface = &self.scene.surfaces[j];
        let t = if via.cutoff_hz.is_none() {
            let Some(t) = surface.polygon.ray_hit(x, direction, length) else { return false };
            let hit = crate::geom::add(x, scale(direction, t));
            if !surface.is_solid(surface.polygon.to_plane(hit)) {
                return false;
            }
            t
        } else {
            let along = dot(direction, surface.polygon.normal);
            let t = -surface.polygon.signed_distance(x) / along;
            if along.abs() < 1e-12 || t <= 1e-7 || t >= length - 1e-7 {
                return false;
            }
            t
        };
        let hit = crate::geom::add(x, scale(direction, t));
        if self.scene.occluded(&[self.index, j], x, direction, t) {
            return false;
        }
        let onward = surface.polygon.mirror_direction(direction);
        !self.scene.occluded(&[j], hit, onward, length - t)
    }
}

pub fn traversals(scene: &Scene) -> Vec<(usize, Option<Via>)> {
    let single = scene.surfaces.iter().enumerate().filter(|(_, s)| s.strength > 0.0).map(|(i, _)| (i, None));
    single.chain(via_pairs(scene).into_iter().chain(shadow_pairs(scene)).map(|(i, via)| (i, Some(via)))).collect()
}

fn extension(scene: &Scene, bins: usize) -> f64 {
    (scene.speed_of_sound * bins as f64 / (scene.sample_rate * OVERSAMPLING as f64)).min(LONGEST_EXTENSION_METRES)
}

pub fn relief_floor(scene: &Scene, traversals: &[(usize, Option<Via>)], bins: usize) -> f64 {
    let reach = extension(scene, bins);
    let area: f64 = traversals.iter().filter(|&&(i, _)| !scene.surfaces[i].relief.is_flat()).map(|&(i, via)| domain(scene, i, reach, via.map(|v| v.surface)).area).sum();
    (area / scene.quality.element_limit).sqrt()
}

fn same_edge((a, b): (V3, V3), (c, d): (V3, V3)) -> bool {
    let close = |p: V3, q: V3| norm(sub(p, q)) < JOIN_TOLERANCE_METRES;
    (close(a, c) && close(b, d)) || (close(a, d) && close(b, c))
}

fn has_edge(polygon: &Polygon, edge: (V3, V3)) -> bool {
    (0..polygon.points.len()).any(|e| same_edge(polygon.edge(e), edge))
}

fn shares(scene: &Scene, index: usize, edge: (V3, V3)) -> bool {
    scene.surfaces.iter().enumerate().any(|(j, other)| j != index && has_edge(&other.polygon, edge))
}

fn body_scale(scene: &Scene, index: usize) -> f64 {
    let mut body = vec![index];
    let mut at = 0;
    while at < body.len() {
        let polygon = &scene.surfaces[body[at]].polygon;
        for e in 0..polygon.points.len() {
            let edge = polygon.edge(e);
            for (j, other) in scene.surfaces.iter().enumerate() {
                if !body.contains(&j) && has_edge(&other.polygon, edge) {
                    body.push(j);
                }
            }
        }
        at += 1;
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for &i in &body {
        let polygon = &scene.surfaces[i].polygon;
        for &q in &polygon.points {
            let p = polygon.to_space(q);
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
    }
    let mut extents = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    extents.sort_by(f64::total_cmp);
    extents[1] / 2.0
}

fn screen(scene: &Scene, index: usize, bins: usize, via: Option<usize>) -> Polygon {
    let polygon = &scene.surfaces[index].polygon;
    let reach = extension(scene, bins);
    let scale = body_scale(scene, index);
    let distances: Vec<f64> = (0..polygon.points.len())
        .map(|i| {
            let edge = polygon.edge(i);
            if joined(scene, index, i, via) {
                reach
            } else if shares(scene, index, edge) {
                scale
            } else {
                0.0
            }
        })
        .collect();
    polygon.offset(&distances)
}

pub fn plain_depth(scene: &Scene, index: usize, bins: usize) -> f64 {
    crate::edge::mean_depth(&screen(scene, index, bins, None), &scene.surfaces[index].pattern)
}

pub fn edge_depth(scene: &Scene, index: usize, via: Option<usize>, bins: usize) -> (f64, f64) {
    let surface = &scene.surfaces[index];
    let domain = screen(scene, index, bins, via);
    crate::edge::effective_depth(&domain, &surface.pattern, |q| surface.point(q), |x| crate::edge::harmonic_reach(x, scene.microphone, scene.incident(x).source_distance))
}

#[allow(clippy::too_many_arguments)]
pub fn scatter_into(scene: &Scene, traversal: usize, index: usize, via: Option<Via>, strength: f64, relief_floor: f64, part: Part, histograms: &mut Histograms) -> Option<usize> {
    let surface = &scene.surfaces[index];
    let facing = match via {
        Some(v) if !v.receiver => {
            let mirror = &scene.surfaces[v.surface].polygon;
            match scene.source {
                None => dot(surface.polygon.normal, mirror.mirror_direction(scene.arrival)),
                Some(source) => surface.polygon.signed_distance(mirror.mirror_point(source)),
            }
        }
        _ => scene.source_side(&surface.polygon),
    };
    if facing.abs() < 1e-9 && !surface.is_bent() {
        return None;
    }
    let mirror = via.filter(|v| v.receiver).map(|v| &scene.surfaces[v.surface].polygon);
    let gradients = scene.gradients().into_iter().map(|(set, pattern, facing)| (set, pattern, mirror.map_or(facing, |m| m.mirror_direction(facing)))).collect();
    let lit_normal = if facing > 0.0 { surface.polygon.normal } else { scale(surface.polygon.normal, -1.0) };
    let rate = scene.sample_rate * OVERSAMPLING as f64;
    let reach = extension(scene, histograms.bins());
    let relief = &surface.relief;
    let relief_element = if relief.is_flat() { 0.0 } else { (relief.finest() / scene.quality.relief_detail).max(relief_floor) };
    let screens = if via.is_none() { screens_of(scene, index) } else { Vec::new() };
    let skip = |receiver: bool| std::iter::once(index).chain(screens.iter().filter(|s| s.1 == receiver).map(|s| s.0)).collect();
    let mut context = Context {
        source_skip: skip(true),
        microphone_skip: skip(false),
        scene,
        index,
        via,
        microphone: mirror.map_or(scene.microphone, |m| m.mirror_point(scene.microphone)),
        domain: domain(scene, index, reach, via.map(|v| v.surface)),
        lit_normal,
        relief_element,
        rate,
        scale: strength,
        histograms,
        kernel: Vec::new(),
        elements: 0,
        part,
        traversal,
        gradients,
    };
    let (lo, hi) = context.domain.bounds();
    let extent = (hi[0] - lo[0]).max(hi[1] - lo[1]);
    let limited = relief_element > 0.0 && relief_element == relief_floor;
    let size = if limited { relief_element * 2.0_f64.powi((extent / relief_element).log2().ceil().max(0.0) as i32) } else { extent };
    visit(&mut context, [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0], size, 0, 0);
    Some(context.elements)
}

fn domain(scene: &Scene, index: usize, extension: f64, via: Option<usize>) -> Polygon {
    let polygon = &scene.surfaces[index].polygon;
    let distances: Vec<f64> = (0..polygon.points.len()).map(|i| if joined(scene, index, i, via) { extension } else { 0.0 }).collect();
    polygon.offset(&distances)
}

const RIGHT_ANGLE_COSINE: f64 = 0.2;

const JOIN_PROBE_METRES: f64 = 5e-3;

pub fn extended_edges(scene: &Scene, index: usize) -> usize {
    let polygon = &scene.surfaces[index].polygon;
    (0..polygon.points.len()).filter(|&i| joined(scene, index, i, None)).count()
}

fn joined(scene: &Scene, index: usize, e: usize, only: Option<usize>) -> bool {
    let surface = &scene.surfaces[index];
    let this = &surface.polygon;
    let n = this.points.len();
    let (qa, qb) = (this.points[e], this.points[(e + 1) % n]);
    if surface.is_bent() && !surface.bend.follows_arc(qa, qb) {
        return false;
    }
    let (a, b) = surface.edge(e);
    let middle_plane = [(qa[0] + qb[0]) / 2.0, (qa[1] + qb[1]) / 2.0];
    let middle = surface.point(middle_plane);
    let (this_normal, _, _) = surface.frame(middle_plane);
    let edge = sub(b, a);
    let microphone_side = surface.locate(scene.microphone).1.signum();
    scene.surfaces.iter().enumerate().any(|(j, other)| {
        let p = &other.polygon;
        if j == index
            || other.is_bent()
            || only.is_some_and(|o| o != j)
            || dot(this_normal, p.normal).abs() >= RIGHT_ANGLE_COSINE
            || p.signed_distance(a).abs() >= JOIN_TOLERANCE_METRES
            || p.signed_distance(b).abs() >= JOIN_TOLERANCE_METRES
            || p.signed_distance(middle).abs() >= JOIN_TOLERANCE_METRES
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
            other.is_solid(q) && surface.locate(probe).1.signum() == microphone_side
        })
    })
}

fn visit(context: &mut Context, centre: [f64; 2], size: f64, level: u32, cell: usize) {
    if level == CELL_LEVEL && !context.owns(cell, level) {
        return;
    }
    let surface = &context.scene.surfaces[context.index];
    let h = size / 2.0;
    let samples = [centre, [centre[0] - h, centre[1] - h], [centre[0] + h, centre[1] - h], [centre[0] - h, centre[1] + h], [centre[0] + h, centre[1] + h]];
    let domain = &context.domain;
    let is_solid = |q: [f64; 2]| domain.contains(q) && surface.pattern.is_solid(q);
    let solid = samples.iter().filter(|&&q| is_solid(q)).count();
    let vertex_inside = domain.points.iter().any(|p| (p[0] - centre[0]).abs() <= h && (p[1] - centre[1]).abs() <= h);
    let x = surface.point(centre);
    let to_microphone = sub(context.microphone, x);
    let distance = norm(to_microphone);
    let c = context.scene.speed_of_sound;
    let incident = context.incident(x);
    let earliest = (incident.extra_metres + distance - size * std::f64::consts::SQRT_2 - 2.0 * surface.relief.bound()) / c;
    if earliest * context.rate > context.histograms.bins() as f64 {
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
    let surface = &scene.surfaces[context.index];
    let real = &surface.polygon;
    let virtual_element = !real.contains(centre);
    let relief = &surface.relief;
    let bent = surface.is_bent();
    let shaped = !relief.is_flat();
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
    let (normal, along_u, along_v) = surface.frame(centre);
    let x = crate::geom::add(surface.point(centre), scale(normal, height));
    let incident = context.incident(x);
    let side = if bent {
        let towards = dot(normal, incident.towards_source);
        if towards == 0.0 {
            return;
        }
        towards.signum()
    } else {
        dot(context.lit_normal, normal)
    };
    let tilted = scale(sub(normal, crate::geom::add(scale(along_u, slope[0]), scale(along_v, slope[1]))), side);
    let tangent_u = crate::geom::add(along_u, scale(normal, slope[0]));
    let tangent_v = crate::geom::add(along_v, scale(normal, slope[1]));
    if (shaped || bent) && dot(tilted, incident.towards_source) <= 0.0 {
        return;
    }
    let shades = |towards: V3| {
        let rise = dot(towards, normal);
        shaped && relief.shades(centre, height, [dot(towards, along_u), dot(towards, along_v)], rise, side.signum(), |q| real.contains(q))
    };
    if shades(incident.towards_source) {
        return;
    }
    let towards_source_clear = virtual_element
        || match context.via {
            Some(via) if !via.receiver => context.through_via(via, x, incident.towards_source, incident.source_distance),
            _ => !scene.occluded(&context.source_skip, x, incident.towards_source, incident.source_distance),
        };
    if !towards_source_clear {
        return;
    }
    let to_microphone = sub(context.microphone, x);
    let distance = norm(to_microphone).max(NEAREST_DISTANCE_METRES);
    let direction = scale(to_microphone, 1.0 / distance);
    let cos_scatter = dot(tilted, direction);
    let heard_via = context.via.filter(|v| v.receiver);
    let blocked = match heard_via {
        Some(via) => !context.through_via(via, x, direction, distance),
        None => !virtual_element && scene.occluded(&context.microphone_skip, x, direction, distance),
    };
    let skip = [context.index];
    let seen_through = |list: &[usize], from: V3, towards: V3, length: f64| list.len() > 1 && scene.occluded(&skip, from, towards, length);
    if heard_via.is_none()
        && context.via.is_none()
        && seen_through(&context.source_skip, x, incident.towards_source, incident.source_distance)
        && seen_through(&context.microphone_skip, x, direction, distance)
    {
        return;
    }
    let shadow = context.via.is_some_and(|v| v.shadow);
    if (cos_scatter > 0.0 || shadow) && blocked {
        return;
    }
    if shadow && cos_scatter >= 0.0 {
        return;
    }
    if cos_scatter > 0.0 && shades(direction) {
        return;
    }
    let c = scene.speed_of_sound;
    let delay = (incident.extra_metres + distance) / c * context.rate;
    let arriving = heard_via.map_or(direction, |via| scene.surfaces[via.surface].polygon.mirror_direction(direction));
    let (listener_delay, weights) = scene.split(scale(arriving, -1.0), distance);
    let listener_shift = listener_delay * context.rate;
    let length = context.histograms.bins();
    if delay >= (length - 2) as f64 {
        return;
    }
    let unshaded = if shaped { relief.rough_shadowing(dot(normal, incident.towards_source).abs()) * relief.rough_shadowing(dot(normal, direction).abs()) } else { 1.0 };
    let area = size * size * surface.bend.area_scale(centre) * incident.amplitude * unshaded;
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
    for &(set, pattern, facing) in &context.gradients {
        let toward_facing = -dot(direction, facing);
        let across = (dot(tilted, facing) + cos_scatter * toward_facing) * area / (2.0 * PI * distance * distance);
        let radial = toward_facing * rigid_near;
        values[set][RIGID_FAR] = (1.0 - pattern) * (radial + across);
        values[set][RIGID_NEAR] = (1.0 - pattern) * (2.0 * radial + across) / distance;
    }
    let towards = incident.towards_source;
    let gradient = scale([-towards[0] - direction[0], -towards[1] - direction[1], -towards[2] - direction[2]], size * context.rate / c);
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
