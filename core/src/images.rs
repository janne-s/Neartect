use crate::geom::{add, dot, norm, scale, sub, V3};
use crate::scene::Scene;

// Class C for a repeating shape: Eckart’s coherent factor, without its lobes.
pub struct Reflection {
    pub surface: usize,
    pub cutoff_hz: f64,
    pub rough_metres: f64,
}

pub struct Path {
    pub delay_seconds: f64,
    pub gain: f64,
    pub from: V3,
    pub distance: f64,
    pub reflections: Vec<Reflection>,
}

pub fn specular_paths(scene: &Scene) -> Vec<Path> {
    let mut paths = Vec::new();
    let mut sequence = Vec::new();
    let covered = covered_pairs(scene);
    let reach = Reach::new(scene);
    extend(scene, &covered, &reach, &mut sequence, scene.microphone, &mut paths);
    paths
}

const FRONT: u8 = 1;
const BACK: u8 = 2;
const SIDE_TOLERANCE_METRES: f64 = 1e-9;

fn side(distance: f64) -> u8 {
    if distance > SIDE_TOLERANCE_METRES {
        FRONT
    } else if distance < -SIDE_TOLERANCE_METRES {
        BACK
    } else {
        0
    }
}

struct Reach {
    count: usize,
    sides: Vec<u8>,
    microphone: Vec<u8>,
    corners: Vec<Vec<V3>>,
    centres: Vec<V3>,
    convex: Vec<bool>,
    bent: Vec<bool>,
}

impl Reach {
    fn new(scene: &Scene) -> Reach {
        let count = scene.surfaces.len();
        let corners: Vec<Vec<V3>> = scene
            .surfaces
            .iter()
            .map(|s| if s.is_bent() { s.bend.outline(&s.polygon, OUTLINE_PIECES) } else { s.polygon.points.iter().map(|&q| s.polygon.to_space(q)).collect() })
            .collect();
        let mut sides = vec![0; count * count];
        for (a, here) in scene.surfaces.iter().enumerate() {
            for (b, points) in corners.iter().enumerate() {
                sides[a * count + b] = if here.is_bent() { FRONT | BACK } else { points.iter().fold(0, |mask, &p| mask | side(here.polygon.signed_distance(p))) };
            }
        }
        let microphone = scene.surfaces.iter().map(|s| if s.is_bent() { FRONT | BACK } else { side(s.polygon.signed_distance(scene.microphone)) }).collect();
        let centres = corners.iter().map(|points| scale(points.iter().fold([0.0; 3], |sum, &p| add(sum, p)), 1.0 / points.len() as f64)).collect();
        let convex = scene.surfaces.iter().map(|s| !s.is_bent() && is_convex(&s.polygon.points)).collect();
        let bent = scene.surfaces.iter().map(|s| s.is_bent()).collect();
        Reach { count, sides, microphone, corners, centres, convex, bent }
    }

    fn can_follow(&self, sequence: &[usize], image: V3, next: usize) -> bool {
        let Some((&last, before)) = sequence.split_last() else { return true };
        let arriving = match before.last() {
            Some(&previous) => self.sides[last * self.count + previous],
            None => self.microphone[last],
        };
        let image_holds = !sequence.iter().any(|&i| self.bent[i]);
        arriving & self.sides[last * self.count + next] != 0 && (!self.convex[last] || !image_holds || self.in_beam(image, last, next))
    }

    fn in_beam(&self, apex: V3, window: usize, next: usize) -> bool {
        let edges = &self.corners[window];
        let inside = sub(self.centres[window], apex);
        (0..edges.len()).all(|e| {
            let normal = crate::geom::cross(sub(edges[e], apex), sub(edges[(e + 1) % edges.len()], apex));
            let orientation = dot(normal, inside);
            if orientation.abs() < BEAM_TOLERANCE * norm(normal) * norm(inside) {
                return true;
            }
            self.corners[next].iter().any(|&p| dot(normal, sub(p, apex)) * orientation.signum() > -BEAM_TOLERANCE * norm(normal) * norm(sub(p, apex)))
        })
    }
}

const BEAM_TOLERANCE: f64 = 1e-9;
const OUTLINE_PIECES: usize = 8;

fn is_convex(points: &[[f64; 2]]) -> bool {
    let n = points.len();
    let turns: Vec<f64> = (0..n)
        .map(|i| {
            let (a, b, c) = (points[i], points[(i + 1) % n], points[(i + 2) % n]);
            (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0])
        })
        .collect();
    turns.iter().all(|&t| t >= 0.0) || turns.iter().all(|&t| t <= 0.0)
}

