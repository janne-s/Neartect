use neartect_core::filter::{near_filter, near_spectrum, spectrum_size};
use neartect_core::geom::{normalize, Polygon, V3};
use neartect_core::pattern::Pattern;
use neartect_core::scene::{Listener, Scene, Surface};
use rustfft::num_complex::Complex64;

const FS: f64 = 48000.0;
const C: f64 = 343.0;
const RIGID: [f64; 8] = [1.0; 8];

fn rectangle(centre: V3, half_u: V3, half_v: V3) -> Polygon {
    let p = |a: f64, b: f64| [centre[0] + a * half_u[0] + b * half_v[0], centre[1] + a * half_u[1] + b * half_v[1], centre[2] + a * half_u[2] + b * half_v[2]];
    Polygon::new(&[p(-1.0, -1.0), p(1.0, -1.0), p(1.0, 1.0), p(-1.0, 1.0)]).unwrap()
}

fn floor(half: f64) -> Surface {
    surface(rectangle([0.0, 0.0, 0.0], [half, 0.0, 0.0], [0.0, half, 0.0]), RIGID)
}

fn surface(polygon: Polygon, reflection: [f64; 8]) -> Surface {
    Surface { polygon, strength: 1.0, reflection, pattern: Pattern::Solid, relief: neartect_core::relief::Relief::flat(), bend: neartect_core::bend::Bend::flat() }
}

fn scene(surfaces: Vec<Surface>, microphone: V3, arrival: V3, length: usize, max_order: usize) -> Scene {
    Scene {
        surfaces,
        microphone,
        arrival: normalize(arrival),
        speed_of_sound: C,
        sample_rate: FS,
        length,
        max_order,
        pipe: None,
        source: None,
        gain: 1.0,
        delay_seconds: 0.0,
        listener: Listener::omni(),
        quality: Default::default(),
    }
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
        let polygon = rectangle([0.0, 0.0, 0.0], [half, 0.0, 0.0], [0.0, half, 0.0]);
        let depth = neartect_core::edge::mean_depth(&polygon, &Pattern::Solid);
        let h = near_spectrum(&scene(vec![surface(polygon, RIGID)], [0.0, 0.0, height], [angle.sin(), 0.0, angle.cos()], length, 1));
        let to_microphone = height / angle.cos();
        let design = neartect_core::images::design_frequency(C, 2.0 * to_microphone, (2.0 * half).powi(2), angle.cos());
        let hz = design / 4.0;
        let image = 0.25 * neartect_core::edge::response(std::f64::consts::TAU * hz, depth, C).norm();
        let reflected = (at(&h, length, hz) - Complex64::new(1.0, 0.0)).norm();
        assert!((reflected / image - 1.0).abs() < 0.1, "{half} {height} {degrees}: {reflected} vs {image}");
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
        bend: neartect_core::bend::Bend::flat(),
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
    let taps = (2..length - 1).filter(|&i| h[i].abs() > 0.2 * peak && h[i].abs() >= h[i - 1].abs() && h[i].abs() >= h[i + 1].abs()).count();
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
        bend: neartect_core::bend::Bend::flat(),
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
    s.pipe = Some(neartect_core::pipe::PipeInside { mouth_a: [0.0, 0.0, 0.0], mouth_b: [0.0, 1.0, 0.0], radius: 0.05, bend: None });
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
    let worst = [20.0, 40.0, 80.0, 160.0, 320.0, 1000.0, 4000.0].iter().map(|&hz| (at(&h, length, hz).norm() - 1.0).abs()).fold(0.0, f64::max);
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
    s.pipe = Some(neartect_core::pipe::PipeInside { mouth_a: [0.0, 0.0, 0.0], mouth_b: [0.0, 1.0, 0.0], radius: 0.05, bend: None });
    let h = near_spectrum(&aimed(s, 0.0, [1.0, 0.0, 0.0]));
    assert!(h.iter().all(|x| x.norm() < 1e-9));
}

// The reference rolls off below 10 Hz as the model does.
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
    let s = aimed(from_point(vec![floor(60.0)], microphone, source, length, 1), 0.5, facing);
    let h = near_spectrum(&s);
    let direct = distance(source, microphone);
    let image_point = [0.0, 0.0, -1.0];
    let image = distance(image_point, microphone);
    let towards = |p: V3| normalize([p[0] - microphone[0], p[1] - microphone[1], p[2] - microphone[2]]);
    let cos = |p: V3| {
        let d = towards(p);
        d[0] * facing[0] + d[1] * facing[1] + d[2] * facing[2]
    };
    let n = spectrum_size(length);
    let mut worst: f64 = 0.0;
    for hz in [60.0, 125.0, 250.0, 500.0, 1000.0, 2000.0] {
        let bin = (hz * n as f64 / FS).round() as usize;
        let f = bin as f64 * FS / n as f64;
        let k = 2.0 * std::f64::consts::PI * f / C;
        let exact = first_order(0.5, cos(source), k, direct) + first_order(0.5, cos(image_point), k, image) * Complex64::from_polar(direct / image, -k * (image - direct));
        worst = worst.max((h[bin] - exact).norm());
    }
    assert!(worst < 0.03, "largest error {worst}");
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

