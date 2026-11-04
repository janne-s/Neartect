// Plane coordinates along `along` are arc length; positive curvature bends towards the polygon’s normal.

use crate::geom::{add, cross, dot, norm, scale, sub, Polygon, V3};
use std::f64::consts::PI;

const FLAT_CURVATURE: f64 = 1e-9;
const SPECULAR_STEPS: usize = 96;
const SPECULAR_ITERATIONS: usize = 60;
// Class C limit at a focus.
pub const MOST_FOCUS_GAIN: f64 = 4.0;

#[derive(Clone, Copy, Debug)]
pub struct Bend {
    pub curvature: f64,
    pub along: [f64; 2],
    pub start: f64,
    pub sphere: bool,
    pub touch: [f64; 2],
}

pub struct Specular {
    pub point: V3,
    pub normal: V3,
    pub plane: [f64; 2],
    pub gain: f64,
}

impl Bend {
    pub fn flat() -> Bend {
        Bend { curvature: 0.0, along: [1.0, 0.0], start: 0.0, sphere: false, touch: [0.0, 0.0] }
    }

    pub fn new(polygon: &Polygon, curvature: f64, direction: V3, touching: V3, sphere: bool) -> Bend {
        if sphere {
            if curvature.is_nan() || curvature.abs() <= FLAT_CURVATURE {
                return Bend::flat();
            }
            return Bend { curvature, along: [1.0, 0.0], start: 0.0, sphere, touch: polygon.to_plane(touching) };
        }
        let along = [dot(direction, polygon.u), dot(direction, polygon.v)];
        let length = (along[0] * along[0] + along[1] * along[1]).sqrt();
        if curvature.is_nan() || curvature.abs() <= FLAT_CURVATURE || length < 1e-9 {
            return Bend::flat();
        }
        let along = [along[0] / length, along[1] / length];
        let q = polygon.to_plane(touching);
        Bend { curvature, along, start: q[0] * along[0] + q[1] * along[1], sphere, touch: q }
    }

    pub fn is_bent(&self) -> bool {
        self.curvature.abs() > FLAT_CURVATURE
    }

    fn across(&self) -> [f64; 2] {
        [-self.along[1], self.along[0]]
    }

    fn arc(&self, q: [f64; 2]) -> f64 {
        q[0] * self.along[0] + q[1] * self.along[1] - self.start
    }

    pub fn along_space(&self, polygon: &Polygon) -> V3 {
        add(scale(polygon.u, self.along[0]), scale(polygon.v, self.along[1]))
    }

    pub fn axis_space(&self, polygon: &Polygon) -> V3 {
        let a = self.across();
        add(scale(polygon.u, a[0]), scale(polygon.v, a[1]))
    }

    fn touching(&self, polygon: &Polygon) -> V3 {
        if self.sphere {
            return polygon.to_space(self.touch);
        }
        polygon.to_space([self.along[0] * self.start, self.along[1] * self.start])
    }

    fn flatten(&self, polygon: &Polygon, w: V3) -> V3 {
        if self.sphere {
            return w;
        }
        let axis = self.axis_space(polygon);
        sub(w, scale(axis, dot(w, axis)))
    }

    fn outward(&self, polygon: &Polygon, q: [f64; 2]) -> (V3, f64) {
        if !self.sphere {
            return (self.along_space(polygon), self.arc(q));
        }
        let d = [q[0] - self.touch[0], q[1] - self.touch[1]];
        let rho = d[0].hypot(d[1]);
        if rho < 1e-12 {
            return (polygon.u, 0.0);
        }
        (add(scale(polygon.u, d[0] / rho), scale(polygon.v, d[1] / rho)), rho)
    }

    pub fn area_scale(&self, q: [f64; 2]) -> f64 {
        if !self.sphere || !self.is_bent() {
            return 1.0;
        }
        let x = self.curvature.abs() * (q[0] - self.touch[0]).hypot(q[1] - self.touch[1]);
        if x < 1e-9 {
            1.0
        } else {
            x.sin() / x
        }
    }

    pub fn point(&self, polygon: &Polygon, q: [f64; 2]) -> V3 {
        let flat = polygon.to_space(q);
        if !self.is_bent() {
            return flat;
        }
        let (e, s) = self.outward(polygon, q);
        let k = self.curvature;
        let (sin, cos) = (k * s).sin_cos();
        add(flat, add(scale(e, sin / k - s), scale(polygon.normal, (1.0 - cos) / k)))
    }

