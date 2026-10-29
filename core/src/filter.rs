use crate::images::specular_paths;
use crate::kirchhoff::{relief_floor, scatter_into, traversals, Histograms, OVERSAMPLING, RIGID_FAR, RIGID_NEAR, SOFT_FAR, SOFT_NEAR};
use crate::geom::{dot, scale, sub, V3};
use crate::pipe::{plane_share, received, transfer, CrossModes, PipeInside};
use crate::scene::Scene;
use crate::spectrum::{angular_frequency, minimum_phase, Fft};
use rustfft::num_complex::Complex64;
use std::f64::consts::PI;

// Scattering that arrives after the filter's end reaches only discarded
// samples, except through the few samples the interpolation compensation and
// the head's spectra reach backwards, and through a pipe mouth that hears the
// sound before the microphone does (`lead_seconds`).
const HORIZON_MARGIN_SAMPLES: f64 = 256.0;

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 { 1.0 } else { (PI * x).sin() / (PI * x) }
}

pub fn spectrum_size(length: usize) -> usize {
    2 * length.max(16).next_power_of_two()
}

pub fn near_spectrum(scene: &Scene) -> Vec<Complex64> {
    let scene = &scene.off_surfaces();
    let mut spectrum = match scene.pipe {
        Some(pipe) => inside_pipe(scene, pipe),
        None => outside_spectrum(scene, 0.0),
    };
    if scene.gain != 1.0 || scene.delay_seconds != 0.0 {
        let n = spectrum_size(scene.length);
        for (k, value) in spectrum.iter_mut().enumerate() {
            let omega = angular_frequency(k, n, scene.sample_rate);
            *value *= Complex64::from_polar(scene.gain, -omega * scene.delay_seconds);
        }
    }
    spectrum
}

fn inside_pipe(scene: &Scene, pipe: PipeInside) -> Vec<Complex64> {
    let n = spectrum_size(scene.length);
    let fs = scene.sample_rate;
    let c = scene.speed_of_sound;
    let length = pipe.length();
    let along = pipe.along(scene.microphone);
    let axis = crate::geom::normalize(sub(pipe.mouth_b, pipe.mouth_a));
    let offset = sub(sub(scene.microphone, pipe.mouth_a), scale(axis, dot(sub(scene.microphone, pipe.mouth_a), axis)));
    let radius_fraction = (crate::geom::norm(offset) / pipe.radius).min(1.0);
    let sideways = |travel: V3| {
        let across = sub(travel, scale(axis, dot(travel, axis)));
        let (a, o) = (crate::geom::norm(across), crate::geom::norm(offset));
        let cos_azimuth = if a > 1e-9 && o > 1e-9 { dot(across, offset) / (a * o) } else { 1.0 };
        (a.min(1.0), cos_azimuth)
    };
    let at_mouth = |mouth: V3| {
        let mut outside = scene.clone();
        outside.pipe = None;
        outside.microphone = mouth;
        outside.listener = crate::scene::Listener::omni();
        let incident = scene.incident(mouth);
        let spectrum = outside_spectrum(&outside, (length + pipe.radius) / c);
        let travel = scale(incident.towards_source, -1.0);
        (spectrum, incident.amplitude, incident.extra_metres / c, dot(travel, axis), sideways(travel))
    };
    let (pattern, gradient, facing_along) = match scene.listener {
        crate::scene::Listener::Microphone { pattern, facing } => (pattern, 1.0 - pattern, dot(facing, axis)),
        crate::scene::Listener::Ear { .. } | crate::scene::Listener::MeasuredEar { .. } => (1.0, 0.0, 0.0),
    };
    let (from_a, gain_a, delay_a, inward_a, (sin_a, azimuth_a)) = at_mouth(pipe.mouth_a);
    let (from_b, gain_b, delay_b, inward_b, (sin_b, azimuth_b)) = at_mouth(pipe.mouth_b);
    let (modes_a, modes_b) = (CrossModes::new(radius_fraction, azimuth_a), CrossModes::new(radius_fraction, azimuth_b));
    (0..=n / 2)
        .map(|k| {
            let f = k as f64 * fs / n as f64;
            let omega = angular_frequency(k, n, fs);
            let ka = 2.0 * PI * f / c * pipe.radius;
            let (p_a, u_a) = transfer(f, c, pipe.radius, length, along);
            let (p_b, u_b) = transfer(f, c, pipe.radius, length, length - along);
            let (cross_p_a, cross_u_a) = modes_a.at(f, c, pipe.radius, length, along, sin_a);
            let (cross_p_b, cross_u_b) = modes_b.at(f, c, pipe.radius, length, length - along, sin_b);
            let plane_a = received(ka, inward_a) * plane_share(ka, sin_a);
            let plane_b = received(ka, -inward_b) * plane_share(ka, sin_b);
            let (p_a, u_a) = (p_a * plane_a + cross_p_a, u_a * plane_a + cross_u_a);
            let (p_b, u_b) = (p_b * plane_b + cross_p_b, u_b * plane_b + cross_u_b);
            let heard_a = p_a * pattern - u_a * (gradient * facing_along);
            let heard_b = p_b * pattern + u_b * (gradient * facing_along);
            from_a[k] * Complex64::from_polar(gain_a, -omega * delay_a) * heard_a
                + from_b[k] * Complex64::from_polar(gain_b, -omega * delay_b) * heard_b
        })
        .collect()
}

