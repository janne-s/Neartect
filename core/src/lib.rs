pub mod filter;
pub mod geom;
pub mod hrtf;
pub mod images;
pub mod kirchhoff;
pub mod pattern;
pub mod pipe;
pub mod relief;
pub mod scene;
pub mod spectrum;

use geom::{normalize, Polygon};
use pattern::Pattern;
use scene::{Scene, Surface};
use spectrum::BAND_COUNT;
use wasm_bindgen::prelude::*;

pub const PATTERN_VALUES: usize = 6;
// Relief: shape, height, period, direction xyz, anchor xyz, offset, rough
// height, grain, seed.
pub const RELIEF_VALUES: usize = 13;
// Settings: rate, speed of sound, length, order, microphone xyz, arrival xyz,
// point source flag and xyz, gain, delay seconds, pipe radius (0 = none),
// pipe mouths xyz, pickup pattern (1 omni … 0 figure of eight), facing xyz
// (an ear's outward axis), listener kind (0 microphone, 1 ear), head radius,
// relief detail, element limit, head front xyz and up xyz (kind 2, an ear on
// the measured head).
pub const SETTINGS_LENGTH: usize = 37;
const MOST_COARSE_RELIEF_DETAIL: f64 = 1.0;
const MOST_FINE_RELIEF_DETAIL: f64 = 24.0;
const FEWEST_ELEMENTS: f64 = 1000.0;
const SURFACE_HEADER: usize = 3 + BAND_COUNT + 1 + PATTERN_VALUES + RELIEF_VALUES;

pub fn read_scene(settings: &[f64], surfaces: &[f64]) -> Result<Scene, String> {
    if settings.len() < SETTINGS_LENGTH {
        return Err(format!("settings need {SETTINGS_LENGTH} numbers"));
    }
    let mut list = Vec::new();
    let mut at = 0;
    while at < surfaces.len() {
        let record = surfaces[at] as usize;
        if record < SURFACE_HEADER || at + record > surfaces.len() {
            return Err(format!("broken surface record at {at}"));
        }
        let r = &surfaces[at..at + record];
        let strength = r[1];
        let mut reflection = [0.0; BAND_COUNT];
        reflection.copy_from_slice(&r[2..2 + BAND_COUNT]);
        let pattern_at = 2 + BAND_COUNT;
        let pattern = Pattern::from_numbers(r[pattern_at], &r[pattern_at + 1..pattern_at + 1 + PATTERN_VALUES]);
        let relief_at = pattern_at + 1 + PATTERN_VALUES;
        let relief_values = &r[relief_at..relief_at + RELIEF_VALUES];
        let vertex_count = r[SURFACE_HEADER - 1] as usize;
        if SURFACE_HEADER + vertex_count * 3 != record {
            return Err(format!("surface record at {at} has the wrong length"));
        }
        let vertices: Vec<[f64; 3]> = r[SURFACE_HEADER..].chunks(3).map(|v| [v[0], v[1], v[2]]).collect();
        if let Some(polygon) = Polygon::new(&vertices) {
            let relief = surface_relief(&polygon, relief_values);
            list.push(Surface { polygon, strength, reflection, pattern, relief });
        }
        at += record;
    }
    Ok(Scene {
        surfaces: list,
        sample_rate: settings[0],
        speed_of_sound: settings[1],
        length: settings[2] as usize,
        max_order: settings[3] as usize,
        microphone: [settings[4], settings[5], settings[6]],
        arrival: normalize([settings[7], settings[8], settings[9]]),
        source: (settings[10] > 0.0).then(|| [settings[11], settings[12], settings[13]]),
        gain: settings[14],
        delay_seconds: settings[15],
        pipe: (settings[16] > 0.0).then(|| pipe::PipeInside {
            radius: settings[16],
            mouth_a: [settings[17], settings[18], settings[19]],
            mouth_b: [settings[20], settings[21], settings[22]],
        }),
        listener: if settings[27] > 1.5 {
            scene::Listener::MeasuredEar {
                front: normalize([settings[31], settings[32], settings[33]]),
                outward: normalize([settings[24], settings[25], settings[26]]),
                up: normalize([settings[34], settings[35], settings[36]]),
            }
        } else if settings[27] > 0.5 {
            scene::Listener::Ear { axis: normalize([settings[24], settings[25], settings[26]]), radius: settings[28] }
        } else {
            scene::Listener::Microphone { pattern: settings[23], facing: normalize([settings[24], settings[25], settings[26]]) }
        },
        quality: scene::Quality {
            relief_detail: settings[29].clamp(MOST_COARSE_RELIEF_DETAIL, MOST_FINE_RELIEF_DETAIL),
            element_limit: settings[30].max(FEWEST_ELEMENTS),
        },
    })
}

fn surface_relief(polygon: &Polygon, v: &[f64]) -> relief::Relief {
    use geom::{dot, norm, scale, sub};
    let n = polygon.normal;
    let wanted = [v[3], v[4], v[5]];
    let flat = sub(wanted, scale(n, dot(wanted, n)));
    let along = if norm(flat) > 1e-6 {
        normalize(flat)
    } else {
        let level = geom::cross(n, [0.0, 0.0, 1.0]);
        if norm(level) > 1e-6 { normalize(level) } else { [1.0, 0.0, 0.0] }
    };
    let across = geom::cross(n, along);
    let anchor = [v[6], v[7], v[8]];
    let from_anchor = sub(polygon.origin, anchor);
    relief::Relief::new(
        v[0],
        v[1],
        v[2],
        [dot(along, polygon.u), dot(along, polygon.v)],
        [dot(from_anchor, along), dot(from_anchor, across)],
        v[9],
        v[10],
        v[11],
        v[12],
    )
}

#[wasm_bindgen(js_name = nearFilter)]
pub fn near_filter(settings: &[f64], surfaces: &[f64]) -> Result<Vec<f32>, JsError> {
    let scene = read_scene(settings, surfaces).map_err(|e| JsError::new(&e))?;
    Ok(filter::near_filter(&scene))
}

#[wasm_bindgen(js_name = coreVersion)]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").into()
}

#[wasm_bindgen(js_name = surfaceHeaderLength)]
pub fn surface_header_length() -> usize {
    SURFACE_HEADER
}