    pub fn rotate(&self, polygon: &Polygon, q: [f64; 2], w: V3) -> V3 {
        if !self.is_bent() {
            return w;
        }
        let (e, s) = self.outward(polygon, q);
        let (sin, cos) = (self.curvature * s).sin_cos();
        let n = polygon.normal;
        let (we, wn) = (dot(w, e), dot(w, n));
        add(w, add(scale(add(scale(e, cos - 1.0), scale(n, sin)), we), scale(add(scale(n, cos - 1.0), scale(e, -sin)), wn)))
    }

    fn centre(&self, polygon: &Polygon) -> V3 {
        add(self.touching(polygon), scale(polygon.normal, 1.0 / self.curvature))
    }

    fn radial(&self, polygon: &Polygon, p: V3) -> V3 {
        self.flatten(polygon, sub(p, self.centre(polygon)))
    }

    fn plane_of(&self, polygon: &Polygon, p: V3, radial: V3) -> [f64; 2] {
        let sign = self.curvature.signum();
        let r0 = scale(polygon.normal, -sign);
        if self.sphere {
            let height = dot(radial, r0);
            let side = sub(radial, scale(r0, height));
            let (du, dv) = (dot(side, polygon.u), dot(side, polygon.v));
            let width = du.hypot(dv);
            if width < 1e-15 {
                return self.touch;
            }
            let rho = width.atan2(height) / self.curvature.abs();
            return [self.touch[0] + rho * du / width, self.touch[1] + rho * dv / width];
        }
        let r1 = scale(self.along_space(polygon), sign);
        let s = dot(radial, r1).atan2(dot(radial, r0)) / self.curvature + self.start;
        let a = self.across();
        let t = dot(sub(p, polygon.origin), self.axis_space(polygon));
        [self.along[0] * s + a[0] * t, self.along[1] * s + a[1] * t]
    }

    pub fn locate(&self, polygon: &Polygon, p: V3) -> ([f64; 2], f64) {
        if !self.is_bent() {
            return (polygon.to_plane(p), polygon.signed_distance(p));
        }
        let radial = self.radial(polygon, p);
        let gap = self.curvature.signum() * (1.0 / self.curvature.abs() - norm(radial));
        (self.plane_of(polygon, p, radial), gap)
    }

    pub fn crossings(&self, polygon: &Polygon, from: V3, direction: V3, max_length: f64) -> Vec<(f64, [f64; 2])> {
        let f = self.radial(polygon, from);
        let d = self.flatten(polygon, direction);
        let a = dot(d, d);
        if a < 1e-18 {
            return Vec::new();
        }
        let radius = 1.0 / self.curvature.abs();
        let b = dot(f, d);
        let c = dot(f, f) - radius * radius;
        let disc = b * b - a * c;
        if disc < 0.0 {
            return Vec::new();
        }
        let root = disc.sqrt();
        [(-b - root) / a, (-b + root) / a]
            .into_iter()
            .filter(|&t| t > 1e-7 && t < max_length - 1e-7)
            .map(|t| {
                let p = add(from, scale(direction, t));
                (t, self.plane_of(polygon, p, self.radial(polygon, p)))
            })
            .collect()
    }

