// Specular paths of order two and above for a plane-wave arrival or a point
// source (its images; Allen and Berkley 1979), with validity and visibility
// checks (Borish 1984). Each reflection takes a
// finite-size high-pass at Rindel's design frequency, class B pending the
// original text.

use crate::geom::{add, dot, norm, scale, sub, V3};
use crate::scene::Scene;

pub struct Reflection {
    pub surface: usize,
    pub cutoff_hz: f64,
}

pub struct Path {
    pub delay_seconds: f64,
    pub gain: f64,
    pub reflections: Vec<Reflection>,
}

pub fn specular_paths(scene: &Scene) -> Vec<Path> {
    let mut paths = Vec::new();
    let mut sequence = Vec::new();
    extend(scene, &mut sequence, &mut paths);
    paths
}

fn extend(scene: &Scene, sequence: &mut Vec<usize>, paths: &mut Vec<Path>) {
    if sequence.len() >= 2 {
        if let Some(path) = trace(scene, sequence) {
            paths.push(path);
        }
    }
    if sequence.len() >= scene.max_order {
        return;
    }
    for i in 0..scene.surfaces.len() {
        if sequence.last() == Some(&i) || scene.surfaces[i].strength <= 0.0 {
            continue;
        }
        sequence.push(i);
        extend(scene, sequence, paths);
        sequence.pop();
    }
}

fn trace(scene: &Scene, sequence: &[usize]) -> Option<Path> {
    match scene.source {
        None => trace_plane(scene, sequence),
        Some(source) => trace_point(scene, sequence, source),
    }
}

fn trace_point(scene: &Scene, sequence: &[usize], source: V3) -> Option<Path> {
    let mut image = source;
    for &i in sequence.iter().rev() {
        image = scene.surfaces[i].polygon.mirror_point(image);
    }
    let path_length = norm(sub(image, scene.microphone));
    let mut point = scene.microphone;
    let mut hits: Vec<(V3, f64, f64)> = Vec::with_capacity(sequence.len());
    let mut travelled = 0.0;
    for (k, &i) in sequence.iter().enumerate() {
        let surface = &scene.surfaces[i];
        let remaining = norm(sub(image, point));
        let direction = scale(sub(image, point), 1.0 / remaining);
        if surface.polygon.signed_distance(point).signum() == surface.polygon.signed_distance(image).signum() {
            return None;
        }
        let t = surface.polygon.ray_hit(point, direction, remaining)?;
        let hit = add(point, scale(direction, t));
        if !surface.pattern.is_solid(surface.polygon.to_plane(hit)) {
            return None;
        }
        let previous: Vec<usize> = if k > 0 { vec![sequence[k - 1], i] } else { vec![i] };
        if scene.occluded(&previous, point, direction, t) {
            return None;
        }
        travelled += t;
        hits.push((hit, travelled, dot(direction, surface.polygon.normal).abs()));
        image = surface.polygon.mirror_point(image);
        point = hit;
    }
    let to_source = sub(source, point);
    let last = norm(to_source);
    if scene.occluded(&[*sequence.last().unwrap()], point, scale(to_source, 1.0 / last), last) {
        return None;
    }
    let direct = norm(sub(source, scene.microphone));
    let c = scene.speed_of_sound;
    Some(Path {
        delay_seconds: (path_length - direct) / c,
        gain: direct / path_length,
        reflections: rindel(scene, sequence, &hits, Some(source)),
    })
}

fn trace_plane(scene: &Scene, sequence: &[usize]) -> Option<Path> {
    let mut direction = scene.arrival;
    for &i in sequence.iter().rev() {
        direction = scene.surfaces[i].polygon.mirror_direction(direction);
    }
    let mut point = scene.microphone;
    let mut hits: Vec<(V3, f64, f64)> = Vec::with_capacity(sequence.len());
    let mut travelled = 0.0;
    for (k, &i) in sequence.iter().enumerate() {
        let surface = &scene.surfaces[i];
        let t = surface.polygon.ray_hit(point, direction, f64::INFINITY)?;
        let hit = add(point, scale(direction, t));
        if !surface.pattern.is_solid(surface.polygon.to_plane(hit)) {
            return None;
        }
        let previous: Vec<usize> = if k > 0 { vec![sequence[k - 1], i] } else { vec![i] };
        if scene.occluded(&previous, point, direction, t) {
            return None;
        }
        travelled += t;
        let cos_incidence = dot(direction, surface.polygon.normal).abs();
        hits.push((hit, travelled, cos_incidence));
        direction = surface.polygon.mirror_direction(direction);
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
    Some(Path { delay_seconds, gain: 1.0, reflections: rindel(scene, sequence, &hits, None) })
}

fn rindel(scene: &Scene, sequence: &[usize], hits: &[(V3, f64, f64)], source: Option<V3>) -> Vec<Reflection> {
    let c = scene.speed_of_sound;
    sequence
        .iter()
        .enumerate()
        .map(|(k, &i)| {
            let (hit, to_microphone, cos_incidence) = hits[k];
            let to_source = if k + 1 < hits.len() {
                norm(sub(hits[k + 1].0, hit))
            } else {
                source.map_or(f64::INFINITY, |s| norm(sub(s, hit)))
            };
            let characteristic = if to_source.is_infinite() {
                2.0 * to_microphone
            } else {
                2.0 * to_source * to_microphone / (to_source + to_microphone)
            };
            let area = scene.surfaces[i].pattern.solid_area(scene.surfaces[i].polygon.area);
            let cutoff_hz = c * characteristic / (2.0 * area * cos_incidence.max(1e-3).powi(2));
            Reflection { surface: i, cutoff_hz }
        })
        .collect()
}
