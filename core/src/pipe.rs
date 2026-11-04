use crate::geom::{add, cross, dot, norm, normalize, scale, sub, V3};
use crate::relief::Relief;
use crate::spectrum::{band_magnitude, BAND_COUNT};
use rustfft::num_complex::Complex64;
use std::f64::consts::PI;

const CROSS_MODES: [(i32, f64); 22] = [
    (1, 1.8412),
    (2, 3.0542),
    (0, 3.8317),
    (3, 4.2012),
    (4, 5.3176),
    (1, 5.3314),
    (5, 6.4156),
    (2, 6.7061),
    (0, 7.0156),
    (6, 7.5013),
    (3, 8.0152),
    (1, 8.5363),
    (7, 8.5778),
    (4, 9.2824),
    (8, 9.6474),
    (2, 9.9695),
    (0, 10.1735),
    (5, 10.5199),
    (9, 10.7114),
    (3, 11.3459),
    (6, 11.7349),
    (4, 12.6819),
];
const MODE_ONSET: f64 = 0.05;
const BESSEL_STEPS: usize = 64;

fn bessel(m: i32, x: f64) -> f64 {
    let h = PI / BESSEL_STEPS as f64;
    let f = |t: f64| (m as f64 * t - x * t.sin()).cos();
    let inner: f64 = (1..BESSEL_STEPS).map(|i| f(i as f64 * h)).sum();
    (inner + 0.5 * (f(0.0) + f(PI))) * h / PI
}

const HIGHEST_ORDER: usize = 10;

fn bessel_orders(x: f64) -> [f64; HIGHEST_ORDER + 2] {
    let h = PI / BESSEL_STEPS as f64;
    let mut sums = [0.0; HIGHEST_ORDER + 2];
    for i in 0..=BESSEL_STEPS {
        let t = i as f64 * h;
        let end = if i == 0 || i == BESSEL_STEPS { 0.5 } else { 1.0 };
        let phase = x * t.sin();
        for (m, sum) in sums.iter_mut().enumerate() {
            *sum += end * (m as f64 * t - phase).cos();
        }
    }
    sums.map(|v| v * h / PI)
}

pub fn plane_share(ka: f64, sin_incidence: f64) -> f64 {
    let beta = ka * sin_incidence;
    if beta < 1e-6 {
        1.0
    } else {
        2.0 * bessel(1, beta) / beta
    }
}

struct Mode {
    order: i32,
    zero: f64,
    at_zero: f64,
    next_at_zero: f64,
    norm: f64,
    shape: f64,
}

// Class C: coupling of the mouth's wave into cross modes and their end load.
pub struct CrossModes {
    modes: Vec<Mode>,
}

