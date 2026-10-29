use neartect_core::filter::{near_filter, near_spectrum, spectrum_size};
use neartect_core::geom::{normalize, Polygon, V3};
use neartect_core::pattern::Pattern;
use neartect_core::scene::{Listener, Scene, Surface};
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
    Surface { polygon, strength: 1.0, reflection, pattern: Pattern::Solid, relief: neartect_core::relief::Relief::flat() }
}

fn scene(surfaces: Vec<Surface>, microphone: V3, arrival: V3, length: usize, max_order: usize) -> Scene {
    Scene { surfaces, microphone, arrival: normalize(arrival), speed_of_sound: C, sample_rate: FS, length, max_order, pipe: None, source: None, gain: 1.0, delay_seconds: 0.0, listener: Listener::omni(), quality: Default::default() }
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
fn image_design_frequency_follows_a_small_panels_kirchhoff_slope() {
    let length = 8192;
    for (half, height, degrees) in [(0.15, 0.5, 0.0), (0.5, 1.0, 0.0), (0.15, 0.5, 45.0)] {
        let angle = f64::to_radians(degrees);
        let panel = surface(rectangle([0.0, 0.0, 0.0], [half, 0.0, 0.0], [0.0, half, 0.0]), RIGID);
        let h = near_spectrum(&scene(vec![panel], [0.0, 0.0, height], [angle.sin(), 0.0, angle.cos()], length, 1));
        let to_microphone = height / angle.cos();
        let design = neartect_core::images::design_frequency(C, 2.0 * to_microphone, (2.0 * half).powi(2), angle.cos());
        let hz = design / 4.0;
        let reflected = (at(&h, length, hz) - Complex64::new(1.0, 0.0)).norm();
        assert!((reflected / 0.25 - 1.0).abs() < 0.1, "{half} {height} {degrees}: {reflected}");
    }
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
        relief: neartect_core::relief::Relief::flat(),
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
        relief: neartect_core::relief::Relief::flat(),
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

fn aimed(mut s: Scene, pattern: f64, facing: V3) -> Scene {
    s.listener = Listener::Microphone { pattern, facing: normalize(facing) };
    s
}

#[test]
fn cardioid_hears_its_front_fully_and_its_rear_not_at_all() {
    let front = near_filter(&aimed(scene(vec![], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0], 64, 1), 0.5, [0.0, 1.0, 0.0]));
    let rear = near_filter(&aimed(scene(vec![], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0], 64, 1), 0.5, [0.0, 1.0, 0.0]));
    let side = near_filter(&aimed(scene(vec![], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0], 64, 1), 0.5, [0.0, 1.0, 0.0]));
    assert!((front[0] - 1.0).abs() < 1e-6 && rear[0].abs() < 1e-6 && (side[0] - 0.5).abs() < 1e-6);
}

#[test]
fn figure_eight_inverts_its_rear_lobe() {
    let rear = near_filter(&aimed(scene(vec![], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0], 64, 1), 0.0, [0.0, 1.0, 0.0]));
    assert!((rear[0] + 1.0).abs() < 1e-6);
}

#[test]
fn cardioid_facing_away_from_a_floor_hears_less_of_its_reflection() {
    let length = 4096;
    let base = || scene(vec![floor(20.0)], [0.0, 0.0, 0.5], [0.0, 0.0, 1.0], length, 1);
    let up = near_spectrum(&aimed(base(), 0.5, [0.0, 0.0, 1.0]));
    let omni = near_spectrum(&base());
    let depth = |h: &[Complex64]| at(h, length, C / 1.0).norm() - at(h, length, C / 2.0).norm();
    assert!(depth(&omni) > 1.6, "omni comb {}", depth(&omni));
    assert!(depth(&up) < 0.3, "cardioid comb {}", depth(&up));
    let sideways = near_spectrum(&aimed(base(), 0.5, [1.0, 0.0, 0.0]));
    assert!(depth(&sideways) > 0.6 && depth(&sideways) < 1.4, "sideways comb {}", depth(&sideways));
}

#[test]
fn figure_eight_across_a_pipe_hears_nothing_inside_it() {
    let mut s = scene(vec![], [0.0, 0.5, 0.0], [1.0, 0.0, 0.0], 8192, 1);
    s.pipe = Some(neartect_core::pipe::PipeInside { mouth_a: [0.0, 0.0, 0.0], mouth_b: [0.0, 1.0, 0.0], radius: 0.05 });
    let h = near_spectrum(&aimed(s, 0.0, [1.0, 0.0, 0.0]));
    assert!(h.iter().all(|x| x.norm() < 1e-9));
}

