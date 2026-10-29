pub type V3 = [f64; 3];

pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn scale(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

pub fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}

pub fn normalize(a: V3) -> V3 {
    let n = norm(a);
    if n > 0.0 { scale(a, 1.0 / n) } else { a }
}

const EPSILON_METRES: f64 = 1e-9;

#[derive(Clone)]
pub struct Polygon {
    pub origin: V3,
    pub normal: V3,
    pub u: V3,
    pub v: V3,
    pub points: Vec<[f64; 2]>,
    pub area: f64,
}

impl Polygon {
    pub fn new(vertices: &[V3]) -> Option<Polygon> {
        if vertices.len() < 3 {
            return None;
        }
        let mut newell = [0.0; 3];
        for i in 0..vertices.len() {
            let a = vertices[i];
            let b = vertices[(i + 1) % vertices.len()];
            newell[0] += (a[1] - b[1]) * (a[2] + b[2]);
            newell[1] += (a[2] - b[2]) * (a[0] + b[0]);
            newell[2] += (a[0] - b[0]) * (a[1] + b[1]);
        }
        let twice_area = norm(newell);
        if twice_area < EPSILON_METRES {
            return None;
        }
        let normal = scale(newell, 1.0 / twice_area);
        let origin = vertices[0];
        let edge = vertices.iter().map(|&p| sub(p, origin)).find(|e| norm(*e) > EPSILON_METRES)?;
        let u = normalize(sub(edge, scale(normal, dot(edge, normal))));
        let v = cross(normal, u);
        let points = vertices
            .iter()
            .map(|&p| {
                let d = sub(p, origin);
                [dot(d, u), dot(d, v)]
            })
            .collect();
        Some(Polygon { origin, normal, u, v, points, area: twice_area / 2.0 })
    }

    pub fn to_plane(&self, p: V3) -> [f64; 2] {
        let d = sub(p, self.origin);
        [dot(d, self.u), dot(d, self.v)]
    }

    pub fn to_space(&self, q: [f64; 2]) -> V3 {
        add(self.origin, add(scale(self.u, q[0]), scale(self.v, q[1])))
    }

    pub fn signed_distance(&self, p: V3) -> f64 {
        dot(sub(p, self.origin), self.normal)
    }

    pub fn contains(&self, q: [f64; 2]) -> bool {
        let mut inside = false;
        let n = self.points.len();
        let mut j = n - 1;
        for i in 0..n {
            let a = self.points[i];
            let b = self.points[j];
            if (a[1] > q[1]) != (b[1] > q[1]) && q[0] < (b[0] - a[0]) * (q[1] - a[1]) / (b[1] - a[1]) + a[0] {
                inside = !inside;
            }
            j = i;
        }
        inside
    }

    pub fn bounds(&self) -> ([f64; 2], [f64; 2]) {
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for p in &self.points {
            lo = [lo[0].min(p[0]), lo[1].min(p[1])];
            hi = [hi[0].max(p[0]), hi[1].max(p[1])];
        }
        (lo, hi)
    }

    pub fn boundary_distance(&self, q: [f64; 2]) -> f64 {
        let n = self.points.len();
        (0..n)
            .map(|i| {
                let a = self.points[i];
                let b = self.points[(i + 1) % n];
                let ab = [b[0] - a[0], b[1] - a[1]];
                let aq = [q[0] - a[0], q[1] - a[1]];
                let t = ((aq[0] * ab[0] + aq[1] * ab[1]) / (ab[0] * ab[0] + ab[1] * ab[1]).max(1e-30)).clamp(0.0, 1.0);
                ((aq[0] - t * ab[0]).powi(2) + (aq[1] - t * ab[1]).powi(2)).sqrt()
            })
            .fold(f64::INFINITY, f64::min)
    }

    // `q` mirrored across the line of its nearest edge, and that edge's unit
    // normal in plane coordinates.
    pub fn mirror_across_nearest_edge(&self, q: [f64; 2]) -> ([f64; 2], [f64; 2]) {
        let n = self.points.len();
        let mut best = (f64::INFINITY, q, [0.0, 0.0]);
        for i in 0..n {
            let a = self.points[i];
            let b = self.points[(i + 1) % n];
            let ab = [b[0] - a[0], b[1] - a[1]];
            let length = (ab[0] * ab[0] + ab[1] * ab[1]).sqrt().max(1e-30);
            let aq = [q[0] - a[0], q[1] - a[1]];
            let t = ((aq[0] * ab[0] + aq[1] * ab[1]) / (length * length)).clamp(0.0, 1.0);
            let gap = ((aq[0] - t * ab[0]).powi(2) + (aq[1] - t * ab[1]).powi(2)).sqrt();
            if gap < best.0 {
                let normal = [-ab[1] / length, ab[0] / length];
                let across = aq[0] * normal[0] + aq[1] * normal[1];
                best = (gap, [q[0] - 2.0 * across * normal[0], q[1] - 2.0 * across * normal[1]], normal);
            }
        }
        (best.1, best.2)
    }

