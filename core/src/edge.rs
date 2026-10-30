// A finite panel cannot hold the pressure doubling that physical optics puts
// on its lit face when it is small against the wavelength: the air flows
// round its edges. The exact low-frequency jump across a thin rigid screen is
// 2 jk Φ p, where Φ is the potential of the screen moving through still air
// at unit speed: (2/π) √(a² − r²) on a disc (Lamb, Hydrodynamics § 108),
// √(a² − x²) across a strip. The near filter takes each surface's rigid part
// through H = sτ (1 + sτ) / (1 + √2 sτ + s²τ²), τ = Φ/c: jkΦ at low
// frequencies (class A), 1 at high ones, where it fades as 1/(kΦ)² so that a
// large surface keeps its physical-optics edges, and at most 0.7 dB above 1
// between (class C). One effective Φ per surface for its Kirchhoff sum, its
// plain mean for the images. Surfaces of one material, whose histograms are
// shared, take their effective Φ's mean weighted by how much of each the
// microphone hears.
//
// Φ at a point is the strip's form over the chord across the nearest edge,
// d1 to that edge and d2 to the far side, times a factor running from 1 for a
// long strip to 2/π where the crossing chord is no longer than that one, as
// on a disc. Integrated over rectangles it gives their added mass within 7 %
// of the tabulated values (Blevins 1979, table 14-2), a disc's within 0.1 %.

use crate::geom::{norm, sub, Polygon, V3};
use crate::pattern::Pattern;
use rustfft::num_complex::Complex64;
use std::f64::consts::FRAC_2_PI;

const GRID: usize = 48;
const SMALLEST_DEPTH_METRES: f64 = 1e-6;
const NEAREST_WEIGHT_METRES: f64 = 1e-3;

fn reach(points: &[[f64; 2]], q: [f64; 2], d: [f64; 2]) -> f64 {
    let n = points.len();
    let mut best = f64::INFINITY;
    for i in 0..n {
        let (a, b) = (points[i], points[(i + 1) % n]);
        let e = [b[0] - a[0], b[1] - a[1]];
        let denominator = d[0] * e[1] - d[1] * e[0];
        if denominator.abs() < 1e-15 {
            continue;
        }
        let w = [a[0] - q[0], a[1] - q[1]];
        let t = (w[0] * e[1] - w[1] * e[0]) / denominator;
        let s = (w[0] * d[1] - w[1] * d[0]) / denominator;
        if t > 1e-12 && (0.0..=1.0).contains(&s) {
            best = best.min(t);
        }
    }
    best
}

fn unit(v: [f64; 2]) -> Option<[f64; 2]> {
    let length = v[0].hypot(v[1]);
    (length > 1e-12).then(|| [v[0] / length, v[1] / length])
}

// Φ at plane point `q` of a screen with outline `polygon` and openings from
// `pattern`, in metres.
pub fn screen_depth(polygon: &Polygon, pattern: &Pattern, q: [f64; 2]) -> f64 {
    let points = &polygon.points;
    let n = points.len();
    let mut nearest = (f64::INFINITY, [0.0, 0.0]);
    for i in 0..n {
        let (a, b) = (points[i], points[(i + 1) % n]);
        let ab = [b[0] - a[0], b[1] - a[1]];
        let t = (((q[0] - a[0]) * ab[0] + (q[1] - a[1]) * ab[1]) / (ab[0] * ab[0] + ab[1] * ab[1]).max(1e-30)).clamp(0.0, 1.0);
        let closest = [a[0] + t * ab[0], a[1] + t * ab[1]];
        let gap = (q[0] - closest[0]).hypot(q[1] - closest[1]);
        if gap < nearest.0 {
            let inward = unit([q[0] - closest[0], q[1] - closest[1]]).or_else(|| unit([-ab[1], ab[0]])).unwrap_or([1.0, 0.0]);
            nearest = (gap, inward);
        }
    }
    if let Some((gap, away)) = pattern.opening_edge(q) {
        if gap < nearest.0 {
            nearest = (gap, away);
        }
    }
    let (d1, inward) = nearest;
    let d2 = reach(points, q, inward);
    let d2 = if d2.is_finite() { d2 } else { d1 };
    let across = [-inward[1], inward[0]];
    let crossing = reach(points, q, across) + reach(points, q, [-across[0], -across[1]]);
    let ratio = if crossing.is_finite() { (d1 + d2) / crossing } else { 0.0 };
    let shape = FRAC_2_PI + (1.0 - FRAC_2_PI) * (1.0 - ratio.min(1.0));
    ((d1 * d2).sqrt() * shape).max(SMALLEST_DEPTH_METRES)
}

pub fn response(omega: f64, depth: f64, speed_of_sound: f64) -> Complex64 {
    if !depth.is_finite() {
        return Complex64::new(1.0, 0.0);
    }
    let s = Complex64::new(0.0, omega * depth / speed_of_sound);
    s * (1.0 + s) / (1.0 + std::f64::consts::SQRT_2 * s + s * s)
}

// The response's biquad by the bilinear transform with K = 2τ in samples:
// b0, b1, b2, a1, a2 with a0 = 1.
pub fn biquad(k: f64) -> [f64; 5] {
    let a0 = 1.0 + std::f64::consts::SQRT_2 * k + k * k;
    [(k + k * k) / a0, -2.0 * k * k / a0, (k * k - k) / a0, (2.0 - 2.0 * k * k) / a0, (1.0 - std::f64::consts::SQRT_2 * k + k * k) / a0]
}