// The proximity term rolls off below 10 Hz in the model (a microphone's own
// low cut), so the reference does too.
fn first_order(pattern: f64, cos: f64, k: f64, r: f64) -> Complex64 {
    let floor = 2.0 * std::f64::consts::PI * 10.0 / C;
    Complex64::new(pattern, 0.0) + (1.0 - pattern) * cos * (Complex64::new(1.0, 0.0) + Complex64::new(1.0 / r, 0.0) / Complex64::new(floor, k))
}

#[test]
fn cardioid_close_to_a_point_source_lifts_its_bass() {
    let length = 4096;
    let r = 0.05;
    let s = aimed(from_point(vec![], [0.0, 0.0, 1.0], [0.0, r, 1.0], length, 1), 0.5, [0.0, 1.0, 0.0]);
    let h = near_spectrum(&s);
    for hz in [100.0, 300.0, 1000.0, 5000.0] {
        let k = 2.0 * std::f64::consts::PI * hz / C;
        let exact = first_order(0.5, 1.0, k, r).norm();
        let got = at(&h, length, hz).norm();
        assert!((got / exact - 1.0).abs() < 0.05, "{hz} Hz: {got} against {exact}");
    }
    assert!(at(&h, length, 100.0).norm() > 3.0);
}

#[test]
fn cardioid_over_a_floor_matches_its_image_with_proximity() {
    let length = 4096;
    let (microphone, source) = ([1.0, 0.0, 0.5], [0.0, 0.0, 1.0]);
    let facing = normalize([-1.0, 0.0, 0.0]);
    let s = aimed(from_point(vec![floor(20.0)], microphone, source, length, 1), 0.5, facing);
    let h = near_spectrum(&s);
    let direct = distance(source, microphone);
    let image_point = [0.0, 0.0, -1.0];
    let image = distance(image_point, microphone);
    let towards = |p: V3| normalize([p[0] - microphone[0], p[1] - microphone[1], p[2] - microphone[2]]);
    let cos = |p: V3| { let d = towards(p); d[0] * facing[0] + d[1] * facing[1] + d[2] * facing[2] };
    let n = spectrum_size(length);
    let mut worst: f64 = 0.0;
    for hz in [60.0, 125.0, 250.0, 500.0, 1000.0, 2000.0] {
        let bin = (hz * n as f64 / FS).round() as usize;
        let f = bin as f64 * FS / n as f64;
        let k = 2.0 * std::f64::consts::PI * f / C;
        let exact = first_order(0.5, cos(source), k, direct)
            + first_order(0.5, cos(image_point), k, image) * Complex64::from_polar(direct / image, -k * (image - direct));
        worst = worst.max((h[bin] - exact).norm());
    }
    // Each surface element's gradient is taken as a spherical wave's; a rigid
    // element is a dipole, which leaves about 0.12 at 60 Hz here (class B).
    assert!(worst < 0.13, "largest error {worst}");
}

#[test]
fn ears_hear_a_source_on_the_left_earlier_and_brighter_on_the_left() {
    let length = 2048;
    let ear = |axis: V3| {
        let mut s = scene(vec![], [0.0, 0.0, 1.5], [-1.0, 0.0, 0.0], length, 1);
        s.listener = Listener::Ear { axis, radius: 0.0875 };
        s
    };
    let left = near_filter(&ear([-1.0, 0.0, 0.0]));
    let right = near_filter(&ear([1.0, 0.0, 0.0]));
    let peak = |h: &[f32]| h.iter().enumerate().max_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).unwrap()).unwrap().0 as f64;
    let itd = (peak(&right) - peak(&left)) / FS;
    let expected = 0.0875 / C * (1.0 + std::f64::consts::FRAC_PI_2);
    assert!((itd - expected).abs() < 2.0 / FS, "itd {itd} against {expected}");
    let lh = near_spectrum(&ear([-1.0, 0.0, 0.0]));
    let rh = near_spectrum(&ear([1.0, 0.0, 0.0]));
    let level = |h: &[Complex64], hz: f64| 20.0 * at(h, length, hz).norm().log10();
    assert!(level(&lh, 100.0).abs() < 0.5 && level(&rh, 100.0).abs() < 0.5);
    assert!(level(&lh, 10000.0) > 5.0, "left {}", level(&lh, 10000.0));
    assert!(level(&rh, 10000.0) < -8.0, "right {}", level(&rh, 10000.0));
}

fn relieved(mut s: Surface, values: [f64; 8]) -> Surface {
    s.relief = neartect_core::relief::Relief::from_numbers(&values);
    s
}