    pub fn edge(&self, i: usize) -> (V3, V3) {
        let n = self.points.len();
        (self.to_space(self.points[i]), self.to_space(self.points[(i + 1) % n]))
    }

    pub fn offset(&self, distances: &[f64]) -> Polygon {
        let n = self.points.len();
        let line = |i: usize| {
            let a = self.points[i];
            let b = self.points[(i + 1) % n];
            let d = [b[0] - a[0], b[1] - a[1]];
            let length = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-30);
            let outward = [d[1] / length, -d[0] / length];
            let shift = distances[i];
            ([a[0] + outward[0] * shift, a[1] + outward[1] * shift], d, outward)
        };
        let points = (0..n)
            .map(|i| {
                let (p1, d1, o1) = line((i + n - 1) % n);
                let (p2, d2, o2) = line(i);
                let det = d1[0] * d2[1] - d1[1] * d2[0];
                if det.abs() < 1e-12 {
                    let shift = distances[i].max(distances[(i + n - 1) % n]);
                    let q = self.points[i];
                    let o = if distances[i] >= distances[(i + n - 1) % n] { o2 } else { o1 };
                    return [q[0] + o[0] * shift, q[1] + o[1] * shift];
                }
                let t = ((p2[0] - p1[0]) * d2[1] - (p2[1] - p1[1]) * d2[0]) / det;
                [p1[0] + t * d1[0], p1[1] + t * d1[1]]
            })
            .collect::<Vec<_>>();
        let twice_area: f64 = (0..n).map(|i| {
            let a = points[i];
            let b = points[(i + 1) % n];
            a[0] * b[1] - b[0] * a[1]
        }).sum();
        Polygon { origin: self.origin, normal: self.normal, u: self.u, v: self.v, points, area: twice_area.abs() / 2.0 }
    }

    pub fn ray_hit(&self, from: V3, direction: V3, max_length: f64) -> Option<f64> {
        let denominator = dot(direction, self.normal);
        if denominator.abs() < 1e-12 {
            return None;
        }
        let t = -self.signed_distance(from) / denominator;
        if t <= 1e-7 || t >= max_length - 1e-7 {
            return None;
        }
        let hit = add(from, scale(direction, t));
        if self.contains(self.to_plane(hit)) { Some(t) } else { None }
    }

    pub fn mirror_point(&self, p: V3) -> V3 {
        sub(p, scale(self.normal, 2.0 * self.signed_distance(p)))
    }

    pub fn mirror_direction(&self, d: V3) -> V3 {
        sub(d, scale(self.normal, 2.0 * dot(d, self.normal)))
    }
}

pub fn blocked(polygons: &[Polygon], skip: &[usize], from: V3, direction: V3, max_length: f64) -> bool {
    polygons
        .iter()
        .enumerate()
        .any(|(i, p)| !skip.contains(&i) && p.ray_hit(from, direction, max_length).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(size: f64, z: f64) -> Polygon {
        Polygon::new(&[[0.0, 0.0, z], [size, 0.0, z], [size, size, z], [0.0, size, z]]).unwrap()
    }

    #[test]
    fn square_has_area_and_upward_normal() {
        let p = square(2.0, 0.0);
        assert!((p.area - 4.0).abs() < 1e-12);
        assert!((p.normal[2] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn contains_concave_l_shape() {
        let p = Polygon::new(&[
            [0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 1.0, 0.0],
            [1.0, 1.0, 0.0], [1.0, 2.0, 0.0], [0.0, 2.0, 0.0],
        ]).unwrap();
        assert!(p.contains(p.to_plane([0.5, 1.5, 0.0])));
        assert!(!p.contains(p.to_plane([1.5, 1.5, 0.0])));
    }

    #[test]
    fn ray_hits_only_inside_polygon() {
        let p = square(1.0, 0.0);
        assert!(p.ray_hit([0.5, 0.5, 1.0], [0.0, 0.0, -1.0], 10.0).is_some());
        assert!(p.ray_hit([1.5, 0.5, 1.0], [0.0, 0.0, -1.0], 10.0).is_none());
        assert!(p.ray_hit([0.5, 0.5, 1.0], [0.0, 0.0, -1.0], 0.5).is_none());
    }

    #[test]
    fn offset_moves_only_chosen_edges() {
        let p = square(1.0, 0.0);
        let q = p.offset(&[0.5, 0.0, 0.0, 0.0]);
        assert!((q.area - 1.5).abs() < 1e-9, "area {}", q.area);
        assert!((p.offset(&[0.0; 4]).area - 1.0).abs() < 1e-9);
    }

    #[test]
    fn mirror_point_reflects_through_plane() {
        let p = square(1.0, 0.0);
        let m = p.mirror_point([0.2, 0.3, 0.4]);
        assert!((m[2] + 0.4).abs() < 1e-12 && (m[0] - 0.2).abs() < 1e-12);
    }
}