impl CrossModes {
    pub fn new(radius_fraction: f64, cos_azimuth: f64) -> CrossModes {
        let azimuth = cos_azimuth.clamp(-1.0, 1.0).acos();
        let modes = CROSS_MODES
            .iter()
            .map(|&(m, zero)| {
                let at_zero = bessel(m, zero);
                Mode {
                    order: m,
                    zero,
                    at_zero,
                    next_at_zero: bessel(m + 1, zero),
                    norm: 0.5 * (1.0 - (m as f64 / zero).powi(2)) * at_zero * at_zero,
                    shape: bessel(m, zero * radius_fraction) * (m as f64 * azimuth).cos(),
                }
            })
            .collect();
        CrossModes { modes }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn at(&self, frequency_hz: f64, speed_of_sound: f64, radius: f64, length: f64, along: f64, sin_incidence: f64, medium: &Medium) -> (Complex64, Complex64) {
        let ka = 2.0 * PI * frequency_hz / speed_of_sound * radius;
        let mut pressure = Complex64::new(0.0, 0.0);
        let mut velocity = Complex64::new(0.0, 0.0);
        let beta = ka * sin_incidence;
        let mut orders = None;
        for mode in self.modes.iter().filter(|mode| mode.zero < ka) {
            let j = orders.get_or_insert_with(|| bessel_orders(beta));
            let m = mode.order as usize;
            let overlap = if (beta - mode.zero).abs() < 1e-6 * mode.zero {
                mode.norm
            } else {
                (beta * j[m + 1] * mode.at_zero - mode.zero * j[m] * mode.next_at_zero) / (beta * beta - mode.zero * mode.zero)
            };
            let onset = ((ka / mode.zero - 1.0) / MODE_ONSET).min(1.0);
            let neumann = if m == 0 { 1.0 } else { 2.0 };
            let share = Complex64::from_polar(neumann * overlap / mode.norm, -PI / 2.0 * m as f64);
            let axial = (ka * ka - mode.zero * mode.zero).sqrt() / ka;
            let loss = medium.cross_mode_loss(mode.zero, axial, radius);
            let wave = Complex64::new(medium.k * axial, -loss);
            let load = radiation(ka) * axial;
            let (p, u) = driven(&[Piece::new(length, wave, Complex64::new(1.0, 0.0), 1.0)], load, load, along);
            pressure += share * mode.shape * p * onset;
            velocity += share * mode.shape * u * onset * axial;
        }
        (pressure, velocity)
    }
}

const END_CORRECTION_PER_RADIUS: f64 = 0.6133;
const WALL_LOSS_PER_ROOT_HZ: f64 = 2.96e-5;

// Turns positive from mouth A towards mouth B.
#[derive(Clone, Copy, Debug)]
pub struct PipeBend {
    pub centre: V3,
    pub axis: V3,
}

#[derive(Clone, Debug)]
pub struct PipeInside {
    pub mouth_a: V3,
    pub mouth_b: V3,
    pub radius: f64,
    pub bend: Option<PipeBend>,
    pub wall: PipeWall,
}

// The relief runs on the unrolled wall: first coordinate around the pipe, second along it from the middle.
#[derive(Clone, Debug)]
pub struct PipeWall {
    pub reflection: [f64; BAND_COUNT],
    pub relief: Relief,
}

const KINEMATIC_VISCOSITY: f64 = 1.5e-5;
const AROUND_SAMPLES: usize = 64;
const PROFILE_STEPS_PER_PERIOD: f64 = 12.0;
const MOST_PROFILE_STEPS: usize = 2048;
const NARROWEST_RADIUS_FRACTION: f64 = 0.1;
const FIRST_PRESSURE_RELEASE_ZERO: f64 = 2.4048;
const SERIES_TERMS: usize = 40;
const NEWTON_STEPS: usize = 12;
const SLOWEST_SCATTERING_AXIAL: f64 = 0.1;
const REPEAT_FLOOR: f64 = 1e-3;
const PIECE_KEY_METRES: f64 = 1e-9;

impl PipeWall {
    pub fn rigid() -> PipeWall {
        PipeWall { reflection: [1.0; BAND_COUNT], relief: Relief::flat() }
    }

    // Class B: locally reacting wall, Morse and Ingard 1968.
    fn admittance(&self, frequency_hz: f64) -> f64 {
        let r = band_magnitude(&self.reflection, frequency_hz).clamp(0.0, 1.0);
        (1.0 - r) / (1.0 + r)
    }

    // Class C.
    fn rough_loss_factor(&self, frequency_hz: f64) -> f64 {
        let sigma = self.relief.rough_height;
        if sigma <= 0.0 {
            return 1.0;
        }
        let layer_squared = 2.0 * KINEMATIC_VISCOSITY / (2.0 * PI * frequency_hz);
        let area = (1.0 + 2.0 * self.relief.rough_slope() * self.relief.rough_slope()).sqrt();
        1.0 + (area - 1.0) * sigma * sigma / (sigma * sigma + layer_squared)
    }

    // Class B for the plane mode: Webster, radius averaged around the wall.
    pub fn profile(&self, radius: f64, length: f64) -> Vec<(f64, f64)> {
        if self.relief.is_shapeless() {
            return vec![(length, radius)];
        }
        let along = self.relief.direction[1].abs();
        let repeat = if along > REPEAT_FLOOR { self.relief.period / along } else { self.relief.period };
        let step = (repeat / PROFILE_STEPS_PER_PERIOD).max(length / MOST_PROFILE_STEPS as f64);
        let whole = ((length / step) as usize).min(MOST_PROFILE_STEPS);
        let rest = length - whole as f64 * step;
        let around = 2.0 * PI * radius / AROUND_SAMPLES as f64;
        let radius_at = |z: f64| {
            let mean = (0..AROUND_SAMPLES).map(|j| self.relief.shape_at([j as f64 * around, z - 0.5 * length])).sum::<f64>() / AROUND_SAMPLES as f64;
            (radius + mean).max(NARROWEST_RADIUS_FRACTION * radius)
        };
        let mut pieces: Vec<(f64, f64)> = (0..whole).map(|i| (step, radius_at((i as f64 + 0.5) * step))).collect();
        if rest > 1e-9 * length || pieces.is_empty() {
            pieces.push((rest.max(1e-9 * length), radius_at(length - 0.5 * rest)));
        }
        let (low, high) = pieces.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &(_, r)| (a.min(r), b.max(r)));
        if high - low < 1e-9 * radius {
            vec![(length, pieces[0].1)]
        } else {
            pieces
        }
    }