fn covered_pairs(scene: &Scene) -> Vec<[usize; 2]> {
    crate::kirchhoff::via_pairs(scene)
        .into_iter()
        .flat_map(|(i, via)| {
            let j = via.surface;
            match (via.cutoff_hz, via.receiver) {
                (None, _) => vec![[i, j], [j, i]],
                (Some(_), false) => vec![[i, j]],
                (Some(_), true) => vec![[j, i]],
            }
        })
        .collect()
}

pub fn path_through(scene: &Scene, sequence: &[usize]) -> Option<Path> {
    trace(scene, sequence).into_iter().next()
}

fn extend(scene: &Scene, covered: &[[usize; 2]], reach: &Reach, sequence: &mut Vec<usize>, image: V3, paths: &mut Vec<Path>) {
    let held = sequence.len() == 2 && covered.contains(&[sequence[0], sequence[1]]);
    if sequence.len() >= 2 && !held {
        paths.extend(trace(scene, sequence));
    }
    if sequence.len() >= scene.max_order {
        return;
    }
    for i in 0..scene.surfaces.len() {
        let second_bend = scene.surfaces[i].is_bent() && sequence.iter().any(|&j| scene.surfaces[j].is_bent());
        if sequence.last() == Some(&i) || scene.surfaces[i].strength <= 0.0 || second_bend || !reach.can_follow(sequence, image, i) {
            continue;
        }
        sequence.push(i);
        extend(scene, covered, reach, sequence, scene.surfaces[i].polygon.mirror_point(image), paths);
        sequence.pop();
    }
}

#[derive(Clone, Copy)]
struct Mirror {
    point: V3,
    normal: V3,
}

impl Mirror {
    fn of(polygon: &crate::geom::Polygon) -> Mirror {
        Mirror { point: polygon.origin, normal: polygon.normal }
    }

    fn distance(&self, x: V3) -> f64 {
        dot(sub(x, self.point), self.normal)
    }

    fn mirror_point(&self, x: V3) -> V3 {
        sub(x, scale(self.normal, 2.0 * self.distance(x)))
    }

    fn mirror_direction(&self, d: V3) -> V3 {
        sub(d, scale(self.normal, 2.0 * dot(d, self.normal)))
    }
}

#[derive(Clone, Copy)]
struct Bounce {
    at: usize,
    point: V3,
    gain: f64,
}

const BOUNCE_TOLERANCE_METRES: f64 = 1e-6;

