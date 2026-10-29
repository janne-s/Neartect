use rustfft::num_complex::Complex64;
use rustfft::FftPlanner;
use std::f64::consts::PI;

pub const BAND_CENTRES_HZ: [f64; 8] = [63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0];
pub const BAND_COUNT: usize = BAND_CENTRES_HZ.len();
const SMALLEST_MAGNITUDE: f64 = 1e-5;

pub struct Fft {
    planner: FftPlanner<f64>,
}

impl Fft {
    pub fn new() -> Fft {
        Fft { planner: FftPlanner::new() }
    }

    pub fn forward(&mut self, data: &mut [Complex64]) {
        self.planner.plan_fft_forward(data.len()).process(data);
    }

    pub fn inverse(&mut self, data: &mut [Complex64]) {
        let n = data.len();
        self.planner.plan_fft_inverse(n).process(data);
        let s = 1.0 / n as f64;
        data.iter_mut().for_each(|x| *x *= s);
    }
}

pub fn band_magnitude(bands: &[f64; BAND_COUNT], frequency_hz: f64) -> f64 {
    if frequency_hz <= BAND_CENTRES_HZ[0] {
        return bands[0];
    }
    if frequency_hz >= BAND_CENTRES_HZ[BAND_COUNT - 1] {
        return bands[BAND_COUNT - 1];
    }
    let position = (frequency_hz / BAND_CENTRES_HZ[0]).log2();
    let i = position.floor() as usize;
    let f = position - i as f64;
    bands[i] * (1.0 - f) + bands[i + 1] * f
}

pub fn minimum_phase(bands: &[f64; BAND_COUNT], n: usize, sample_rate: f64, fft: &mut Fft) -> Vec<Complex64> {
    let mut cepstrum: Vec<Complex64> = (0..n)
        .map(|k| {
            let bin = if k <= n / 2 { k } else { n - k };
            let m = band_magnitude(bands, bin as f64 * sample_rate / n as f64).max(SMALLEST_MAGNITUDE);
            Complex64::new(m.ln(), 0.0)
        })
        .collect();
    fft.inverse(&mut cepstrum);
    for k in 1..n / 2 {
        cepstrum[k] *= 2.0;
    }
    for k in n / 2 + 1..n {
        cepstrum[k] = Complex64::new(0.0, 0.0);
    }
    fft.forward(&mut cepstrum);
    cepstrum.iter().map(|c| c.exp()).collect()
}

pub fn angular_frequency(bin: usize, n: usize, sample_rate: f64) -> f64 {
    2.0 * PI * bin as f64 * sample_rate / n as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimum_phase_keeps_band_magnitudes() {
        let bands = [1.0, 0.9, 0.8, 0.5, 0.3, 0.3, 0.2, 0.2];
        let mut fft = Fft::new();
        let n = 8192;
        let h = minimum_phase(&bands, n, 48000.0, &mut fft);
        for (i, &f) in BAND_CENTRES_HZ.iter().enumerate() {
            let bin = (f * n as f64 / 48000.0).round() as usize;
            assert!((h[bin].norm() - bands[i]).abs() < 0.02, "band {f}: {}", h[bin].norm());
        }
    }

    #[test]
    fn minimum_phase_is_causal() {
        let bands = [1.0, 0.95, 0.7, 0.4, 0.2, 0.15, 0.1, 0.1];
        let mut fft = Fft::new();
        let n = 4096;
        let mut h = minimum_phase(&bands, n, 48000.0, &mut fft);
        fft.inverse(&mut h);
        let late: f64 = h[n / 2..].iter().map(|x| x.norm_sqr()).sum();
        let total: f64 = h.iter().map(|x| x.norm_sqr()).sum();
        assert!(late / total < 1e-4);
    }
}