fn solid_grid(polygon: &Polygon, pattern: &Pattern) -> Vec<[f64; 2]> {
    let (lo, hi) = polygon.bounds();
    let step = [(hi[0] - lo[0]) / GRID as f64, (hi[1] - lo[1]) / GRID as f64];
    (0..GRID * GRID)
        .map(|n| [lo[0] + ((n / GRID) as f64 + 0.5) * step[0], lo[1] + ((n % GRID) as f64 + 0.5) * step[1]])
        .filter(|&q| polygon.contains(q) && pattern.is_solid(q))
        .collect()
}

pub fn mean_depth(polygon: &Polygon, pattern: &Pattern) -> f64 {
    let grid = solid_grid(polygon, pattern);
    if grid.is_empty() {
        return f64::INFINITY;
    }
    grid.iter().map(|&q| screen_depth(polygon, pattern, q)).sum::<f64>() / grid.len() as f64
}

// One Φ for a whole surface as heard at the microphone: the mean of Φ over
// the surface weighted by each part's physical-optics share there, 1/(R Φ̄)
// from afar and 1/R² close by, where Φ̄ is its plain mean. `reach` is the
// harmonic distance to the microphone and the source.
// Returns Φ and the total weight.
pub fn effective_depth(polygon: &Polygon, pattern: &Pattern, to_space: impl Fn([f64; 2]) -> V3, distance: impl Fn(V3) -> f64) -> (f64, f64) {
    let samples: Vec<(f64, f64)> =
        solid_grid(polygon, pattern).into_iter().map(|q| (screen_depth(polygon, pattern, q), distance(to_space(q)).max(NEAREST_WEIGHT_METRES))).collect();
    if samples.is_empty() {
        return (f64::INFINITY, 0.0);
    }
    let (lo, hi) = polygon.bounds();
    let cell = (hi[0] - lo[0]) * (hi[1] - lo[1]) / (GRID * GRID) as f64;
    let plain = samples.iter().map(|(depth, _)| depth).sum::<f64>() / samples.len() as f64;
    let (weighted, total) = samples.iter().fold((0.0, 0.0), |(sum, total), &(depth, r)| {
        let weight = 1.0 / (r * plain) + 1.0 / (r * r);
        (sum + weight * depth, total + weight)
    });
    (weighted / total, total * cell)
}

pub fn harmonic_reach(point: V3, microphone: V3, source_distance: f64) -> f64 {
    let r = norm(sub(point, microphone));
    if source_distance.is_finite() {
        r * source_distance / (r + source_distance)
    } else {
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn added_mass(polygon: &Polygon) -> f64 {
        let (lo, hi) = polygon.bounds();
        let steps = 400;
        let (du, dv) = ((hi[0] - lo[0]) / steps as f64, (hi[1] - lo[1]) / steps as f64);
        let mut sum = 0.0;
        for i in 0..steps {
            for j in 0..steps {
                let q = [lo[0] + (i as f64 + 0.5) * du, lo[1] + (j as f64 + 0.5) * dv];
                if polygon.contains(q) {
                    sum += 2.0 * screen_depth(polygon, &Pattern::Solid, q) * du * dv;
                }
            }
        }
        sum
    }

    fn rectangle(a: f64, b: f64) -> Polygon {
        Polygon::new(&[[0.0, 0.0, 0.0], [a, 0.0, 0.0], [a, b, 0.0], [0.0, b, 0.0]]).unwrap()
    }

    #[test]
    fn a_disc_takes_lambs_added_mass() {
        let a = 0.3;
        let disc: Vec<V3> = (0..256)
            .map(|i| {
                let t = i as f64 / 256.0 * std::f64::consts::TAU;
                [a * t.cos(), a * t.sin(), 0.0]
            })
            .collect();
        let m = added_mass(&Polygon::new(&disc).unwrap());
        let exact = 8.0 / 3.0 * a * a * a;
        assert!((m / exact - 1.0).abs() < 0.01, "disc {m} vs {exact}");
    }

    #[test]
    fn rectangles_take_their_tabulated_added_mass() {
        for (aspect, k) in [(1.0, 0.579), (2.0, 0.757), (4.0, 0.871)] {
            let m = added_mass(&rectangle(1.0, aspect));
            let table = k * std::f64::consts::PI / 4.0 * aspect;
            assert!((m / table - 1.0).abs() < 0.08, "aspect {aspect}: {m} vs {table}");
        }
    }

    #[test]
    fn depth_follows_the_strip_across_a_long_panel() {
        let strip = rectangle(1.0, 40.0);
        let phi = screen_depth(&strip, &Pattern::Solid, [0.25, 20.0]);
        let exact = (0.5_f64 * 0.5 - 0.25 * 0.25).sqrt();
        assert!((phi / exact - 1.0).abs() < 0.02, "{phi} vs {exact}");
    }
}

#[cfg(test)]
mod response_tests {
    use super::*;

    #[test]
    fn the_biquad_matches_the_analogue_response_below_a_tenth_of_the_rate() {
        let rate = 96000.0;
        for depth in [0.01, 0.1, 1.0] {
            let tau = depth / 343.0 * rate;
            let [b0, b1, b2, a1, a2] = biquad(2.0 * tau);
            for hz in [20.0, 200.0, 2000.0] {
                let w = std::f64::consts::TAU * hz / rate;
                let z = Complex64::from_polar(1.0, -w);
                let digital = (b0 + b1 * z + b2 * z * z) / (1.0 + a1 * z + a2 * z * z);
                let analogue = response(std::f64::consts::TAU * hz, depth, 343.0);
                assert!((digital - analogue).norm() < 0.01, "{depth} m {hz} Hz: {digital} vs {analogue}");
            }
        }
    }
}