fn trace(scene: &Scene, sequence: &[usize]) -> Vec<Path> {
    let mirrors: Vec<Mirror> = sequence.iter().map(|&i| Mirror::of(&scene.surfaces[i].polygon)).collect();
    let bent: Vec<usize> = (0..sequence.len()).filter(|&k| scene.surfaces[sequence[k]].is_bent()).collect();
    match bent[..] {
        [] => trace_with(scene, sequence, &mirrors, None).into_iter().collect(),
        [k] => {
            let surface = &scene.surfaces[sequence[k]];
            let to = mirrors[..k].iter().fold(scene.microphone, |x, m| m.mirror_point(x));
            let later = &mirrors[k + 1..];
            let source = scene.source.map(|s| later.iter().rev().fold(s, |x, m| m.mirror_point(x)));
            let towards = later.iter().rev().fold(scene.arrival, |d, m| m.mirror_direction(d));
            let range = surface.bend.arc_range(&surface.polygon);
            surface
                .bend
                .specular(&surface.polygon, range, to, source, towards)
                .into_iter()
                .filter(|found| surface.is_solid(found.plane))
                .filter_map(|found| {
                    let mut own = mirrors.clone();
                    own[k] = Mirror { point: found.point, normal: found.normal };
                    trace_with(scene, sequence, &own, Some(Bounce { at: k, point: found.point, gain: found.gain }))
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

fn hit_on(surface: &crate::scene::Surface, mirror: Mirror, bounce: Option<Bounce>, point: V3, direction: V3, remaining: f64) -> Option<(f64, V3)> {
    match bounce {
        Some(b) => {
            let along = dot(direction, mirror.normal);
            if along.abs() < 1e-12 {
                return None;
            }
            let t = -mirror.distance(point) / along;
            let hit = add(point, scale(direction, t));
            (t > 1e-7 && t < remaining - 1e-7 && norm(sub(hit, b.point)) < BOUNCE_TOLERANCE_METRES * (1.0 + t)).then_some((t, hit))
        }
        None => {
            let (t, q) = surface.ray_hit(point, direction, remaining)?;
            surface.pattern.is_solid(q).then(|| (t, add(point, scale(direction, t))))
        }
    }
}

fn trace_with(scene: &Scene, sequence: &[usize], mirrors: &[Mirror], bounce: Option<Bounce>) -> Option<Path> {
    let path = match scene.source {
        None => trace_plane(scene, sequence, mirrors, bounce),
        Some(source) => trace_point(scene, sequence, mirrors, bounce, source),
    }?;
    Some(Path { gain: path.gain * bounce.map_or(1.0, |b| b.gain), ..path })
}

fn trace_point(scene: &Scene, sequence: &[usize], mirrors: &[Mirror], bounce: Option<Bounce>, source: V3) -> Option<Path> {
    let mut image = source;
    for m in mirrors.iter().rev() {
        image = m.mirror_point(image);
    }
    let path_length = norm(sub(image, scene.microphone));
    let from = crate::geom::normalize(sub(image, scene.microphone));
    let mut point = scene.microphone;
    let mut hits: Vec<(V3, f64, f64)> = Vec::with_capacity(sequence.len());
    let mut travelled = 0.0;
    for (k, &i) in sequence.iter().enumerate() {
        let mirror = mirrors[k];
        let remaining = norm(sub(image, point));
        let direction = scale(sub(image, point), 1.0 / remaining);
        if mirror.distance(point).signum() == mirror.distance(image).signum() {
            return None;
        }
        let (t, hit) = hit_on(&scene.surfaces[i], mirror, bounce.filter(|b| b.at == k), point, direction, remaining)?;
        let previous: Vec<usize> = if k > 0 { vec![sequence[k - 1], i] } else { vec![i] };
        if scene.occluded(&previous, point, direction, t) {
            return None;
        }
        travelled += t;
        hits.push((hit, travelled, dot(direction, mirror.normal).abs()));
        image = mirror.mirror_point(image);
        point = hit;
    }
    let to_source = sub(source, point);
    let last = norm(to_source);
    if scene.occluded(&[*sequence.last().unwrap()], point, scale(to_source, 1.0 / last), last) {
        return None;
    }
    let direct = norm(sub(source, scene.microphone));
    let c = scene.speed_of_sound;
    Some(Path { delay_seconds: (path_length - direct) / c, gain: direct / path_length, from, distance: path_length, reflections: rindel(scene, sequence, &hits, Some(source)) })
}

fn trace_plane(scene: &Scene, sequence: &[usize], mirrors: &[Mirror], bounce: Option<Bounce>) -> Option<Path> {
    let mut direction = scene.arrival;
    for m in mirrors.iter().rev() {
        direction = m.mirror_direction(direction);
    }
    let from = direction;
    let mut point = scene.microphone;
    let mut hits: Vec<(V3, f64, f64)> = Vec::with_capacity(sequence.len());
    let mut travelled = 0.0;
    for (k, &i) in sequence.iter().enumerate() {
        let mirror = mirrors[k];
        let (t, hit) = hit_on(&scene.surfaces[i], mirror, bounce.filter(|b| b.at == k), point, direction, f64::INFINITY)?;
        let previous: Vec<usize> = if k > 0 { vec![sequence[k - 1], i] } else { vec![i] };
        if scene.occluded(&previous, point, direction, t) {
            return None;
        }
        travelled += t;
        hits.push((hit, travelled, dot(direction, mirror.normal).abs()));
        direction = mirror.mirror_direction(direction);
        point = hit;
    }
    if norm(sub(direction, scene.arrival)) > 1e-6 {
        return None;
    }
    if scene.occluded(&[*sequence.last().unwrap()], point, direction, f64::INFINITY) {
        return None;
    }
    let c = scene.speed_of_sound;
    let delay_seconds = (dot(scene.arrival, sub(scene.microphone, point)) + travelled) / c;
    Some(Path { delay_seconds, gain: 1.0, from, distance: f64::INFINITY, reflections: rindel(scene, sequence, &hits, None) })
}

pub fn design_frequency(speed_of_sound: f64, characteristic_metres: f64, area: f64, cos_incidence: f64) -> f64 {
    speed_of_sound * characteristic_metres / (2.0 * area * cos_incidence.max(1e-3))
}

fn rindel(scene: &Scene, sequence: &[usize], hits: &[(V3, f64, f64)], source: Option<V3>) -> Vec<Reflection> {
    let c = scene.speed_of_sound;
    sequence
        .iter()
        .enumerate()
        .map(|(k, &i)| {
            let (hit, to_microphone, cos_incidence) = hits[k];
            let to_source = if k + 1 < hits.len() { norm(sub(hits[k + 1].0, hit)) } else { source.map_or(f64::INFINITY, |s| norm(sub(s, hit))) };
            let characteristic = if to_source.is_infinite() { 2.0 * to_microphone } else { 2.0 * to_source * to_microphone / (to_source + to_microphone) };
            let area = scene.surfaces[i].pattern.solid_area(scene.surfaces[i].polygon.area);
            let cutoff_hz = design_frequency(c, characteristic, area, cos_incidence);
            let rough_metres = scene.surfaces[i].relief.rms() * cos_incidence;
            Reflection { surface: i, cutoff_hz, rough_metres }
        })
        .collect()
}
