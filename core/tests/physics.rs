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
    Scene { surfaces, microphone, arrival: normalize(arrival), speed_of_sound: C, sample_rate: FS, length, max_order, pipe: None, source: None, gain: 1.0, delay_seconds: 0.0 }
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

fn grating(period: [f64; 2], bar: [f64; 2], half: f64) -> Surface {
    Surface {
        polygon: rectangle([0.0, 0.0, 0.0], [half, 0.0, 0.0], [0.0, half, 0.0]),
        strength: 1.0,
        reflection: RIGID,
        pattern: Pattern::Grid { period, bar, thickness: 0.003 },
    }
}

#[test]
fn slats_are_transparent_to_bass_and_reflect_treble() {
    let length = 4096;
    let slats = || grating([0.02, 0.0], [0.01, 0.0], 1.5);
    let front = near_spectrum(&scene(vec![slats()], [0.0, 0.0, 0.3], [0.0, 0.0, 1.0], length, 1));
    let behind = near_spectrum(&scene(vec![slats()], [0.0, 0.0, -0.3], [0.0, 0.0, 1.0], length, 1));
    let reflected = |h: &[Complex64], lo: f64, hi: f64| {
        let n = spectrum_size(length);
        let a = (lo * n as f64 / FS) as usize;
        let b = (hi * n as f64 / FS) as usize;
        h[a..=b].iter().map(|x| (x - Complex64::new(1.0, 0.0)).norm()).sum::<f64>() / (b - a + 1) as f64
    };
    assert!(reflected(&front, 100.0, 300.0) < 0.15, "bass reflection {}", reflected(&front, 100.0, 300.0));
    assert!(mean_magnitude(&behind, length, 100.0, 300.0) > 0.85);
    let treble = reflected(&front, 4000.0, 8000.0);
    assert!(treble > 0.25 && treble < 0.9, "treble reflection {treble}");
}

#[test]
fn slats_seen_at_an_angle_give_a_repetition_pitch() {
    let length = 4096;
    let slats = grating([0.05, 0.0], [0.01, 0.0], 1.0);
    let h = near_filter(&scene(vec![slats], [0.0, 0.0, 0.5], [-1.0, 0.0, 1.0], length, 1));
    let peak = h[1..].iter().map(|x| x.abs()).fold(0.0f32, f32::max);
    let taps = (2..length - 1)
        .filter(|&i| h[i].abs() > 0.2 * peak && h[i].abs() >= h[i - 1].abs() && h[i].abs() >= h[i + 1].abs())
        .count();
    assert!(taps >= 5, "only {taps} taps: the bars do not repeat");
}

#[test]
fn round_opening_lets_sound_through_a_screen() {
    let mut screen = floor(5.0);
    let solid = near_spectrum(&scene(vec![floor(5.0)], [0.0, 0.0, -0.3], [0.0, 0.0, 1.0], 4096, 1));
    screen.pattern = Pattern::Opening { centre: [5.0, 5.0], half: [0.3, 0.3], round: true };
    let open = near_spectrum(&scene(vec![screen], [0.0, 0.0, -0.3], [0.0, 0.0, 1.0], 4096, 1));
    let through = mean_magnitude(&open, 4096, 2000.0, 8000.0);
    assert!(mean_magnitude(&solid, 4096, 2000.0, 8000.0) < 0.1 && through > 0.6, "through {through}");
}

#[test]
fn outside_of_a_box_edge_is_not_extended() {
    let top = surface(rectangle([0.0, 0.0, 1.0], [0.5, 0.0, 0.0], [0.0, 0.5, 0.0]), RIGID);
    let side = surface(rectangle([0.5, 0.0, 0.5], [0.0, 0.5, 0.0], [0.0, 0.0, 0.5]), RIGID);
    let length = 4096;
    let alone = near_spectrum(&scene(vec![surface(rectangle([0.0, 0.0, 1.0], [0.5, 0.0, 0.0], [0.0, 0.5, 0.0]), RIGID)], [0.0, 0.0, 1.3], [0.0, 0.0, 1.0], length, 1));
    let with_side = near_spectrum(&scene(vec![top, side], [0.0, 0.0, 1.3], [0.0, 0.0, 1.0], length, 1));
    let difference = mean_magnitude(&alone, length, 100.0, 300.0) - mean_magnitude(&with_side, length, 100.0, 300.0);
    assert!(difference.abs() < 0.1, "a convex edge changed the bass by {difference}");
}

