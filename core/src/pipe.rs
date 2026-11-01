use crate::geom::{add, cross, dot, norm, normalize, scale, sub, V3};
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

    pub fn at(&self, frequency_hz: f64, speed_of_sound: f64, radius: f64, length: f64, along: f64, sin_incidence: f64) -> (Complex64, Complex64) {
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
            let (p, u) = guided(frequency_hz, speed_of_sound, radius, length, along, axial, 1.0);
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

#[derive(Clone, Copy, Debug)]
pub struct PipeInside {
    pub mouth_a: V3,
    pub mouth_b: V3,
    pub radius: f64,
    pub bend: Option<PipeBend>,
}

const SLICES: usize = 200;

fn turned(v: V3, axis: V3, angle: f64) -> V3 {
    let (sin, cos) = angle.sin_cos();
    add(add(scale(v, cos), scale(cross(axis, v), sin)), scale(axis, dot(axis, v) * (1.0 - cos)))
}

// Class B: Keefe and Benade 1983 slice model, long waves; overstates their 4.7 % measurement.
pub fn bend_speed_ratio(b: f64) -> f64 {
    if b <= 0.0 {
        return 1.0;
    }
    let b = b.min(0.999);
    let h = 2.0 / SLICES as f64;
    let sum: f64 = (0..SLICES)
        .map(|i| {
            let u = -1.0 + (i as f64 + 0.5) * h;
            let w = b * (1.0 - u * u).sqrt();
            ((1.0 + w) / (1.0 - w)).ln()
        })
        .sum();
    (sum * h / (PI * b)).sqrt()
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

fn speed_ratio(ka: f64, bend_ratio: f64) -> f64 {
    if bend_ratio <= 0.0 {
        return 1.0;
    }
    1.0 + (bend_speed_ratio(bend_ratio) - 1.0) * (1.0 - ka).max(0.0)
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

// Normalised impedances, ρc/S = 1.
pub fn transfer(frequency_hz: f64, speed_of_sound: f64, radius: f64, length: f64, along: f64, bend_ratio: f64) -> (Complex64, Complex64) {
    if frequency_hz <= 0.0 {
        return (Complex64::new(1.0 - along / length, 0.0), Complex64::new(0.0, 0.0));
    }
    let k = 2.0 * PI * frequency_hz / speed_of_sound;
    let faster = speed_ratio(k * radius, bend_ratio);
    let (pressure, velocity) = guided(frequency_hz, speed_of_sound, radius, length, along, 1.0 / faster, faster);
    (pressure, velocity * faster)
}

fn guided(frequency_hz: f64, speed_of_sound: f64, radius: f64, length: f64, along: f64, axial: f64, admittance: f64) -> (Complex64, Complex64) {
    let j = Complex64::new(0.0, 1.0);
    let k = 2.0 * PI * frequency_hz / speed_of_sound;
    let loss = WALL_LOSS_PER_ROOT_HZ * frequency_hz.sqrt() / radius;
    let wave = Complex64::new(k * axial, -loss);
    let load = radiation(k * radius) * axial * admittance;
    let t = (wave * length).tan();
    let input = (load + j * t) / (Complex64::new(1.0, 0.0) + j * load * t);
    let flow = Complex64::new(1.0, 0.0) / (load + input);
    let mouth = flow * input;
    let pressure = mouth * (wave * along).cos() - j * flow * (wave * along).sin();
    let velocity = flow * (wave * along).cos() - j * mouth * (wave * along).sin();
    (pressure, velocity)
}