    pub fn medium(&self, frequency_hz: f64, speed_of_sound: f64, radius: f64) -> Medium {
        let k = 2.0 * PI * frequency_hz / speed_of_sound;
        Medium {
            k,
            frequency_hz,
            radius,
            eigen: lined_eigenvalue(k * radius, self.admittance(frequency_hz)),
            rough_loss: self.rough_loss_factor(frequency_hz),
            rough_height: self.relief.rough_height,
        }
    }
}

pub struct Medium {
    k: f64,
    frequency_hz: f64,
    radius: f64,
    eigen: Complex64,
    rough_loss: f64,
    rough_height: f64,
}

impl Medium {
    fn wall_loss(&self, radius: f64) -> f64 {
        WALL_LOSS_PER_ROOT_HZ * self.frequency_hz.sqrt() / radius * self.rough_loss
    }

    fn lined_ratio(&self, radius: f64) -> Complex64 {
        if self.eigen == Complex64::new(0.0, 0.0) {
            return Complex64::new(1.0, 0.0);
        }
        (Complex64::new(1.0, 0.0) - self.eigen * (radius / self.radius) / (self.k * radius).powi(2)).sqrt()
    }

    // Class C: Eckart 1953 per wall hit.
    fn cross_mode_loss(&self, zero: f64, axial: f64, radius: f64) -> f64 {
        let lining = -self.k * self.lined_ratio(radius).im;
        let kappa = zero / radius;
        let hits_per_metre = kappa / (2.0 * radius * (self.k * axial).max(SLOWEST_SCATTERING_AXIAL * kappa));
        let scattering = 2.0 * (kappa * self.rough_height).powi(2) * hits_per_metre;
        self.wall_loss(radius) + lining + scattering
    }
}

// Class B: Morse and Ingard 1968, lined circular duct, for s = (κa)².
pub fn lined_eigenvalue(ka: f64, admittance: f64) -> Complex64 {
    if admittance <= 0.0 || ka <= 0.0 {
        return Complex64::new(0.0, 0.0);
    }
    let c = Complex64::new(0.0, ka * admittance);
    let mut s = c / (0.5 + c / (FIRST_PRESSURE_RELEASE_ZERO * FIRST_PRESSURE_RELEASE_ZERO));
    for _ in 0..NEWTON_STEPS {
        let quarter = s / 4.0;
        let (mut j0, mut x_j1, mut j0_slope) = (Complex64::new(0.0, 0.0), Complex64::new(0.0, 0.0), Complex64::new(0.0, 0.0));
        let mut term = Complex64::new(1.0, 0.0);
        for m in 0..SERIES_TERMS {
            let next = term * (-quarter) / ((m + 1) as f64 * (m + 1) as f64);
            j0 += term;
            x_j1 -= next * 2.0 * ((m + 1) as f64);
            j0_slope -= term / (4.0 * (m + 1) as f64);
            term = next;
        }
        let step = (x_j1 - c * j0) / (j0 * 0.5 - c * j0_slope);
        s -= step;
        if step.norm() < 1e-12 * (1.0 + s.norm()) {
            break;
        }
    }
    s
}

#[derive(Clone, Copy)]
struct Piece {
    length: f64,
    wave: Complex64,
    impedance: Complex64,
    area: f64,
    pass: Complex64,
}

impl Piece {
    fn new(length: f64, wave: Complex64, impedance: Complex64, area: f64) -> Piece {
        Piece { length, wave, impedance, area, pass: (Complex64::new(0.0, -1.0) * wave * length).exp() }
    }
}

