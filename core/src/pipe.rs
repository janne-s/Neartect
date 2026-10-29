// The inside of an open pipe as a plane-wave waveguide (class B): each mouth
// is driven by the near field just outside it, loaded by the unflanged
// radiation impedance (Levine–Schwinger low-ka form with end correction
// 0.6133 a), and the wave loses energy to the walls by viscothermal
// attenuation 2.96e-5 √f / a Np/m (Kinsler et al., air at 20 °C). Valid below
// the first cross mode, ka < 1.84; above it the plane wave stands in for all
// modes. The rise of the received pressure towards twice the incident wave on
// axis as ka grows is a heuristic blend (class C).

use crate::geom::{dot, norm, normalize, sub, V3};
use rustfft::num_complex::Complex64;

const END_CORRECTION_PER_RADIUS: f64 = 0.6133;
const WALL_LOSS_PER_ROOT_HZ: f64 = 2.96e-5;

#[derive(Clone, Copy, Debug)]
pub struct PipeInside {
    pub mouth_a: V3,
    pub mouth_b: V3,
    pub radius: f64,
}

impl PipeInside {
    pub fn length(&self) -> f64 {
        norm(sub(self.mouth_b, self.mouth_a))
    }

    pub fn along(&self, point: V3) -> f64 {
        dot(sub(point, self.mouth_a), normalize(sub(self.mouth_b, self.mouth_a))).clamp(0.0, self.length())
    }
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
pub fn transfer(frequency_hz: f64, speed_of_sound: f64, radius: f64, length: f64, along: f64) -> (Complex64, Complex64) {
    let j = Complex64::new(0.0, 1.0);
    if frequency_hz <= 0.0 {
        return (Complex64::new(1.0 - along / length, 0.0), Complex64::new(0.0, 0.0));
    }
    let k = 2.0 * std::f64::consts::PI * frequency_hz / speed_of_sound;
    let loss = WALL_LOSS_PER_ROOT_HZ * frequency_hz.sqrt() / radius;
    let wave = Complex64::new(k, -loss);
    let load = radiation(k * radius);
    let t = (wave * length).tan();
    let input = (load + j * t) / (Complex64::new(1.0, 0.0) + j * load * t);
    let flow = Complex64::new(1.0, 0.0) / (load + input);
    let mouth = flow * input;
    let pressure = mouth * (wave * along).cos() - j * flow * (wave * along).sin();
    let velocity = flow * (wave * along).cos() - j * mouth * (wave * along).sin();
    (pressure, velocity)
}