    pub fn specular(&self, polygon: &Polygon, range: (f64, f64), to: V3, source: Option<V3>, towards_source: V3) -> Vec<Specular> {
        if self.sphere {
            return self.sphere_specular(polygon, to, source, towards_source);
        }
        let k = self.curvature;
        let e = self.along_space(polygon);
        let n = polygon.normal;
        let axis = self.axis_space(polygon);
        let origin = self.touching(polygon);
        let flat = |p: V3| [dot(sub(p, origin), e), dot(sub(p, origin), n)];
        let b = flat(to);
        let a = source.map(flat);
        let incoming = [dot(towards_source, e), dot(towards_source, n)];
        let incoming_length = (incoming[0] * incoming[0] + incoming[1] * incoming[1]).sqrt();
        if a.is_none() && incoming_length < 1e-9 {
            return Vec::new();
        }
        let on_arc = |theta: f64| [theta.sin() / k, (1.0 - theta.cos()) / k];
        let unit = |v: [f64; 2]| {
            let l = (v[0] * v[0] + v[1] * v[1]).sqrt().max(1e-30);
            ([v[0] / l, v[1] / l], l)
        };
        let legs = |theta: f64| {
            let p = on_arc(theta);
            let (to_b, rb) = unit([b[0] - p[0], b[1] - p[1]]);
            let (to_a, ra) = match a {
                Some(a) => unit([a[0] - p[0], a[1] - p[1]]),
                None => ([incoming[0] / incoming_length, incoming[1] / incoming_length], f64::INFINITY),
            };
            (p, to_a, ra, to_b, rb)
        };
        let condition = |theta: f64| {
            let (_, to_a, _, to_b, _) = legs(theta);
            let tangent = [theta.cos(), theta.sin()];
            let normal = [-theta.sin(), theta.cos()];
            let lit = (to_a[0] * normal[0] + to_a[1] * normal[1]) * (to_b[0] * normal[0] + to_b[1] * normal[1]) > 0.0;
            (to_a[0] * tangent[0] + to_a[1] * tangent[1] + to_b[0] * tangent[0] + to_b[1] * tangent[1], lit)
        };
        let (lo, hi) = (k * range.0.min(range.1), k * range.0.max(range.1));
        let (lo, hi) = (lo.min(hi), lo.max(hi));
        let step = (hi - lo) / SPECULAR_STEPS as f64;
        let mut found = Vec::new();
        let mut previous = condition(lo);
        for i in 1..=SPECULAR_STEPS {
            let theta = lo + step * i as f64;
            let here = condition(theta);
            let below = |f: f64| f < 0.0;
            if previous.1 && here.1 && below(previous.0) != below(here.0) {
                let (mut x0, mut x1) = (theta - step, theta);
                let start_below = below(previous.0);
                for _ in 0..SPECULAR_ITERATIONS {
                    let middle = 0.5 * (x0 + x1);
                    if below(condition(middle).0) == start_below {
                        x0 = middle;
                    } else {
                        x1 = middle;
                    }
                }
                found.push(0.5 * (x0 + x1));
            }
            previous = here;
        }
        found.dedup_by(|x, y| (*x - *y).abs() < step * 0.5);
        let t_of = |p: V3| dot(sub(p, polygon.origin), axis);
        found
            .into_iter()
            .map(|theta| {
                let (_, _, ra, to_b, rb) = legs(theta);
                let normal2 = [-theta.sin(), theta.cos()];
                let facing = to_b[0] * normal2[0] + to_b[1] * normal2[1];
                let t = match source {
                    Some(s) => t_of(s) + (t_of(to) - t_of(s)) * ra / (ra + rb),
                    None => t_of(to) + rb * dot(towards_source, axis) / incoming_length,
                };
                let s = theta / k + self.start;
                let across = self.across();
                let plane = [self.along[0] * s + across[0] * t, self.along[1] * s + across[1] * t];
                let point = self.point(polygon, plane);
                let normal = add(scale(n, theta.cos()), scale(e, -theta.sin()));
                let concave = k * facing.signum();
                let ratio = if ra.is_finite() { rb / ra } else { 0.0 };
                let spread = (1.0 + ratio - 2.0 * concave * rb / facing.abs().max(1e-6)).abs().max(1e-12);
                let gain = ((1.0 + ratio) / spread).sqrt().min(MOST_FOCUS_GAIN);
                Specular { point, normal, plane, gain }
            })
            .collect()
    }