// Written with decaying exponentials only, so long lossy pipes stay finite.
fn driven(pieces: &[Piece], near_load: Complex64, far_load: Complex64, along: f64) -> (Complex64, Complex64) {
    let j = Complex64::new(0.0, 1.0);
    let one = Complex64::new(1.0, 0.0);
    let mut reflections = Vec::with_capacity(pieces.len());
    let mut load = far_load;
    for piece in pieces.iter().rev() {
        let at_end = (load - piece.impedance) / (load + piece.impedance);
        reflections.push(at_end);
        let at_start = at_end * piece.pass * piece.pass;
        load = piece.impedance * (one + at_start) / (one - at_start);
    }
    reflections.reverse();
    let flow = one / (near_load + load);
    let (mut p, mut u) = (flow * load, flow);
    let mut start = 0.0;
    for (index, (piece, &at_end)) in pieces.iter().zip(&reflections).enumerate() {
        let forward = 0.5 * (p + piece.impedance * u);
        let last = along < start + piece.length || index + 1 == pieces.len();
        let (ahead, back) = if last {
            let x = (along - start).clamp(0.0, piece.length);
            ((-j * piece.wave * x).exp(), at_end * (-j * piece.wave * (2.0 * piece.length - x)).exp())
        } else {
            (piece.pass, at_end * piece.pass)
        };
        let (inside_p, inside_u) = (forward * (ahead + back), forward * (ahead - back) / piece.impedance);
        if last {
            return (inside_p, inside_u / piece.area);
        }
        (p, u) = (inside_p, inside_u);
        start += piece.length;
    }
    (p, u)
}

const EQUIVALENT_WIDTH_PER_DIAMETER: f64 = PI / 4.0;
const BEND_NODES: usize = 64;
const BISECTIONS: usize = 60;
const LONG_WAVE: f64 = 1e-3;

fn turned(v: V3, axis: V3, angle: f64) -> V3 {
    let (sin, cos) = angle.sin_cos();
    add(add(scale(v, cos), scale(cross(axis, v), sin)), scale(axis, dot(axis, v) * (1.0 - cos)))
}

// Class B: Cummings 1974 via Rostafinski 1991 (§7, §14): a rectangular bend π/4 of the diameter wide.
pub fn bend_speed_ratio(k: f64, radius: f64, bend_ratio: f64) -> f64 {
    if bend_ratio <= 0.0 {
        return 1.0;
    }
    let centreline = radius / bend_ratio;
    let half_width = EQUIVALENT_WIDTH_PER_DIAMETER * radius;
    rectangular_bend_speed_ratio(k, centreline - half_width, centreline + half_width)
}

pub fn rectangular_bend_speed_ratio(k: f64, inner: f64, outer: f64) -> f64 {
    let k = k.max(LONG_WAVE / (outer - inner));
    k * 0.5 * (inner + outer) / angular_wavenumber(k, inner, outer)
}

fn angular_wavenumber(k: f64, inner: f64, outer: f64) -> f64 {
    let h = (outer - inner) / BEND_NODES as f64;
    let r = |i: f64| inner + i * h;
    let weight = |i: usize| if i == 0 || i == BEND_NODES { 0.5 * h } else { h };
    let mass: Vec<f64> = (0..=BEND_NODES).map(|i| weight(i) / r(i as f64)).collect();
    let diagonal: Vec<f64> = (0..=BEND_NODES)
        .map(|i| {
            let below = if i > 0 { r(i as f64 - 0.5) / h } else { 0.0 };
            let above = if i < BEND_NODES { r(i as f64 + 0.5) / h } else { 0.0 };
            (k * k * r(i as f64) * weight(i) - below - above) / mass[i]
        })
        .collect();
    let off: Vec<f64> = (0..BEND_NODES).map(|i| r(i as f64 + 0.5) / h / (mass[i] * mass[i + 1]).sqrt()).collect();
    let below_count = |x: f64| {
        let mut count = 0;
        let mut d = 1.0;
        for i in 0..=BEND_NODES {
            d = diagonal[i] - x - if i > 0 { off[i - 1] * off[i - 1] / d } else { 0.0 };
            if d == 0.0 {
                d = 1e-300;
            }
            if d < 0.0 {
                count += 1;
            }
        }
        count
    };
    let reach = |i: usize| (if i > 0 { off[i - 1].abs() } else { 0.0 }) + (if i < BEND_NODES { off[i].abs() } else { 0.0 });
    let mut lo = (0..=BEND_NODES).map(|i| diagonal[i] - reach(i)).fold(f64::INFINITY, f64::min);
    let mut hi = (0..=BEND_NODES).map(|i| diagonal[i] + reach(i)).fold(f64::NEG_INFINITY, f64::max);
    for _ in 0..BISECTIONS {
        let middle = 0.5 * (lo + hi);
        if below_count(middle) > BEND_NODES {
            hi = middle;
        } else {
            lo = middle;
        }
    }
    (0.5 * (lo + hi)).max(1e-30).sqrt()
}

impl PipeInside {
    fn radial(&self, bend: PipeBend, p: V3) -> V3 {
        let d = sub(p, bend.centre);
        sub(d, scale(bend.axis, dot(d, bend.axis)))
    }

