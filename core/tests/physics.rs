use neartect_core::filter::{near_filter, near_spectrum, spectrum_size};
use neartect_core::geom::{normalize, Polygon, V3};
use neartect_core::pattern::Pattern;
use neartect_core::scene::{Scene, Surface};
use rustfft::num_complex::Complex64;

const FS: f64 = 48000.0;
const C: f64 = 343.0;
const RIGID: [f64; 8] = [1.0; 8];

fn rectangle(centre: V3, half_u: V3, half_v: V3) -> Polygon {
    let p = |a: f64, b: f64| [
        centre[0] + a * half_u[0] + b * half_v[0],
        centre[1] + a * half_u[1] + b * half_v[1],
        centre[2] + a * half_u[2] + b * half_v[2],
    ];
    Polygon::new(&[p(-1.0, -1.0), p(1.0, -1.0), p(1.0, 1.0), p(-1.0, 1.0)]).unwrap()
}

fn floor(half: f64) -> Surface {
    surface(rectangle([0.0, 0.0, 0.0], [half, 0.0, 0.0], [0.0, half, 0.0]), RIGID)
}

fn surface(polygon: Polygon, reflection: [f64; 8]) -> Surface {
    Surface { polygon, strength: 1.0, reflection, pattern: Pattern::Solid }
}

fn scene(surfaces: Vec<Surface>, microphone: V3, arrival: V3, length: usize, max_order: usize) -> Scene {
    Scene { surfaces, microphone, arrival: normalize(arrival), speed_of_sound: C, sample_rate: FS, length, max_order }
}

fn at(spectrum: &[Complex64], length: usize, hz: f64) -> Complex64 {
    let n = spectrum_size(length);
    spectrum[(hz * n as f64 / FS).round() as usize]
}

fn mean_magnitude(spectrum: &[Complex64], length: usize, lo: f64, hi: f64) -> f64 {
    let n = spectrum_size(length);
    let a = (lo * n as f64 / FS) as usize;
    let b = (hi * n as f64 / FS) as usize;
    spectrum[a..=b].iter().map(|x| x.norm()).sum::<f64>() / (b - a + 1) as f64
}

#[test]
fn free_field_is_a_unit_impulse() {
    let h = near_filter(&scene(vec![], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], 256, 3));
    assert!((h[0] - 1.0).abs() < 1e-6);
    assert!(h[1..].iter().all(|x| x.abs() < 1e-6));
}

#[test]
fn zero_strength_removes_a_surface() {
    let mut s = floor(5.0);
    s.strength = 0.0;
    let h = near_filter(&scene(vec![s], [0.0, 0.0, 0.3], [0.0, 0.0, 1.0], 512, 3));
    assert!((h[0] - 1.0).abs() < 1e-6 && h[1..].iter().all(|x| x.abs() < 1e-6));
}

#[test]
fn wall_half_a_metre_away_has_its_first_notch_near_171_hz() {
    let length = 8192;
    let h = near_spectrum(&scene(vec![floor(20.0)], [0.0, 0.0, 0.5], [0.0, 0.0, 1.0], length, 1));
    let notch = at(&h, length, C / 2.0).norm();
    let peak = at(&h, length, C / 1.0).norm();
    assert!(notch < 0.25, "notch {notch}");
    assert!((peak - 2.0).abs() < 0.2, "peak {peak}");
}

#[test]
fn microphone_one_centimetre_from_a_wall_gains_6_db_below_8_6_khz() {
    let length = 4096;
    let h = near_spectrum(&scene(vec![floor(3.0)], [0.0, 0.0, 0.01], [0.0, 0.0, 1.0], length, 1));
    let low = mean_magnitude(&h, length, 200.0, 2000.0);
    let notch = at(&h, length, C / 0.04).norm();
    assert!((low - 2.0).abs() < 0.08, "low {low}");
    assert!(notch < 0.3, "notch {notch}");
}