fn measured(mut s: Scene, outward: V3) -> Scene {
    s.listener = Listener::MeasuredEar { front: [0.0, 1.0, 0.0], outward, up: [0.0, 0.0, 1.0] };
    s
}

#[test]
fn measured_head_hears_a_source_on_the_left_earlier_and_brighter_on_the_left() {
    let length = 2048;
    let free = || scene(vec![], [0.0, 0.0, 1.5], [-1.0, 0.0, 0.0], length, 1);
    let left = near_filter(&measured(free(), [-1.0, 0.0, 0.0]));
    let right = near_filter(&measured(free(), [1.0, 0.0, 0.0]));
    let onset = |h: &[f32]| {
        let peak = h.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
        h.iter().position(|x| x.abs() > 0.3 * peak).unwrap() as f64 / FS
    };
    let itd = onset(&right) - onset(&left);
    assert!(itd > 0.55e-3 && itd < 0.8e-3, "itd {itd}");
    let lh = near_spectrum(&measured(free(), [-1.0, 0.0, 0.0]));
    let rh = near_spectrum(&measured(free(), [1.0, 0.0, 0.0]));
    let band = |h: &[Complex64]| 20.0 * mean_magnitude(h, length, 4000.0, 8000.0).log10();
    assert!(band(&lh) - band(&rh) > 10.0, "level difference {}", band(&lh) - band(&rh));
    assert!(lh.iter().chain(&rh).all(|x| x.re.is_finite() && x.im.is_finite()));
}

#[test]
fn measured_head_is_symmetric_for_a_source_in_front() {
    let length = 2048;
    let front = || scene(vec![], [0.0, 0.0, 1.5], [0.0, 1.0, 0.0], length, 1);
    let left = near_filter(&measured(front(), [-1.0, 0.0, 0.0]));
    let right = near_filter(&measured(front(), [1.0, 0.0, 0.0]));
    assert_eq!(left, right);
}

#[test]
fn measured_ear_over_a_floor_matches_its_image() {
    let length = 4096;
    let (microphone, source) = ([1.0, 0.0, 0.5], [0.0, 0.0, 1.0]);
    let s = measured(from_point(vec![floor(20.0)], microphone, source, length, 1), [-1.0, 0.0, 0.0]);
    let h = near_spectrum(&s);
    let direct = distance(source, microphone);
    let image_point = [0.0, 0.0, -1.0];
    let image = distance(image_point, microphone);
    let towards = |p: V3| normalize([p[0] - microphone[0], p[1] - microphone[1], p[2] - microphone[2]]);
    let n = spectrum_size(length);
    let mut worst: f64 = 0.0;
    for hz in [125.0, 250.0, 500.0, 1000.0, 2000.0] {
        let bin = (hz * n as f64 / FS).round() as usize;
        let omega = 2.0 * std::f64::consts::PI * bin as f64 * FS / n as f64;
        let k = omega / C;
        let exact = s.response(towards(source), direct, omega) + s.response(towards(image_point), image, omega) * Complex64::from_polar(direct / image, -k * (image - direct));
        worst = worst.max((h[bin] - exact).norm());
    }
    assert!(worst < 0.12, "largest error {worst}");
}

#[test]
fn steep_ridges_shade_each_other_at_grazing_incidence() {
    let length = 2048;
    let microphone = [0.0, 0.0, 0.3];
    let grazing = [0.0, 0.97, 0.243];
    let flat = near_spectrum(&scene(vec![floor(3.0)], microphone, grazing, length, 1));
    let ridged = near_spectrum(&scene(vec![relieved(floor(3.0), [2.0, 0.05, 0.1, std::f64::consts::FRAC_PI_2, 0.0, 0.0, 0.05, 1.0])], microphone, grazing, length, 1));
    let reflected = |h: &[Complex64]| mean_magnitude(&h.iter().map(|x| x - Complex64::new(1.0, 0.0)).collect::<Vec<_>>(), length, 3000.0, 8000.0);
    assert!(reflected(&ridged) < 0.6 * reflected(&flat), "ridged {} flat {}", reflected(&ridged), reflected(&flat));
}