fn outside_spectrum(scene: &Scene, lead_seconds: f64) -> Vec<Complex64> {
    let n = spectrum_size(scene.length);
    let n_fine = n * OVERSAMPLING;
    let horizon = (((scene.length as f64 + HORIZON_MARGIN_SAMPLES + lead_seconds * scene.sample_rate) as usize) * OVERSAMPLING).min(n_fine);
    let half = n / 2;
    let fs = scene.sample_rate;
    let c = scene.speed_of_sound;
    let mut fft = Fft::new();
    let direct_distance = scene.direct_distance();
    let direct_from = scene.direct_from();
    let factors: Vec<Vec<Complex64>> = (0..scene.sets()).map(|set| (0..=half).map(|k| scene.set_factor(set, angular_frequency(k, n, fs))).collect()).collect();
    let heard = |(delay, weights): &(f64, [f64; crate::scene::MOST_SETS]), k: usize| -> Complex64 {
        let sum: Complex64 = factors.iter().zip(weights).map(|(f, w)| f[k] * w).sum();
        sum * Complex64::from_polar(1.0, -angular_frequency(k, n, fs) * delay)
    };
    let direct_split = scene.split(direct_from, direct_distance);
    let mut total: Vec<Complex64> = (0..=half).map(|k| heard(&direct_split, k)).collect();
    let reflection: Vec<Vec<Complex64>> = scene
        .surfaces
        .iter()
        .map(|s| minimum_phase(&s.reflection, n, fs, &mut fft))
        .collect();
    let j = Complex64::new(0.0, 1.0);

    let mut groups: Vec<(Vec<u64>, usize, Option<usize>, Histograms)> = Vec::new();
    let traversals = traversals(scene);
    let floor = relief_floor(scene, &traversals, horizon);
    for &(index, also) in &traversals {
        let surface = &scene.surfaces[index];
        let mut key: Vec<u64> = surface.reflection.iter().map(|r| r.to_bits()).chain(surface.pattern.low_frequency_key()).collect();
        if let Some(j) = also {
            key.push(u64::MAX);
            key.extend(scene.surfaces[j].reflection.iter().map(|r| r.to_bits()).chain(scene.surfaces[j].pattern.low_frequency_key()));
        }
        let s = surface.strength * also.map_or(1.0, |j| scene.surfaces[j].strength);
        let existing = groups.iter().position(|(k, _, _, _)| *k == key);
        let at = existing.unwrap_or_else(|| {
            groups.push((key, index, also, Histograms::new(scene.sets(), horizon)));
            groups.len() - 1
        });
        if scatter_into(scene, index, also, s, floor, &mut groups[at].3).is_none() && existing.is_none() {
            groups.pop();
        }
    }

    for (_, index, also, h) in &groups {
        let surface = &scene.surfaces[*index];
        for set in 0..h.sets() {
            let mut packed: Vec<Complex64> = h.list(set, RIGID_FAR).zip(h.list(set, SOFT_FAR)).map(|(a, b)| Complex64::new(a, b)).collect();
            let mut near: Vec<Complex64> = h.list(set, RIGID_NEAR).zip(h.list(set, SOFT_NEAR)).map(|(a, b)| Complex64::new(a, b)).collect();
            packed.resize(n_fine, Complex64::new(0.0, 0.0));
            near.resize(n_fine, Complex64::new(0.0, 0.0));
            fft.forward(&mut packed);
            fft.forward(&mut near);
            for k in 0..=half {
                let mirror = packed[(n_fine - k) % n_fine].conj();
                let rigid_far = (packed[k] + mirror) * 0.5;
                let soft = (packed[k] - mirror) * Complex64::new(0.0, -0.5);
                let omega = angular_frequency(k, n, fs);
                let omega_over_c = omega / c;
                let compensation = 1.0 / sinc(k as f64 / n_fine as f64).powi(2);
                let near_mirror = near[(n_fine - k) % n_fine].conj();
                let rigid_near = (near[k] + near_mirror) * 0.5;
                let soft_near = (near[k] - near_mirror) * Complex64::new(0.0, -0.5);
                let rigid = j * omega_over_c * rigid_far + rigid_near;
                let pressure_release = -(j * omega_over_c * soft + soft_near);
                let r = reflection[*index][k];
                let scattered = (Complex64::new(1.0, 0.0) + r) * 0.5 * rigid + (Complex64::new(1.0, 0.0) - r) * 0.5 * pressure_release;
                let hz = k as f64 * fs / n as f64;
                let grating = surface.pattern.low_frequency_factor(hz, c);
                let second = also.map_or(Complex64::new(1.0, 0.0), |j| reflection[j][k] * scene.surfaces[j].pattern.low_frequency_factor(hz, c));
                total[k] += scattered * second * factors[set][k] * (compensation * grating);
            }
        }
    }

    for path in specular_paths(scene) {
        let strength: f64 = path.reflections.iter().map(|r| scene.surfaces[r.surface].strength).product();
        let path_split = scene.split(path.from, path.distance);
        for k in 0..=half {
            let omega = angular_frequency(k, n, fs);
            let mut gain = heard(&path_split, k) * (strength * path.gain) * Complex64::from_polar(1.0, -omega * path.delay_seconds);
            for r in &path.reflections {
                let cutoff = 2.0 * PI * r.cutoff_hz;
                let grating = scene.surfaces[r.surface].pattern.low_frequency_factor(k as f64 * fs / n as f64, c);
                let rough = (-2.0 * (omega / c * r.rough_metres).powi(2)).exp();
                gain *= reflection[r.surface][k] * (j * omega) / (j * omega + cutoff) * (grating * rough);
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