#[test]
fn niche_lip_is_not_extended_into_the_room() {
    let wall_with_hole = Surface {
        polygon: rectangle([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        strength: 1.0,
        reflection: RIGID,
        pattern: Pattern::Opening { centre: [1.0, 1.0], half: [0.2, 0.2], round: false },
    };
    let side = surface(rectangle([-0.2, -0.1, 1.0], [0.0, 0.1, 0.0], [0.0, 0.0, 0.2]), RIGID);
    let length = 2048;
    let s = scene(vec![wall_with_hole, side], [0.0, 0.5, 1.0], [0.0, 1.0, 0.0], length, 1);
    let lip = neartect_core::kirchhoff::extended_edges(&s, 1);
    assert_eq!(lip, 0, "the niche side was extended past its lip");
}

#[test]
fn hidden_back_of_a_box_is_not_extended() {
    let h = 0.3;
    let faces = vec![
        surface(rectangle([0.0, h, 1.0], [h, 0.0, 0.0], [0.0, 0.0, h]), RIGID),
        surface(rectangle([0.0, -h, 1.0], [h, 0.0, 0.0], [0.0, 0.0, h]), RIGID),
        surface(rectangle([h, 0.0, 1.0], [0.0, h, 0.0], [0.0, 0.0, h]), RIGID),
        surface(rectangle([-h, 0.0, 1.0], [0.0, h, 0.0], [0.0, 0.0, h]), RIGID),
        surface(rectangle([0.0, 0.0, 1.0 + h], [h, 0.0, 0.0], [0.0, h, 0.0]), RIGID),
        surface(rectangle([0.0, 0.0, 1.0 - h], [h, 0.0, 0.0], [0.0, h, 0.0]), RIGID),
    ];
    let s = scene(faces, [0.1, 0.9, 1.1], [0.0, 1.0, 0.0], 1024, 1);
    assert_eq!(neartect_core::kirchhoff::extended_edges(&s, 1), 0);
}

fn inside_pipe(along: f64, arrival: V3) -> Vec<Complex64> {
    let mut s = scene(vec![], [0.0, along, 0.0], arrival, 8192, 1);
    s.pipe = Some(neartect_core::pipe::PipeInside { mouth_a: [0.0, 0.0, 0.0], mouth_b: [0.0, 1.0, 0.0], radius: 0.05 });
    near_spectrum(&s)
}

#[test]
fn pipe_passes_the_outside_pressure_at_low_frequency() {
    let h = inside_pipe(0.5, [1.0, 0.0, 0.0]);
    let low = at(&h, 8192, 15.0).norm();
    assert!((low - 1.0).abs() < 0.1, "low {low}");
}

#[test]
fn open_pipe_resonates_at_its_end_corrected_length() {
    let h = inside_pipe(0.5, [1.0, 0.0, 0.0]);
    let n = spectrum_size(8192);
    let (a, b) = ((100.0 * n as f64 / FS) as usize, (250.0 * n as f64 / FS) as usize);
    let peak = (a..=b).max_by(|&i, &j| h[i].norm().partial_cmp(&h[j].norm()).unwrap()).unwrap();
    let peak_hz = peak as f64 * FS / n as f64;
    let expected = C / (2.0 * (1.0 + 2.0 * 0.6133 * 0.05));
    assert!((peak_hz / expected - 1.0).abs() < 0.05, "peak {peak_hz} Hz, expected {expected}");
    assert!(h[peak].norm() > 3.0, "resonance only {}", h[peak].norm());
}

fn from_point(surfaces: Vec<Surface>, microphone: V3, source: V3, length: usize, max_order: usize) -> Scene {
    let mut s = scene(surfaces, microphone, [0.0, 0.0, 1.0], length, max_order);
    s.source = Some(source);
    s
}

fn distance(a: V3, b: V3) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[test]
fn point_source_in_free_field_is_a_unit_impulse() {
    let h = near_filter(&from_point(vec![], [0.0, 0.0, 1.0], [2.0, 1.0, 1.5], 256, 3));
    assert!((h[0] - 1.0).abs() < 1e-6);
    assert!(h[1..].iter().all(|x| x.abs() < 1e-6));
}

#[test]
fn point_source_over_a_rigid_floor_matches_its_image() {
    let length = 4096;
    let (microphone, source) = ([1.0, 0.0, 0.5], [0.0, 0.0, 1.0]);
    let h = near_spectrum(&from_point(vec![floor(20.0)], microphone, source, length, 1));
    let direct = distance(source, microphone);
    let image = distance([0.0, 0.0, -1.0], microphone);
    let n = spectrum_size(length);
    let mut worst: f64 = 0.0;
    for hz in [60.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0] {
        let k = (hz * n as f64 / FS).round() as usize;
        let omega = 2.0 * std::f64::consts::PI * k as f64 * FS / n as f64;
        let exact = Complex64::new(1.0, 0.0) + Complex64::from_polar(direct / image, -omega * (image - direct) / C);
        worst = worst.max((h[k] - exact).norm());
    }
    assert!(worst < 0.05, "largest error {worst}");
}

#[test]
fn absorbing_floor_under_a_near_point_source_reflects_nothing() {
    let length = 4096;
    let h = near_spectrum(&from_point(vec![surface(floor(10.0).polygon, [0.0; 8])], [0.2, 0.0, 0.2], [0.0, 0.0, 0.3], length, 1));
    let worst = [20.0, 40.0, 80.0, 160.0, 320.0, 1000.0, 4000.0]
        .iter()
        .map(|&hz| (at(&h, length, hz).norm() - 1.0).abs())
        .fold(0.0, f64::max);
    assert!(worst < 0.05, "largest deviation {worst}");
}

#[test]
fn point_source_in_a_rigid_edge_adds_three_images_at_low_frequency() {
    let length = 8192;
    let wall = surface(rectangle([0.0, 0.0, 10.0], [10.0, 0.0, 0.0], [0.0, 0.0, 10.0]), RIGID);
    let floor = surface(rectangle([0.0, 10.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]), RIGID);
    let (microphone, source) = ([0.3, 0.4, 0.5], [-0.5, 1.0, 0.8]);
    let h = near_spectrum(&from_point(vec![floor, wall], microphone, source, length, 2));
    let direct = distance(source, microphone);
    let images = [[-0.5, 1.0, -0.8], [-0.5, -1.0, 0.8], [-0.5, -1.0, -0.8]];
    let expected = 1.0 + images.iter().map(|&i| direct / distance(i, microphone)).sum::<f64>();
    let low = at(&h, length, 30.0).norm();
    assert!((low / expected - 1.0).abs() < 0.06, "low {low}, expected {expected}");
}

#[test]
fn alignment_delays_and_scales_the_filter() {
    let mut s = scene(vec![], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], 256, 1);
    s.gain = 0.5;
    s.delay_seconds = 48.0 / FS;
    let h = near_filter(&s);
    assert!((h[48] - 0.5).abs() < 1e-4, "{}", h[48]);
    assert!(h.iter().enumerate().all(|(i, x)| i == 48 || x.abs() < 1e-4));
}
