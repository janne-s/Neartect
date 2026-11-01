// The inside of an open pipe as a waveguide (class B): each mouth is driven
// by the near field just outside it, loaded by the unflanged radiation
// impedance (Levine–Schwinger low-ka form with end correction 0.6133 a), and
// the wave loses energy to the walls by viscothermal attenuation
// 2.96e-5 √f / a Np/m (Kinsler et al., air at 20 °C). Above ka = j'mn the
// rigid duct's cross modes (m, n) propagate with k_z = √(k² − (j'mn/a)²)
// (Morse and Ingard 1968, ch. 9): the mouth's incident wave, taken as a plane
// wave across the mouth, is projected onto each mode (exact for that field),
// and each mode's open end takes the plane mode's load in its own impedance
// k/k_z (class C). Modes run to j'mn ≈ 12.7; above that the propagating set
// is incomplete. The rise of the received plane-mode pressure towards twice
// the incident wave on axis as ka grows is a heuristic blend (class C).

// A bent pipe follows a circular centreline. In the long-wave limit its
// plane mode keeps the straight pipe's compliance while its inertance falls:
// summing the cross-section as parallel rectangular slices (Keefe and Benade
// 1983, after Cummings 1974; Rostafinski 1991, NASA RP-1248, §14), phase
// velocity and wave admittance both rise by √(L₀/L), with
// L₀/L = (1/πB) ∫₋₁¹ ln((1 + B√(1 − u²))/(1 − B√(1 − u²))) du, B = a/R (class B;
// Keefe and Benade measured 4.7 % where this gives more). The rise is taken to
// fade by ka = 1, where Cummings' bends move at the straight speed (class C),
// and above it the bend's slower phase velocity is left out. Bends transmit
// the plane mode almost fully below the first cross mode (Cummings 1974:
// T ≥ 97.5 % even for R₂/R₁ = 10.3), so no reflection is added at the bend;
// cross modes run along the centreline as in a straight pipe (class C).

use crate::geom::{add, cross, dot, norm, normalize, scale, sub, V3};
use rustfft::num_complex::Complex64;
use std::f64::consts::PI;

// Zeros of J'm, (m, n, j'mn), for the rigid circular duct's cross modes.
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

// Bessel's integral, J_m(x) = (1/π) ∫₀^π cos(mτ − x sin τ) dτ, by the
// trapezoid rule, which converges geometrically for this periodic integrand.
fn bessel(m: i32, x: f64) -> f64 {
    let h = PI / BESSEL_STEPS as f64;
    let f = |t: f64| (m as f64 * t - x * t.sin()).cos();
    let inner: f64 = (1..BESSEL_STEPS).map(|i| f(i as f64 * h)).sum();
    (inner + 0.5 * (f(0.0) + f(PI))) * h / PI
}

const HIGHEST_ORDER: usize = 10;

// J₀ … J₁₀ at `x` in one pass of Bessel's integral.
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

// The plane mode's share of a plane wave crossing the mouth at
// `sin_incidence`, 2 J₁(β)/β with β = ka sin θ; the rest goes to cross modes.
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

// The cross modes heard by a microphone `radius_fraction` of the way out from
// the axis, at `cos_azimuth` from the incident wave's sideways direction.
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

    // Pressure and ρc times the axial velocity, per unit pressure of the
    // mouth's plane mode: each mode's share of a plane wave crossing the mouth
    // at `sin_incidence` (∫₀¹ x J_m(βx) J_m(γx) dx by Lommel, with its limit as
    // β → γ), its shape at the microphone, and its transfer over `along`.
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

// A bent centreline turns about `axis` through `centre`, positive from mouth
// A towards mouth B.
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

// √(L₀/L) for bend parameter B = a/R by the midpoint rule.
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

    // The angle from mouth A to `p` about the bend's axis, in [0, 2π).
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

    // The direction of travel from A towards B at `along`.
    pub fn tangent(&self, along: f64) -> V3 {
        match self.bend {
            None => normalize(sub(self.mouth_b, self.mouth_a)),
            Some(bend) => {
                let r = turned(normalize(self.radial(bend, self.mouth_a)), bend.axis, along / self.bend_radius(bend));
                normalize(cross(bend.axis, r))
            }
        }
    }

    // A vector across the pipe at `from` carried round the bend to `to`.
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

pub fn received(ka: f64, cos_inward: f64) -> f64 {
    let blend = ka * ka / (1.0 + ka * ka);
    1.0 + blend * cos_inward.max(0.0)
}

// Pressure and ρc times the particle velocity away from the driven mouth, at
// `along` metres from it, per unit blocked pressure there, with the far mouth
// open; normalised impedances (ρc/S = 1). A first-order microphone in the pipe
// hears a·p − (1 − a)·(facing · direction of travel)·ρc v.
pub fn transfer(frequency_hz: f64, speed_of_sound: f64, radius: f64, length: f64, along: f64, bend_ratio: f64) -> (Complex64, Complex64) {
    if frequency_hz <= 0.0 {
        return (Complex64::new(1.0 - along / length, 0.0), Complex64::new(0.0, 0.0));
    }
    let k = 2.0 * PI * frequency_hz / speed_of_sound;
    let faster = speed_ratio(k * radius, bend_ratio);
    let (pressure, velocity) = guided(frequency_hz, speed_of_sound, radius, length, along, 1.0 / faster, faster);
    (pressure, velocity * faster)
}

// One mode with axial wavenumber `axial`·k, in its own impedance k/k_z. A
// bend's plane mode runs at `axial` = 1/F with `admittance` F times the
// straight pipe's: the mouths' loads are F times larger in the line's own
// impedance, and ρc times the velocity is F times the line's own variable.
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