#[test]
fn rigid_corner_gains_18_db_at_low_frequencies() {
    let half = 10.0;
    let floor = surface(rectangle([half, half, 0.0], [half, 0.0, 0.0], [0.0, half, 0.0]), RIGID);
    let wall_x = surface(rectangle([0.0, half, half], [0.0, half, 0.0], [0.0, 0.0, half]), RIGID);
    let wall_y = surface(rectangle([half, 0.0, half], [half, 0.0, 0.0], [0.0, 0.0, half]), RIGID);
    let length = 8192;
    let h = near_spectrum(&scene(vec![floor, wall_x, wall_y], [0.03, 0.05, 0.07], [1.0, 1.0, 1.0], length, 3));
    let low = mean_magnitude(&h, length, 60.0, 150.0);
    assert!((20.0 * low.log10() - 18.06).abs() < 1.0, "corner {} dB", 20.0 * low.log10());
}

#[test]
fn fully_absorbing_surface_reflects_nothing() {
    let s = surface(rectangle([0.0, 0.0, 0.0], [20.0, 0.0, 0.0], [0.0, 20.0, 0.0]), [0.0; 8]);
    let length = 4096;
    let h = near_spectrum(&scene(vec![s], [0.0, 0.0, 0.3], [0.0, 0.0, 1.0], length, 1));
    let deviation = mean_magnitude(&h, length, 200.0, 8000.0);
    assert!((deviation - 1.0).abs() < 0.05, "mean {deviation}");
}

#[test]
fn large_screen_blocks_the_arrival_behind_it() {
    let length = 4096;
    let h = near_spectrum(&scene(vec![floor(20.0)], [0.0, 0.0, -0.5], [0.0, 0.0, 1.0], length, 1));
    let behind = mean_magnitude(&h, length, 1000.0, 8000.0);
    assert!(behind < 0.05, "behind {behind}");
}

#[test]
fn small_panel_loses_its_bass_reflection() {
    let panel = surface(rectangle([0.0, 0.0, 0.0], [0.15, 0.0, 0.0], [0.0, 0.15, 0.0]), RIGID);
    let length = 4096;
    let h = near_spectrum(&scene(vec![panel], [0.0, 0.0, 0.2], [0.0, 0.0, 1.0], length, 1));
    let reflected = |hz: f64| (at(&h, length, hz) - Complex64::new(1.0, 0.0)).norm();
    let low = reflected(100.0);
    let high = (3000..6000).step_by(50).map(|f| reflected(f as f64)).sum::<f64>() / 60.0;
    assert!(low < 0.3, "low {low}");
    assert!((high - 1.0).abs() < 0.3, "high {high}");
}

#[test]
fn diffuse_incidence_at_a_wall_is_3_db_above_the_room_average() {
    let length = 2048;
    let mut energy = 0.0;
    let mut count = 0.0;
    for i in 0..12 {
        for j in 0..24 {
            let cos_elevation = (i as f64 + 0.5) / 12.0;
            let sin_elevation = (1.0 - cos_elevation * cos_elevation).sqrt();
            let azimuth = j as f64 / 24.0 * std::f64::consts::TAU;
            let arrival = [sin_elevation * azimuth.cos(), sin_elevation * azimuth.sin(), cos_elevation];
            let h = near_spectrum(&scene(vec![floor(4.0)], [0.0, 0.0, 0.01], arrival, length, 1));
            energy += at(&h, length, 300.0).norm_sqr();
            count += 1.0;
        }
    }
    let incident_and_reflected_far_from_the_wall = 2.0;
    let gain_db = 10.0 * (energy / count / incident_and_reflected_far_from_the_wall).log10();
    assert!((gain_db - 3.0).abs() < 1.0, "gain {gain_db} dB");
}

#[test]
fn identical_scenes_give_identical_filters() {
    let make = || scene(vec![floor(2.0)], [0.1, 0.2, 0.3], [0.3, 0.2, 1.0], 1024, 3);
    assert_eq!(near_filter(&make()), near_filter(&make()));
}

#[test]
fn filter_has_no_nan_or_infinity() {
    let h = near_filter(&scene(vec![floor(2.0)], [0.0, 0.0, 0.002], [0.5, 0.0, 0.2], 1024, 3));
    assert!(h.iter().all(|x| x.is_finite()));
}