    fn sphere_specular(&self, polygon: &Polygon, to: V3, source: Option<V3>, towards_source: V3) -> Vec<Specular> {
        let radius = 1.0 / self.curvature.abs();
        let centre = self.centre(polygon);
        let to_centre = sub(to, centre);
        let other = source.map_or(towards_source, |s| sub(s, centre));
        let unit = |v: V3| {
            let l = norm(v);
            (l > 1e-12).then(|| scale(v, 1.0 / l))
        };
        let Some(normal_of_plane) = unit(cross(to_centre, other)).or_else(|| unit(cross(to_centre, polygon.normal))).or_else(|| unit(cross(polygon.normal, polygon.u))) else {
            return Vec::new();
        };
        let towards_touching = scale(polygon.normal, -self.curvature.signum());
        let Some(a) = unit(sub(towards_touching, scale(normal_of_plane, dot(towards_touching, normal_of_plane)))) else {
            return Vec::new();
        };
        let b = cross(normal_of_plane, a);
        let reach = polygon.points.iter().map(|q| (q[0] - self.touch[0]).hypot(q[1] - self.touch[1])).fold(0.0, f64::max);
        let widest = (reach / radius * 1.05 + 0.01).min(PI);
        let at = |phi: f64| {
            let (sin, cos) = phi.sin_cos();
            let out = add(scale(a, cos), scale(b, sin));
            (add(centre, scale(out, radius)), out, add(scale(a, -sin), scale(b, cos)))
        };
        let legs = |p: V3| {
            let (to_b, rb) = (sub(to, p), norm(sub(to, p)));
            let to_b = scale(to_b, 1.0 / rb.max(1e-30));
            let (to_a, ra) = match source {
                Some(s) => {
                    let ra = norm(sub(s, p));
                    (scale(sub(s, p), 1.0 / ra.max(1e-30)), ra)
                }
                None => (towards_source, f64::INFINITY),
            };
            (to_a, ra, to_b, rb)
        };
        let condition = |phi: f64| {
            let (p, out, tangent) = at(phi);
            let (to_a, _, to_b, _) = legs(p);
            (dot(tangent, add(to_a, to_b)), dot(to_a, out) * dot(to_b, out) > 0.0)
        };
        let step = 2.0 * widest / SPECULAR_STEPS as f64;
        let mut found: Vec<f64> = Vec::new();
        let mut previous = condition(-widest);
        for i in 1..=SPECULAR_STEPS {
            let phi = -widest + step * i as f64;
            let here = condition(phi);
            if previous.1 && here.1 && (previous.0 < 0.0) != (here.0 < 0.0) {
                let (mut x0, mut x1) = (phi - step, phi);
                let start_below = previous.0 < 0.0;
                for _ in 0..SPECULAR_ITERATIONS {
                    let middle = 0.5 * (x0 + x1);
                    if (condition(middle).0 < 0.0) == start_below {
                        x0 = middle;
                    } else {
                        x1 = middle;
                    }
                }
                found.push(0.5 * (x0 + x1));
            }
            previous = here;
        }
        found.dedup_by(|x, y| (*x - *y).abs() < step * 0.5);
        found
            .into_iter()
            .map(|phi| {
                let (point, out, _) = at(phi);
                let (_, ra, to_b, rb) = legs(point);
                let cosine = dot(to_b, out).abs().max(1e-6);
                let concave = -dot(to_b, out).signum() / radius;
                let ratio = if ra.is_finite() { rb / ra } else { 0.0 };
                let tangential = (1.0 + ratio - 2.0 * concave * rb / cosine).abs().max(1e-12);
                let sagittal = (1.0 + ratio - 2.0 * concave * rb * cosine).abs().max(1e-12);
                let gain = ((1.0 + ratio) / (tangential * sagittal).sqrt()).min(MOST_FOCUS_GAIN);
                let plane = self.plane_of(polygon, point, sub(point, centre));
                Specular { point, normal: self.rotate(polygon, plane, polygon.normal), plane, gain }
            })
            .collect()
    }

    pub fn outline(&self, polygon: &Polygon, pieces: usize) -> Vec<V3> {
        let n = polygon.points.len();
        let mut points = Vec::new();
        for i in 0..n {
            let a = polygon.points[i];
            let b = polygon.points[(i + 1) % n];
            for j in 0..pieces {
                let f = j as f64 / pieces as f64;
                points.push(self.point(polygon, [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f]));
            }
        }
        points
    }

