// A measured head (B2): SADIE II KU100 (University of York, Apache 2.0;
// Armstrong et al. 2018, Applied Sciences 8(11), 2029), diffuse-field
// equalised. Each direction is a minimum-phase response and a pure delay
// (class B; Kulkarni et al. 1999 for minimum phase plus delay), the left ear
// stands for both by symmetry, and the responses are 17 principal spectra
// with weights on rings of fixed elevation every 5°. Weights and delays are
// interpolated linearly between directions. Built by `tools/hrtf-table.py`;
// against the measured directions the third-octave levels differ by 0.3 dB
// (median) and 1.4 dB (95 %), the delays by 2.4 µs (95 %).

use rustfft::num_complex::Complex64;
use rustfft::FftPlanner;
use std::f64::consts::PI;
use std::sync::OnceLock;

pub const SETS: usize = 17;
const TABLE: &[u8] = include_bytes!("../data/ku100.bin");
const FINE_SIZE: usize = 8192;
const FADE_START_HZ: f64 = 20000.0;

pub struct Table {
    rate: f64,
    elevations: Vec<f64>,
    steps: usize,
    delays: Vec<f64>,
    weights: Vec<f64>,
    spectra: Vec<Vec<Complex64>>,
    pub latency: f64,
}

fn read_f32(bytes: &[u8], at: usize, count: usize) -> Vec<f64> {
    (0..count).map(|i| f32::from_le_bytes(bytes[at + 4 * i..at + 4 * i + 4].try_into().unwrap()) as f64).collect()
}

fn read_table() -> Table {
    let word = |i: usize| u32::from_le_bytes(TABLE[4 * i..4 * i + 4].try_into().unwrap()) as usize;
    let (sets, taps, rings, steps) = (word(0), word(1), word(2), word(3));
    assert_eq!(sets, SETS, "head table has {sets} sets");
    let rate = read_f32(TABLE, 16, 1)[0];
    let mut at = 20;
    let elevations = read_f32(TABLE, at, rings);
    at += 4 * rings;
    let delays = read_f32(TABLE, at, rings * steps);
    at += 4 * rings * steps;
    let weights = read_f32(TABLE, at, rings * steps * sets);
    at += 4 * rings * steps * sets;
    let basis = read_f32(TABLE, at, sets * taps);
    let fft = FftPlanner::new().plan_fft_forward(FINE_SIZE);
    let spectra = basis
        .chunks(taps)
        .map(|taps| {
            let mut buffer: Vec<Complex64> = (0..FINE_SIZE).map(|i| Complex64::new(*taps.get(i).unwrap_or(&0.0), 0.0)).collect();
            fft.process(&mut buffer);
            buffer.truncate(FINE_SIZE / 2 + 1);
            buffer
        })
        .collect();
    let latency = delays.iter().fold(0.0_f64, |m, &d| m.max(-d));
    Table { rate, elevations, steps, delays, weights, spectra, latency }
}

pub fn table() -> &'static Table {
    static CELL: OnceLock<Table> = OnceLock::new();
    CELL.get_or_init(read_table)
}

impl Table {
    // Delay in seconds (relative to the head centre, plus the latency that
    // keeps every delay positive) and the set weights for an arrival from
    // `front`, `outward` (towards the ear), and `up` components.
    pub fn split(&self, front: f64, outward: f64, up: f64) -> (f64, [f64; SETS]) {
        let azimuth = outward.atan2(front).to_degrees().rem_euclid(360.0);
        let elevation = up.clamp(-1.0, 1.0).asin().to_degrees();
        let rings = self.elevations.len();
        let k = self.elevations.partition_point(|&e| e <= elevation).saturating_sub(1).min(rings - 2);
        let t = ((elevation - self.elevations[k]) / (self.elevations[k + 1] - self.elevations[k])).clamp(0.0, 1.0);
        let s = azimuth / (360.0 / self.steps as f64);
        let i = (s.floor() as usize) % self.steps;
        let j = (i + 1) % self.steps;
        let f = s - s.floor();
        let corners = [(k, i, (1.0 - t) * (1.0 - f)), (k, j, (1.0 - t) * f), (k + 1, i, t * (1.0 - f)), (k + 1, j, t * f)];
        let mut delay = self.latency;
        let mut weights = [0.0; SETS];
        for (ring, step, share) in corners {
            let point = ring * self.steps + step;
            delay += share * self.delays[point];
            for (w, value) in weights.iter_mut().zip(&self.weights[point * SETS..(point + 1) * SETS]) {
                *w += share * value;
            }
        }
        (delay, weights)
    }

    pub fn factor(&self, set: usize, omega: f64) -> Complex64 {
        let hz = omega / (2.0 * PI);
        let nyquist = self.rate / 2.0;
        if hz >= nyquist {
            return Complex64::new(0.0, 0.0);
        }
        let x = hz / self.rate * FINE_SIZE as f64;
        let i = x.floor() as usize;
        let f = x - i as f64;
        let spectrum = &self.spectra[set];
        let value = spectrum[i] * (1.0 - f) + spectrum[(i + 1).min(FINE_SIZE / 2)] * f;
        let fade = if hz > FADE_START_HZ { 0.5 + 0.5 * (PI * (hz - FADE_START_HZ) / (nyquist - FADE_START_HZ)).cos() } else { 1.0 };
        value * fade
    }
}