#[test]
fn rough_floor_keeps_the_coherent_reflection_eckart_predicts() {
    let length = 4096;
    let sigma = 0.01;
    let n = spectrum_size(length);
    let microphone = [0.0, 0.0, 0.5];
    let flat = near_spectrum(&scene(vec![floor(3.0)], microphone, [0.0, 0.0, 1.0], length, 1));
    let seeds = 12;
    let mut mean = vec![Complex64::new(0.0, 0.0); flat.len()];
    for seed in 0..seeds {
        let rough = relieved(floor(3.0), [0.0, 0.0, 1.0, 0.0, 0.0, sigma, 0.05, seed as f64]);
        let h = near_spectrum(&scene(vec![rough], microphone, [0.0, 0.0, 1.0], length, 1));
        for (m, x) in mean.iter_mut().zip(&h) {
            *m += (x - Complex64::new(1.0, 0.0)) / seeds as f64;
        }
    }
    for (lo, hi) in [(300.0, 600.0), (1500.0, 2500.0), (3500.0, 4500.0)] {
        let (a, b) = ((lo * n as f64 / FS) as usize, (hi * n as f64 / FS) as usize);
        let ratio = (a..=b).map(|i| mean[i].norm()).sum::<f64>() / (a..=b).map(|i| (flat[i] - Complex64::new(1.0, 0.0)).norm()).sum::<f64>();
        let k = 2.0 * std::f64::consts::PI * (lo + hi) / 2.0 / C;
        let eckart = (-2.0 * (k * sigma).powi(2)).exp();
        assert!((ratio - eckart).abs() < 0.12, "{lo}–{hi} Hz: coherent {ratio}, Eckart {eckart}");
    }
}

#[test]
fn moving_the_relief_changes_the_filter_and_flat_relief_changes_nothing() {
    let length = 2048;
    let waves = |offset: f64| relieved(floor(2.0), [1.0, 0.02, 0.2, 0.0, offset, 0.0, 1.0, 0.0]);
    let a = near_filter(&scene(vec![waves(0.0)], [0.0, 0.0, 0.3], [0.3, 0.0, 1.0], length, 1));
    let b = near_filter(&scene(vec![waves(0.05)], [0.0, 0.0, 0.3], [0.3, 0.0, 1.0], length, 1));
    assert!(a.iter().zip(&b).map(|(x, y)| (x - y).abs()).sum::<f32>() > 0.01);
    let zero = relieved(floor(2.0), [1.0, 0.0, 0.2, 0.0, 0.0, 0.0, 1.0, 0.0]);
    let flat = near_filter(&scene(vec![zero], [0.0, 0.0, 0.3], [0.3, 0.0, 1.0], length, 1));
    let plain = near_filter(&scene(vec![floor(2.0)], [0.0, 0.0, 0.3], [0.3, 0.0, 1.0], length, 1));
    assert!(flat.iter().zip(&plain).all(|(x, y)| (x - y).abs() < 1e-6));
}

#[test]
fn corrugated_floor_sends_a_grating_lobe() {
    let length = 4096;
    let period = 0.1;
    let waves = relieved(floor(3.0), [1.0, 0.01, period, 0.0, 0.0, 0.0, 1.0, 0.0]);
    let microphone = [0.0, 0.0, 1.0];
    let h = near_spectrum(&scene(vec![waves], microphone, [0.0, 0.0, 1.0], length, 1));
    let flat = near_spectrum(&scene(vec![floor(3.0)], microphone, [0.0, 0.0, 1.0], length, 1));
    let spread = |h: &[Complex64]| mean_magnitude(h, length, 4000.0, 12000.0);
    let differs: f64 = {
        let n = spectrum_size(length);
        let (a, b) = ((4000.0 * n as f64 / FS) as usize, (12000.0 * n as f64 / FS) as usize);
        (a..=b).map(|i| (h[i] - flat[i]).norm()).sum::<f64>() / (b - a + 1) as f64
    };
    assert!(differs > 0.2, "corrugation changes the treble by only {differs}");
    assert!(spread(&h).is_finite());
}

#[test]
fn element_limit_coarsens_relief_and_leaves_flat_surfaces_alone() {
    let length = 2048;
    let microphone = [0.0, 0.0, 0.5];
    let coarse = neartect_core::scene::Quality { relief_detail: 6.0, element_limit: 1000.0 };
    let flat = scene(vec![floor(3.0)], microphone, [0.0, 0.0, 1.0], length, 1);
    let mut flat_coarse = flat.clone();
    flat_coarse.quality = coarse;
    assert_eq!(near_filter(&flat), near_filter(&flat_coarse));
    let rough = scene(vec![relieved(floor(3.0), [0.0, 0.0, 1.0, 0.0, 0.0, 0.01, 0.05, 1.0])], microphone, [0.0, 0.0, 1.0], length, 1);
    let mut rough_coarse = rough.clone();
    rough_coarse.quality = coarse;
    let (fine, limited) = (near_filter(&rough), near_filter(&rough_coarse));
    assert_ne!(fine, limited);
    assert!(limited.iter().all(|x| x.is_finite()));
}