    pub fn arc_range(&self, polygon: &Polygon) -> (f64, f64) {
        polygon.points.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &q| (lo.min(self.arc(q)), hi.max(self.arc(q))))
    }

    pub fn follows_arc(&self, a: [f64; 2], b: [f64; 2]) -> bool {
        let d = [b[0] - a[0], b[1] - a[1]];
        let l = (d[0] * d[0] + d[1] * d[1]).sqrt();
        !self.sphere && l > 0.0 && ((d[0] * self.along[0] + d[1] * self.along[1]) / l).abs() > 1.0 - 1e-6
    }

    pub fn axis_normal_cosine(&self, polygon: &Polygon, normal: V3) -> f64 {
        dot(normal, self.axis_space(polygon)).abs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall(curvature: f64) -> (Polygon, Bend) {
        let polygon = Polygon::new(&[[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0], [-1.0, 0.0, 1.0]]).unwrap();
        let normal = polygon.normal;
        let bend = Bend::new(&polygon, curvature, [1.0, 0.0, 0.0], [0.0, 0.0, 0.5], false);
        assert!(normal[1].abs() > 0.99);
        (polygon, bend)
    }

    #[test]
    fn a_bent_point_lies_on_the_cylinder_and_locates_back() {
        let (polygon, bend) = wall(0.8);
        let q = polygon.to_plane([0.7, 0.0, 0.3]);
        let p = bend.point(&polygon, q);
        let (back, gap) = bend.locate(&polygon, p);
        assert!(gap.abs() < 1e-12 && (back[0] - q[0]).abs() < 1e-12 && (back[1] - q[1]).abs() < 1e-12);
        let normal = bend.rotate(&polygon, q, polygon.normal);
        let (_, off) = bend.locate(&polygon, add(p, scale(normal, 0.05)));
        assert!((off - 0.05).abs() < 1e-12, "gap {off}");
    }

    #[test]
    fn a_ray_crosses_the_cylinder_where_the_surface_is() {
        let (polygon, bend) = wall(-0.6);
        let q = polygon.to_plane([0.4, 0.0, 0.6]);
        let p = bend.point(&polygon, q);
        let from = add(p, [0.1, 2.0, 0.2]);
        let to = sub(p, from);
        let length = norm(to);
        let hits = bend.crossings(&polygon, from, scale(to, 1.0 / length), 10.0);
        assert!(hits.iter().any(|&(t, h)| (t - length).abs() < 1e-9 && (h[0] - q[0]).abs() < 1e-9 && (h[1] - q[1]).abs() < 1e-9));
    }

    #[test]
    fn a_dome_point_lies_on_the_sphere_and_locates_back() {
        let polygon = Polygon::new(&[[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0]]).unwrap();
        let bend = Bend::new(&polygon, -0.7, [1.0, 0.0, 0.0], [0.0, 0.0, 0.0], true);
        let q = polygon.to_plane([0.6, -0.4, 0.0]);
        let p = bend.point(&polygon, q);
        let centre = scale(polygon.normal, -1.0 / 0.7);
        assert!((norm(sub(p, centre)) - 1.0 / 0.7).abs() < 1e-12);
        let (back, gap) = bend.locate(&polygon, p);
        assert!(gap.abs() < 1e-12 && (back[0] - q[0]).abs() < 1e-12 && (back[1] - q[1]).abs() < 1e-12);
        let normal = bend.rotate(&polygon, q, polygon.normal);
        let (_, off) = bend.locate(&polygon, add(p, scale(normal, 0.05)));
        assert!((off - 0.05).abs() < 1e-12, "gap {off}");
        let from = add(p, [0.3, -0.2, 2.0]);
        let to = sub(p, from);
        let length = norm(to);
        let hits = bend.crossings(&polygon, from, scale(to, 1.0 / length), 10.0);
        assert!(hits.iter().any(|&(t, h)| (t - length).abs() < 1e-9 && (h[0] - q[0]).abs() < 1e-9 && (h[1] - q[1]).abs() < 1e-9));
    }

    #[test]
    fn a_dome_spreads_a_plane_wave_in_both_directions_and_obeys_the_reflection_law() {
        let radius = 2.0;
        let polygon = Polygon::new(&[[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0]]).unwrap();
        let towards = polygon.normal[2].signum();
        let bend = Bend::new(&polygon, -towards / radius, [1.0, 0.0, 0.0], [0.0, 0.0, 0.0], true);
        let found = bend.specular(&polygon, (0.0, 0.0), [0.0, 0.0, 1.0], None, [0.0, 0.0, 1.0]);
        assert_eq!(found.len(), 1);
        assert!((found[0].gain - 1.0 / (1.0 + 2.0 / radius)).abs() < 1e-9, "gain {}", found[0].gain);
        let to = [0.4, 0.2, 1.0];
        let source = [-0.5, 0.3, 1.5];
        let found = bend.specular(&polygon, (0.0, 0.0), to, Some(source), [0.0, 0.0, 1.0]);
        assert_eq!(found.len(), 1);
        let s = &found[0];
        let unit = |v: V3| scale(v, 1.0 / norm(v));
        let (a, b) = (unit(sub(source, s.point)), unit(sub(to, s.point)));
        assert!((dot(a, s.normal) - dot(b, s.normal)).abs() < 1e-9 && norm(sub(add(a, b), scale(s.normal, dot(add(a, b), s.normal)))) < 1e-9);
        assert!(bend.locate(&polygon, s.point).1.abs() < 1e-9);
    }

    #[test]
    fn a_convex_mirror_spreads_a_plane_wave_by_the_ray_tube_factor() {
        let radius = 2.0;
        let (polygon, bend) = wall(-1.0 / radius);
        let side = polygon.normal;
        let to = add([0.0, 0.0, 0.5], scale(side, 1.0));
        let found = bend.specular(&polygon, (-1.0, 1.0), to, None, side);
        assert_eq!(found.len(), 1);
        let expected = (1.0_f64 / (1.0 + 2.0 * 1.0 / radius)).sqrt();
        assert!((found[0].gain - expected).abs() < 1e-9, "gain {}", found[0].gain);
        assert!(norm(sub(found[0].point, [0.0, 0.0, 0.5])) < 1e-9);
    }
}
