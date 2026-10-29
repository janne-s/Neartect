use crate::images::specular_paths;
use crate::kirchhoff::{scatter, Histograms, OVERSAMPLING};
use crate::scene::Scene;
use crate::spectrum::{angular_frequency, minimum_phase, Fft};
use rustfft::num_complex::Complex64;
use std::f64::consts::PI;

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 { 1.0 } else { (PI * x).sin() / (PI * x) }
}

pub fn spectrum_size(length: usize) -> usize {
    2 * length.max(16).next_power_of_two()
}

pub fn near_spectrum(scene: &Scene) -> Vec<Complex64> {
    let n = spectrum_size(scene.length);
    let n_fine = n * OVERSAMPLING;
    let half = n / 2;
    let fs = scene.sample_rate;
    let c = scene.speed_of_sound;
    let mut fft = Fft::new();
    let mut total = vec![Complex64::new(1.0, 0.0); half + 1];
    let reflection: Vec<Vec<Complex64>> = scene
        .surfaces
        .iter()
        .map(|s| minimum_phase(&s.reflection, n, fs, &mut fft))
        .collect();
    let j = Complex64::new(0.0, 1.0);

    let mut groups: Vec<(Vec<u64>, usize, Histograms)> = Vec::new();
    for (index, surface) in scene.surfaces.iter().enumerate() {
        if surface.strength <= 0.0 {
            continue;
        }
        let Some(h) = scatter(scene, index, n_fine) else { continue };
        let key: Vec<u64> = surface.reflection.iter().map(|r| r.to_bits()).chain(surface.pattern.low_frequency_key()).collect();
        let s = surface.strength;
        match groups.iter_mut().find(|(k, _, _)| *k == key) {
            Some((_, _, sum)) => {
                for i in 0..n_fine {
                    sum.rigid_far[i] += s * h.rigid_far[i];
                    sum.rigid_near[i] += s * h.rigid_near[i];
                    sum.soft[i] += s * h.soft[i];
                }
            }
            None => {
                let scaled = Histograms {
                    rigid_far: h.rigid_far.iter().map(|x| x * s).collect(),
                    rigid_near: h.rigid_near.iter().map(|x| x * s).collect(),
                    soft: h.soft.iter().map(|x| x * s).collect(),
                };
                groups.push((key, index, scaled));
            }
        }
    }

    for (_, index, h) in &groups {
        let surface = &scene.surfaces[*index];
        let mut packed: Vec<Complex64> = h.rigid_far.iter().zip(&h.soft).map(|(&a, &b)| Complex64::new(a, b)).collect();
        let mut near: Vec<Complex64> = h.rigid_near.iter().map(|&a| Complex64::new(a, 0.0)).collect();
        fft.forward(&mut packed);
        fft.forward(&mut near);
        for k in 0..=half {
            let mirror = packed[(n_fine - k) % n_fine].conj();
            let rigid_far = (packed[k] + mirror) * 0.5;
            let soft = (packed[k] - mirror) * Complex64::new(0.0, -0.5);
            let omega_over_c = angular_frequency(k, n, fs) / c;
            let compensation = 1.0 / sinc(k as f64 / n_fine as f64).powi(2);
            let rigid = j * omega_over_c * rigid_far + near[k];
            let pressure_release = -(j * omega_over_c) * soft;
            let r = reflection[*index][k];
            let scattered = (Complex64::new(1.0, 0.0) + r) * 0.5 * rigid + (Complex64::new(1.0, 0.0) - r) * 0.5 * pressure_release;
            let grating = surface.pattern.low_frequency_factor(k as f64 * fs / n as f64, c);
            total[k] += scattered * (compensation * grating);
        }
    }

    for path in specular_paths(scene) {
        let strength: f64 = path.reflections.iter().map(|r| scene.surfaces[r.surface].strength).product();
        for k in 0..=half {
            let omega = angular_frequency(k, n, fs);
            let mut gain = Complex64::new(strength, 0.0) * Complex64::from_polar(1.0, -omega * path.delay_seconds);
            for r in &path.reflections {
                let cutoff = 2.0 * PI * r.cutoff_hz;
                let grating = scene.surfaces[r.surface].pattern.low_frequency_factor(k as f64 * fs / n as f64, c);
                gain *= reflection[r.surface][k] * (j * omega) / (j * omega + cutoff) * grating;
            }
            total[k] += gain;
        }
    }
    total
}

pub fn near_filter(scene: &Scene) -> Vec<f32> {
    let half_spectrum = near_spectrum(scene);
    let n = spectrum_size(scene.length);
    let mut full = vec![Complex64::new(0.0, 0.0); n];
    for k in 0..=n / 2 {
        full[k] = half_spectrum[k];
        if k > 0 && k < n / 2 {
            full[n - k] = half_spectrum[k].conj();
        }
    }
    full[n / 2] = Complex64::new(full[n / 2].re, 0.0);
    Fft::new().inverse(&mut full);
    full[..scene.length].iter().map(|x| x.re as f32).collect()
}
