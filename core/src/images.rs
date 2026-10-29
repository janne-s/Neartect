// Specular paths of order two and above for a plane-wave arrival or a point
// source (its images; Allen and Berkley 1979), with validity and visibility
// checks (Borish 1984). Each reflection takes a first-order high-pass at the
// finite-size design frequency f = c a* / (2 S cos θ), a* = 2 a1 a2 / (a1 + a2)
// (Rindel 1986, in the area form of Archives of Acoustics 39(4), 2014): its
// asymptote is the Kirchhoff sum of a panel small against the wavelength
// (class A there); the transition is class B.

use crate::geom::{add, dot, norm, scale, sub, V3};
use crate::scene::Scene;

// Each reflection keeps its relief's rms height times the cosine of incidence:
// the coherent part falls as exp(−2 (kσ cos θ)²) (Eckart; class C for a
// repeating shape, whose lobes the images do not follow).
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
    extend(scene, &covered, &mut sequence, &mut paths);
    paths
}

// Second-order sequences, nearest the microphone first, that the Kirchhoff sum
// via a joined flat surface already holds (both orders).
fn covered_pairs(scene: &Scene) -> Vec<[usize; 2]> {
    crate::kirchhoff::via_pairs(scene).into_iter().flat_map(|(i, j)| [[i, j], [j, i]]).collect()
}

fn extend(scene: &Scene, covered: &[[usize; 2]], sequence: &mut Vec<usize>, paths: &mut Vec<Path>) {
    let held = sequence.len() == 2 && covered.contains(&[sequence[0], sequence[1]]);
    if sequence.len() >= 2 && !held {
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
        extend(scene, covered, sequence, paths);
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
    let from = crate::geom::normalize(sub(image, scene.microphone));
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
        from,
        distance: path_length,
        reflections: rindel(scene, sequence, &hits, Some(source)),
    })
}

fn trace_plane(scene: &Scene, sequence: &[usize]) -> Option<Path> {
    let mut direction = scene.arrival;
    for &i in sequence.iter().rev() {
        direction = scene.surfaces[i].polygon.mirror_direction(direction);
    }
    let from = direction;
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
            let cutoff_hz = design_frequency(c, characteristic, area, cos_incidence);
            let rough_metres = scene.surfaces[i].relief.rms() * cos_incidence;
            Reflection { surface: i, cutoff_hz, rough_metres }
        })
        .collect()
}