    fn bend_radius(&self, bend: PipeBend) -> f64 {
        norm(self.radial(bend, self.mouth_a))
    }

    fn turn(&self, bend: PipeBend, p: V3) -> f64 {
        let a = normalize(self.radial(bend, self.mouth_a));
        let r = self.radial(bend, p);
        dot(bend.axis, cross(a, r)).atan2(dot(a, r)).rem_euclid(2.0 * PI)
    }

    pub fn length(&self) -> f64 {
        match self.bend {
            None => norm(sub(self.mouth_b, self.mouth_a)),
            Some(bend) => self.bend_radius(bend) * self.turn(bend, self.mouth_b),
        }
    }

    pub fn along(&self, point: V3) -> f64 {
        match self.bend {
            None => dot(sub(point, self.mouth_a), normalize(sub(self.mouth_b, self.mouth_a))).clamp(0.0, self.length()),
            Some(bend) => {
                let total = self.turn(bend, self.mouth_b);
                let angle = self.turn(bend, point);
                let angle = if angle <= total {
                    angle
                } else if angle > (total + 2.0 * PI) / 2.0 {
                    0.0
                } else {
                    total
                };
                angle * self.bend_radius(bend)
            }
        }
    }

    pub fn centre_point(&self, along: f64) -> V3 {
        match self.bend {
            None => add(self.mouth_a, scale(normalize(sub(self.mouth_b, self.mouth_a)), along)),
            Some(bend) => {
                let base = add(bend.centre, scale(bend.axis, dot(sub(self.mouth_a, bend.centre), bend.axis)));
                add(base, turned(self.radial(bend, self.mouth_a), bend.axis, along / self.bend_radius(bend)))
            }
        }
    }

    pub fn tangent(&self, along: f64) -> V3 {
        match self.bend {
            None => normalize(sub(self.mouth_b, self.mouth_a)),
            Some(bend) => {
                let r = turned(normalize(self.radial(bend, self.mouth_a)), bend.axis, along / self.bend_radius(bend));
                normalize(cross(bend.axis, r))
            }
        }
    }

    pub fn carried(&self, v: V3, from: f64, to: f64) -> V3 {
        match self.bend {
            None => v,
            Some(bend) => turned(v, bend.axis, (to - from) / self.bend_radius(bend)),
        }
    }

    pub fn bend_ratio(&self) -> f64 {
        self.bend.map_or(0.0, |bend| self.radius / self.bend_radius(bend))
    }
}

fn radiation(ka: f64) -> Complex64 {
    let resistance = (ka * ka / 4.0) / (1.0 + ka * ka / 4.0);
    let reactance = END_CORRECTION_PER_RADIUS * ka / (1.0 + (END_CORRECTION_PER_RADIUS * ka).powi(2));
    Complex64::new(resistance, reactance)
}

// Class C: heuristic blend towards twice the incident pressure on axis.
pub fn received(ka: f64, cos_inward: f64) -> f64 {
    let blend = ka * ka / (1.0 + ka * ka);
    1.0 + blend * cos_inward.max(0.0)
}

// Normalised impedances, ρc/S = 1 at the nominal radius; `profile` runs from the driven mouth.
pub fn transfer(medium: &Medium, profile: &[(f64, f64)], along: f64, faster: f64) -> (Complex64, Complex64) {
    let length: f64 = profile.iter().map(|&(l, _)| l).sum();
    if medium.frequency_hz <= 0.0 {
        return (Complex64::new(1.0 - along / length, 0.0), Complex64::new(0.0, 0.0));
    }
    let key = |v: f64| (v / PIECE_KEY_METRES).round() as i64;
    let mut made: std::collections::HashMap<(i64, i64), Piece> = std::collections::HashMap::new();
    let pieces: Vec<Piece> = profile
        .iter()
        .map(|&(l, r)| {
            *made.entry((key(l), key(r))).or_insert_with(|| {
                let area = (r / medium.radius).powi(2);
                let ratio = medium.lined_ratio(r);
                Piece::new(l, medium.k * ratio / faster - Complex64::new(0.0, medium.wall_loss(r)), Complex64::new(1.0, 0.0) / (ratio * area), area)
            })
        })
        .collect();
    let end_load = |r: f64| radiation(medium.k * r) / (r / medium.radius).powi(2);
    let (pressure, velocity) = driven(&pieces, end_load(profile[0].1), end_load(profile[profile.len() - 1].1), along);
    (pressure, velocity * faster)
}