#[test]
fn ray_along_a_steep_wave_trough_is_shaded_and_a_steep_ray_is_not() {
    let r = neartect_core::relief::Relief::new(1.0, 0.05, 0.1, [1.0, 0.0], [0.0, 0.0], 0.0, 0.0, 1.0, 0.0);
    let inside = |_: [f64; 2]| true;
    let trough = [0.075, 0.0];
    let (h, _) = r.at(trough);
    assert!(r.shades(trough, h, [1.0, 0.0], 0.2, 1.0, inside));
    assert!(!r.shades(trough, h, [1.0, 0.0], 5.0, 1.0, inside));
    assert!(!r.shades(trough, h, [0.0, 1.0], 0.0, 1.0, inside));
}

#[test]
fn a_point_past_an_edge_mirrors_across_the_nearest_edge() {
    let square = rectangle([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    let inside = square.to_plane([0.6, 0.2, 0.0]);
    let outside = square.to_plane([1.4, 0.2, 0.0]);
    let (mirrored, normal) = square.mirror_across_nearest_edge(outside);
    assert!((mirrored[0] - inside[0]).abs() < 1e-12 && (mirrored[1] - inside[1]).abs() < 1e-12, "{mirrored:?}");
    assert!((normal[0].abs() + normal[1].abs() - 1.0).abs() < 1e-12);
}

#[test]
fn rough_shadowing_is_none_overhead_and_grows_towards_grazing() {
    let r = neartect_core::relief::Relief::new(0.0, 0.0, 1.0, [1.0, 0.0], [0.0, 0.0], 0.0, 0.01, 0.05, 3.0);
    let seen: Vec<f64> = [1.0, 0.7, 0.3, 0.1, 0.02].iter().map(|&c| r.rough_shadowing(c)).collect();
    assert_eq!(seen[0], 1.0);
    assert!(seen.windows(2).all(|w| w[1] <= w[0] + 1e-12), "{seen:?}");
    assert!(seen[4] < 0.5, "{seen:?}");
}

fn wall(half: f64, height: f64) -> Surface {
    surface(rectangle([0.0, 0.0, height], [half, 0.0, 0.0], [0.0, 0.0, height]), RIGID)
}

#[test]
fn second_order_through_a_barely_relieved_wall_matches_the_images() {
    let length = 4096;
    let arrival = [0.3, 1.0, 0.4];
    let microphone = [0.2, 0.6, 0.4];
    let barely = relieved(wall(4.0, 2.0), [1.0, 1e-9, 1.0, 0.0, 0.0, 0.0, 0.05, 1.0]);
    let images = near_spectrum(&scene(vec![floor(4.0), wall(4.0, 2.0)], microphone, arrival, length, 2));
    let summed = near_spectrum(&scene(vec![floor(4.0), barely], microphone, arrival, length, 2));
    let n = spectrum_size(length);
    let (a, b) = ((100.0 * n as f64 / FS) as usize, (3000.0 * n as f64 / FS) as usize);
    let error = (a..=b).map(|k| (images[k] - summed[k]).norm()).sum::<f64>() / (b - a + 1) as f64;
    assert!(error < 0.06, "mean error {error}");
}

fn wide_pipe(microphone: V3, arrival: V3) -> Vec<Complex64> {
    let mut s = scene(vec![], microphone, arrival, 8192, 1);
    s.pipe = Some(neartect_core::pipe::PipeInside { mouth_a: [0.0, 0.0, 0.0], mouth_b: [0.0, 2.0, 0.0], radius: 0.3, bend: None });
    near_spectrum(&s)
}

#[test]
fn bessel_overlaps_vanish_for_a_square_on_wave_and_are_whole_at_the_mode() {
    let (p, _) = neartect_core::pipe::CrossModes::new(0.5, 1.0).at(2000.0, C, 0.3, 2.0, 0.5, 0.0);
    assert!(p.norm() < 1e-4, "{p}");
    assert!((neartect_core::pipe::plane_share(5.0, 0.0) - 1.0).abs() < 1e-12);
    assert!(neartect_core::pipe::plane_share(5.0, 0.8) < 0.5);
}

#[test]
fn wide_pipe_has_cross_modes_off_axis_above_the_first_cutoff_only() {
    let oblique = [0.6, -0.8, 0.0];
    let on_axis = wide_pipe([0.0, 1.0, 0.0], oblique);
    let off_axis = wide_pipe([0.2, 1.0, 0.0], oblique);
    let cutoff = 1.8412 * C / (2.0 * std::f64::consts::PI * 0.3);
    let difference = |lo: f64, hi: f64| {
        let n = spectrum_size(8192);
        let (a, b) = ((lo * n as f64 / FS) as usize, (hi * n as f64 / FS) as usize);
        (a..=b).map(|k| (on_axis[k].norm() - off_axis[k].norm()).abs()).sum::<f64>() / (b - a + 1) as f64
    };
    assert!(difference(50.0, 0.8 * cutoff) < 0.02, "below {}", difference(50.0, 0.8 * cutoff));
    assert!(difference(1.2 * cutoff, 3.0 * cutoff) > 0.2, "above {}", difference(1.2 * cutoff, 3.0 * cutoff));
    assert!(on_axis.iter().chain(&off_axis).all(|x| x.re.is_finite() && x.im.is_finite()));
}

#[test]
fn cardioid_on_or_a_hair_from_a_wall_stays_bounded() {
    let length = 2048;
    for gap in [0.0, 1e-5, 1e-4, 1e-3] {
        let s = aimed(scene(vec![floor(2.0)], [0.0, 0.0, gap], [0.3, 0.0, 1.0], length, 2), 0.5, [1.0, 0.0, 0.0]);
        let h = near_filter(&s);
        let peak = h.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
        let late = h[length / 2..].iter().fold(0.0_f32, |m, x| m.max(x.abs()));
        assert!(peak < 3.0 && late < 0.01, "gap {gap}: peak {peak} late {late}");
    }
}

#[test]
fn relief_takes_no_more_elements_than_the_limit() {
    use neartect_core::kirchhoff::{relief_floor, scatter_into, traversals, Histograms, Part};
    let length = 4096;
    let water = relieved(floor(4.0), [1.0, 0.05, 0.8, 0.0, 0.0, 0.004, 0.1, 0.0]);
    let wall = surface(rectangle([0.0, -1.0, 1.0], [2.0, 0.0, 0.0], [0.0, 0.0, 1.0]), RIGID);
    for limit in [20_000.0, 100_000.0] {
        let mut s = scene(vec![water.clone(), wall.clone()], [0.0, 0.0, 0.5], [0.0, 0.3, 1.0], length, 2);
        s.quality.element_limit = limit;
        let bins = 2 * spectrum_size(length);
        let list = traversals(&s);
        let floor = relief_floor(&s, &list, bins);
        let elements: usize = list
            .iter()
            .enumerate()
            .filter(|(_, &(i, _))| i == 0)
            .filter_map(|(t, &(i, via))| scatter_into(&s, t, i, via, 1.0, floor, Part::WHOLE, &mut Histograms::new(s.sets(), bins)))
            .sum();
        assert!(elements as f64 <= limit * 1.05, "limit {limit}: {elements} elements");
        assert!(elements as f64 >= limit * 0.25, "limit {limit}: {elements} elements");
    }
}

#[test]
fn pruned_image_search_keeps_every_image_of_a_closed_box() {
    let (w, d, h) = (3.1, 2.3, 1.7);
    let walls = vec![
        surface(rectangle([0.0, d / 2.0, h / 2.0], [0.0, d / 2.0, 0.0], [0.0, 0.0, h / 2.0]), RIGID),
        surface(rectangle([w, d / 2.0, h / 2.0], [0.0, d / 2.0, 0.0], [0.0, 0.0, h / 2.0]), RIGID),
        surface(rectangle([w / 2.0, 0.0, h / 2.0], [w / 2.0, 0.0, 0.0], [0.0, 0.0, h / 2.0]), RIGID),
        surface(rectangle([w / 2.0, d, h / 2.0], [w / 2.0, 0.0, 0.0], [0.0, 0.0, h / 2.0]), RIGID),
        surface(rectangle([w / 2.0, d / 2.0, 0.0], [w / 2.0, 0.0, 0.0], [0.0, d / 2.0, 0.0]), RIGID),
        surface(rectangle([w / 2.0, d / 2.0, h], [w / 2.0, 0.0, 0.0], [0.0, d / 2.0, 0.0]), RIGID),
    ];
    let order = 5;
    let s = from_point(walls, [0.71, 0.93, 0.52], [2.27, 1.41, 1.13], 4096, order);
    let paths = neartect_core::images::specular_paths(&s);
    for k in 2..=order {
        let found = paths.iter().filter(|p| p.reflections.len() == k).count();
        assert_eq!(found, 4 * k * k + 2, "order {k}");
    }
}

#[test]
fn parts_sum_to_the_whole_spectrum() {
    use neartect_core::filter::near_spectrum_part;
    use neartect_core::kirchhoff::Part;
    let length = 2048;
    let wall = relieved(surface(rectangle([0.0, -0.4, 1.0], [1.5, 0.0, 0.0], [0.0, 0.0, 1.0]), RIGID), [2.0, 0.03, 0.15, 0.0, 0.0, 0.002, 0.05, 3.0]);
    let mut s = scene(vec![floor(2.0), wall], [0.1, 0.0, 0.6], [0.2, 0.8, 0.4], length, 3);
    s.listener = Listener::MeasuredEar { front: [0.0, 1.0, 0.0], outward: [1.0, 0.0, 0.0], up: [0.0, 0.0, 1.0] };
    let whole = near_spectrum(&s);
    for count in [3, 8] {
        let mut sum = vec![Complex64::new(0.0, 0.0); whole.len()];
        for index in 0..count {
            for (a, b) in sum.iter_mut().zip(near_spectrum_part(&s, Part { index, count })) {
                *a += b;
            }
        }
        let worst = sum.iter().zip(&whole).map(|(a, b)| (a - b).norm()).fold(0.0, f64::max);
        let peak = whole.iter().map(|x| x.norm()).fold(0.0, f64::max);
        assert!(worst < 1e-12 * peak, "{count} parts: {worst}");
    }
}

#[test]
fn a_coincident_pair_hears_what_its_capsules_hear_alone() {
    use neartect_core::filter::near_spectra_part;
    use neartect_core::kirchhoff::Part;
    let length = 2048;
    let wall = relieved(surface(rectangle([0.0, -0.4, 1.0], [1.5, 0.0, 0.0], [0.0, 0.0, 1.0]), RIGID), [2.0, 0.03, 0.15, 0.0, 0.0, 0.0, 0.05, 0.0]);
    let base = from_point(vec![floor(2.0), wall], [0.1, 0.0, 0.6], [0.4, 0.9, 0.8], length, 3);
    let patterns = [0.5, 0.0];
    let facings = [normalize([0.7, 0.7, 0.0]), normalize([-0.7, 0.7, 0.0])];
    let mut piped = base.clone();
    piped.microphone = [0.0, 0.3, 0.6];
    piped.pipe = Some(neartect_core::pipe::PipeInside { mouth_a: [0.0, 0.0, 0.6], mouth_b: [0.0, 1.0, 0.6], radius: 0.05, bend: None });
    for base in [base, piped] {
        let mut pair = base.clone();
        pair.listener = Listener::Pair { patterns, facings };
        let together = near_spectra_part(&pair, Part::WHOLE);
        for capsule in 0..2 {
            let alone = near_spectrum(&aimed(base.clone(), patterns[capsule], facings[capsule]));
            let worst = together[capsule].iter().zip(&alone).map(|(a, b)| (a - b).norm()).fold(0.0, f64::max);
            let peak = alone.iter().map(|x| x.norm()).fold(0.0, f64::max);
            assert!(worst < 1e-12 * peak, "capsule {capsule}: {worst}");
        }
    }
}

fn disc(radius: f64) -> Surface {
    let points: Vec<V3> = (0..96)
        .map(|i| {
            let t = i as f64 / 96.0 * std::f64::consts::TAU;
            [radius * t.cos(), radius * t.sin(), 0.0]
        })
        .collect();
    surface(Polygon::new(&points).unwrap(), RIGID)
}

#[test]
fn a_small_disc_close_to_the_microphone_does_not_double_the_bass() {
    let length = 8192;
    let a = 0.15;
    let h = near_spectrum(&scene(vec![disc(a)], [0.0, 0.0, 0.001], [0.0, 0.0, 1.0], length, 1));
    let hz = 30.0;
    let ka = std::f64::consts::TAU * hz / C * a;
    let exact = Complex64::new(1.0, std::f64::consts::FRAC_2_PI * ka).norm();
    let level = 20.0 * (at(&h, length, hz).norm() / exact).log10();
    assert!(level.abs() < 0.5, "{level} dB from Lamb's limit");
}

#[test]
fn a_small_disc_scatters_rayleighs_dipole_from_afar() {
    let length = 8192;
    let (a, r) = (0.15, 2.0);
    let h = near_spectrum(&scene(vec![disc(a)], [0.0, 0.0, r], [0.0, 0.0, 1.0], length, 1));
    for ka in [0.1, 0.2] {
        let hz = ka * C / (std::f64::consts::TAU * a);
        let k = ka / a;
        let exact = 2.0 / (3.0 * std::f64::consts::PI) * k * k * a * a * a / r;
        let scattered = (at(&h, length, hz) - Complex64::new(1.0, 0.0)).norm();
        assert!((scattered / exact - 1.0).abs() < 0.25, "ka {ka}: {scattered} vs {exact}");
    }
}

#[test]
fn second_order_through_a_barely_relieved_wall_and_a_free_panel_follows_the_images() {
    let length = 4096;
    let arrival = [-0.6, 1.0, 0.3];
    let panel = surface(rectangle([1.0, 1.5, 2.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.5]), RIGID);
    let barely = relieved(wall(4.0, 2.0), [1.0, 1e-9, 1.0, 0.0, 0.0, 0.0, 0.05, 1.0]);
    for (microphone, receiver) in [([0.2, 0.6, 2.0], false), ([0.7, 2.2, 2.0], true)] {
        let with = |surfaces: Vec<Surface>, order| near_spectrum(&scene(surfaces, microphone, arrival, length, order));
        let vias = neartect_core::kirchhoff::via_pairs(&scene(vec![panel.clone(), barely.clone()], microphone, arrival, length, 2));
        assert!(vias.len() == 1 && vias[0].1.receiver == receiver && vias[0].1.cutoff_hz.is_some(), "{vias:?}");
        let images = with(vec![panel.clone(), wall(4.0, 2.0)], 2);
        let summed = with(vec![panel.clone(), barely.clone()], 2);
        let first = with(vec![panel.clone(), wall(4.0, 2.0)], 1);
        let n = spectrum_size(length);
        let (a, b) = ((100.0 * n as f64 / FS) as usize, (3000.0 * n as f64 / FS) as usize);
        let mean = |f: &dyn Fn(usize) -> f64| (a..=b).map(f).sum::<f64>() / (b - a + 1) as f64;
        let error = mean(&|k| (images[k] - summed[k]).norm());
        let second = mean(&|k| (images[k] - first[k]).norm());
        assert!(error < 0.2 * second, "receiver {receiver}: error {error} of {second}");
    }
}

fn bent_wall(radius: f64, concave_towards: V3, half_arc: f64, half_height: f64) -> Surface {
    bent_wall_at([0.0; 3], radius, concave_towards, half_arc, half_height)
}

fn bent_wall_at(centre: V3, radius: f64, concave_towards: V3, half_arc: f64, half_height: f64) -> Surface {
    let polygon = rectangle(centre, [half_arc, 0.0, 0.0], [0.0, 0.0, half_height]);
    let towards = neartect_core::geom::dot(polygon.normal, concave_towards).signum();
    let bend = neartect_core::bend::Bend::new(&polygon, towards / radius, [1.0, 0.0, 0.0], centre, false);
    Surface { bend, ..surface(polygon, RIGID) }
}

fn bowl(radius: f64, concave_up: bool, half_width: f64) -> Surface {
    let flat = disc(half_width);
    let towards = flat.polygon.normal[2].signum() * if concave_up { 1.0 } else { -1.0 };
    let bend = neartect_core::bend::Bend::new(&flat.polygon, towards / radius, [1.0, 0.0, 0.0], [0.0; 3], true);
    Surface { bend, ..flat }
}

fn reflection_amplitude(surfaces: Vec<Surface>, microphone: V3, arrival: V3) -> f64 {
    let length = 4096;
    let h = near_spectrum(&scene(surfaces, microphone, arrival, length, 1));
    let n = spectrum_size(length);
    let (a, b) = ((3000.0 * n as f64 / FS) as usize, (9000.0 * n as f64 / FS) as usize);
    let power = h[a..=b].iter().map(|x| x.norm_sqr()).sum::<f64>() / (b - a + 1) as f64;
    (power - 1.0).max(0.0).sqrt()
}

#[test]
fn a_nearly_flat_bend_reflects_as_the_flat_wall() {
    let microphone = [0.0, 0.6, 0.0];
    let flat = reflection_amplitude(vec![surface(rectangle([0.0; 3], [3.0, 0.0, 0.0], [0.0, 0.0, 3.0]), RIGID)], microphone, [0.0, 1.0, 0.0]);
    let bent = reflection_amplitude(vec![bent_wall(1e5, [0.0, 1.0, 0.0], 3.0, 3.0)], microphone, [0.0, 1.0, 0.0]);
    assert!((flat - bent).abs() < 1e-3, "flat {flat} bent {bent}");
}

#[test]
fn a_convex_wall_spreads_its_reflection_by_the_ray_tube_factor() {
    let radius = 2.0;
    let d = reflection_amplitude(vec![bent_wall(radius, [0.0, -1.0, 0.0], 1.5, 3.0)], [0.0, 1.0, 0.0], [0.0, 1.0, 0.0]);
    let expected = (1.0_f64 / (1.0 + 2.0 * 1.0 / radius)).sqrt();
    assert!((d - expected).abs() < 0.08, "reflection {d}, ray tube {expected}");
}

#[test]
fn a_concave_wall_gathers_its_reflection_by_the_ray_tube_factor() {
    let radius = 2.0;
    let d = reflection_amplitude(vec![bent_wall(radius, [0.0, 1.0, 0.0], 1.5, 3.0)], [0.0, 0.5, 0.0], [0.0, 1.0, 0.0]);
    let expected = (1.0_f64 / (1.0 - 2.0 * 0.5 / radius)).sqrt();
    assert!((d - expected).abs() < 0.12, "reflection {d}, ray tube {expected}");
}

#[test]
fn a_nearly_flat_bowl_reflects_as_the_flat_disc() {
    let microphone = [0.0, 0.0, 0.6];
    let flat = reflection_amplitude(vec![disc(2.0)], microphone, [0.0, 0.0, 1.0]);
    let bent = reflection_amplitude(vec![bowl(1e5, true, 2.0)], microphone, [0.0, 0.0, 1.0]);
    assert!((flat - bent).abs() < 1e-3, "flat {flat} bent {bent}");
}

#[test]
fn a_dome_spreads_its_reflection_in_both_directions() {
    let radius = 2.0;
    let d = reflection_amplitude(vec![bowl(radius, false, 1.5)], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]);
    let expected = 1.0 / (1.0 + 2.0 * 1.0 / radius);
    assert!((d - expected).abs() < 0.08, "reflection {d}, ray tube {expected}");
}

fn ring_integral_on_axis(radius: f64, height: f64, half_width: f64, concave: f64) -> f64 {
    let rings = 1400;
    let step = half_width / rings as f64;
    let k_c = 1.0 / radius;
    let bands: Vec<f64> = (0..400).map(|i| 3000.0 + 6000.0 * i as f64 / 399.0).collect();
    let power: f64 = bands
        .iter()
        .map(|&hz| {
            let k = std::f64::consts::TAU * hz / C;
            let scattered: Complex64 = (0..rings)
                .map(|i| {
                    let theta = (i as f64 + 0.5) * step * k_c;
                    let (r, h) = (theta.sin() / k_c, concave * (1.0 - theta.cos()) / k_c);
                    let area = std::f64::consts::TAU * theta.sin() / k_c * step;
                    let (dx, dz) = (-r, height - h);
                    let d = dx.hypot(dz);
                    let cosine = (-concave * theta.sin() * dx + theta.cos() * dz) / d;
                    let incident = Complex64::from_polar(1.0, k * (height - h));
                    -2.0 * incident * (Complex64::new(-1.0 / d, k)) * Complex64::from_polar(1.0 / d, k * d) * cosine * area / (4.0 * std::f64::consts::PI)
                })
                .sum();
            (Complex64::new(1.0, 0.0) + scattered).norm_sqr()
        })
        .sum::<f64>()
        / bands.len() as f64;
    (power - 1.0).max(0.0).sqrt()
}

#[test]
fn a_bowl_and_a_dome_on_their_axis_match_the_ring_integral() {
    for (radius, height, half_width, concave) in [(2.0, 0.5, 1.5, true), (2.0, 0.25, 0.8, true), (2.0, 1.0, 1.5, false)] {
        let d = reflection_amplitude(vec![bowl(radius, concave, half_width)], [0.0, 0.0, height], [0.0, 0.0, 1.0]);
        let exact = ring_integral_on_axis(radius, height, half_width, if concave { 1.0 } else { -1.0 });
        assert!((d / exact - 1.0).abs() < 0.05, "R {radius} z {height}: reflection {d}, ring integral {exact}");
    }
}

#[test]
fn a_second_order_path_over_a_dome_and_a_floor_finds_a_true_reflection_point() {
    let radius = 2.0;
    let mut dome = bowl(radius, false, 1.2);
    let tilt = |p: V3| [p[0], p[2], 3.0 + p[1]];
    let points: Vec<V3> = dome.polygon.points.iter().map(|&q| tilt(dome.polygon.to_space(q))).collect();
    dome.polygon = Polygon::new(&points).unwrap();
    let towards = dome.polygon.normal[1].signum();
    dome.bend = neartect_core::bend::Bend::new(&dome.polygon, -towards / radius, [1.0, 0.0, 0.0], [0.0, 0.0, 3.0], true);
    let microphone = [0.3, 1.0, 1.2];
    let arrival = normalize([0.2, 1.0, 0.5]);
    let s = scene(vec![dome.clone(), floor(10.0)], microphone, arrival, 2048, 2);
    let order = |paths: Vec<neartect_core::images::Path>| {
        let mut paths: Vec<_> = paths.into_iter().map(|p| (p.reflections.iter().map(|r| r.surface).collect::<Vec<_>>(), p.gain, p.delay_seconds)).collect();
        paths.sort_by(|a, b| a.0.cmp(&b.0));
        paths
    };
    let bent = order(neartect_core::images::specular_paths(&s));
    let wide = disc(3.0);
    let points: Vec<V3> = wide.polygon.points.iter().map(|&q| tilt(wide.polygon.to_space(q))).collect();
    let flat = surface(Polygon::new(&points).unwrap(), RIGID);
    let flat = order(neartect_core::images::specular_paths(&scene(vec![flat, floor(10.0)], microphone, arrival, 2048, 2)));
    assert!(bent.iter().all(|p| p.1 > 0.2 && p.1 < 1.0), "{bent:?}");
    let first_dome = |paths: &[(Vec<usize>, f64, f64)]| paths.iter().find(|p| p.0 == [0, 1]).map(|p| p.2);
    let (dome_delay, flat_delay) = (first_dome(&bent).unwrap(), first_dome(&flat).unwrap());
    assert!(dome_delay > flat_delay, "dome {dome_delay} flat {flat_delay}");
}

#[test]
fn a_second_order_path_over_a_floor_keeps_the_bend_factor() {
    let radius = 2.0;
    let wall = bent_wall_at([0.0, 0.0, 3.0], radius, [0.0, -1.0, 0.0], 1.5, 3.0);
    let floor = floor(10.0);
    let arrival = normalize([0.0, 1.0, 0.5]);
    let s = scene(vec![wall, floor], [0.0, 1.0, 1.0], arrival, 2048, 2);
    let paths = neartect_core::images::specular_paths(&s);
    let expected = (1.0_f64 / (1.0 + 2.0 * 1.0 / radius)).sqrt();
    assert_eq!(paths.len(), 1, "paths {}", paths.len());
    assert_eq!(paths[0].reflections.iter().map(|r| r.surface).collect::<Vec<_>>(), [0, 1]);
    assert!((paths[0].gain - expected).abs() < 1e-6, "gain {}", paths[0].gain);
}

#[test]
fn a_bend_speeds_the_long_wave_by_the_slice_model() {
    use neartect_core::pipe::bend_speed_ratio;
    let small = bend_speed_ratio(0.1);
    assert!((small - (1.0 + 0.01 / 8.0)).abs() < 1e-4, "small bend {small}");
    let slide = bend_speed_ratio(9.25 / 12.7);
    assert!(slide > 1.085 && slide < 1.095, "tuning slide {slide}");
}

fn bent_pipe(turn: f64, length: f64, radius: f64) -> neartect_core::pipe::PipeInside {
    let bend_radius = length / turn;
    neartect_core::pipe::PipeInside {
        mouth_a: [bend_radius, 0.0, 0.0],
        mouth_b: [bend_radius * turn.cos(), bend_radius * turn.sin(), 0.0],
        radius,
        bend: Some(neartect_core::pipe::PipeBend { centre: [0.0, 0.0, 0.0], axis: [0.0, 0.0, 1.0] }),
    }
}

#[test]
fn a_bent_pipe_measures_along_its_centreline() {
    let pipe = bent_pipe(std::f64::consts::FRAC_PI_2, 1.0, 0.05);
    assert!((pipe.length() - 1.0).abs() < 1e-12);
    let middle = pipe.centre_point(0.5);
    assert!((pipe.along(middle) - 0.5).abs() < 1e-12);
    let start = pipe.tangent(0.0);
    assert!((start[1] - 1.0).abs() < 1e-12);
}

fn bent_inside(turn: f64, radius: f64) -> Vec<Complex64> {
    let pipe = bent_pipe(turn, 1.0, radius);
    let mut s = scene(vec![], pipe.centre_point(0.5), [1.0, 0.0, 0.0], 8192, 1);
    s.pipe = Some(pipe);
    near_spectrum(&s)
}

fn resonance(h: &[Complex64]) -> f64 {
    let n = spectrum_size(8192);
    let (a, b) = ((100.0 * n as f64 / FS) as usize, (250.0 * n as f64 / FS) as usize);
    let n_fine = 64;
    let magnitude = |x: f64| {
        let i = x.floor() as usize;
        let f = x - i as f64;
        h[i].norm() * (1.0 - f) + h[i + 1].norm() * f
    };
    let mut best = (0.0, a as f64);
    for step in 0..(b - a) * n_fine {
        let x = a as f64 + step as f64 / n_fine as f64;
        if magnitude(x) > best.0 {
            best = (magnitude(x), x);
        }
    }
    best.1 * FS / n as f64
}

#[test]
fn a_nearly_straight_bend_resonates_as_the_straight_pipe() {
    let mut s = scene(vec![], [0.0, 0.5, 0.0], [1.0, 0.0, 0.0], 8192, 1);
    s.pipe = Some(neartect_core::pipe::PipeInside { mouth_a: [0.0, 0.0, 0.0], mouth_b: [0.0, 1.0, 0.0], radius: 0.05, bend: None });
    let straight = resonance(&near_spectrum(&s));
    let bent = resonance(&bent_inside(1e-3, 0.05));
    assert!((bent / straight - 1.0).abs() < 2e-3, "straight {straight} Hz, bent {bent} Hz");
}

#[test]
fn a_sharp_bend_raises_the_resonance_by_at_most_its_long_wave_speed() {
    let turn = std::f64::consts::PI;
    let radius = 0.2;
    let mut s = scene(vec![], [0.0, 0.5, 0.0], [1.0, 0.0, 0.0], 8192, 1);
    s.pipe = Some(neartect_core::pipe::PipeInside { mouth_a: [0.0, 0.0, 0.0], mouth_b: [0.0, 1.0, 0.0], radius, bend: None });
    let straight = resonance(&near_spectrum(&s));
    let bent = resonance(&bent_inside(turn, radius));
    let most = neartect_core::pipe::bend_speed_ratio(radius / (1.0 / turn));
    let rise = bent / straight;
    assert!(rise > 1.002 && rise < most + 0.005, "rise {rise}, long-wave speed {most}");
}
