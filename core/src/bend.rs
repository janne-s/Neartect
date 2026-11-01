// Plane coordinates along `along` are arc length; positive curvature bends towards the polygon’s normal.

use crate::geom::{add, dot, norm, scale, sub, Polygon, V3};

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
}

pub struct Specular {
    pub point: V3,
    pub normal: V3,
    pub plane: [f64; 2],
    pub gain: f64,
}

impl Bend {
    pub fn flat() -> Bend {
        Bend { curvature: 0.0, along: [1.0, 0.0], start: 0.0 }
    }

    pub fn new(polygon: &Polygon, curvature: f64, direction: V3, touching: V3) -> Bend {
        let along = [dot(direction, polygon.u), dot(direction, polygon.v)];
        let length = (along[0] * along[0] + along[1] * along[1]).sqrt();
        if curvature.is_nan() || curvature.abs() <= FLAT_CURVATURE || length < 1e-9 {
            return Bend::flat();
        }
        let along = [along[0] / length, along[1] / length];
        let q = polygon.to_plane(touching);
        Bend { curvature, along, start: q[0] * along[0] + q[1] * along[1] }
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
        polygon.to_space([self.along[0] * self.start, self.along[1] * self.start])
    }

    pub fn point(&self, polygon: &Polygon, q: [f64; 2]) -> V3 {
        let flat = polygon.to_space(q);
        if !self.is_bent() {
            return flat;
        }
        let s = self.arc(q);
        let k = self.curvature;
        let (sin, cos) = (k * s).sin_cos();
        let e = self.along_space(polygon);
        add(flat, add(scale(e, sin / k - s), scale(polygon.normal, (1.0 - cos) / k)))
    }

    pub fn rotate(&self, polygon: &Polygon, q: [f64; 2], w: V3) -> V3 {
        if !self.is_bent() {
            return w;
        }
        let (sin, cos) = (self.curvature * self.arc(q)).sin_cos();
        let e = self.along_space(polygon);
        let n = polygon.normal;
        let (we, wn) = (dot(w, e), dot(w, n));
        add(w, add(scale(add(scale(e, cos - 1.0), scale(n, sin)), we), scale(add(scale(n, cos - 1.0), scale(e, -sin)), wn)))
    }

    fn centre(&self, polygon: &Polygon) -> V3 {
        add(self.touching(polygon), scale(polygon.normal, 1.0 / self.curvature))
    }

    fn radial(&self, polygon: &Polygon, p: V3) -> V3 {
        let w = sub(p, self.centre(polygon));
        let axis = self.axis_space(polygon);
        sub(w, scale(axis, dot(w, axis)))
    }

    fn plane_of(&self, polygon: &Polygon, p: V3, radial: V3) -> [f64; 2] {
        let sign = self.curvature.signum();
        let r0 = scale(polygon.normal, -sign);
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
        let axis = self.axis_space(polygon);
        let f = self.radial(polygon, from);
        let d = sub(direction, scale(axis, dot(direction, axis)));
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
        l > 0.0 && ((d[0] * self.along[0] + d[1] * self.along[1]) / l).abs() > 1.0 - 1e-6
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
        let bend = Bend::new(&polygon, curvature, [1.0, 0.0, 0.0], [0.0, 0.0, 0.5]);
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
