pub mod filter;
pub mod geom;
pub mod images;
pub mod kirchhoff;
pub mod pattern;
pub mod scene;
pub mod spectrum;

use geom::{normalize, Polygon};
use pattern::Pattern;
use scene::{Scene, Surface};
use spectrum::BAND_COUNT;
use wasm_bindgen::prelude::*;

pub const PATTERN_VALUES: usize = 6;
const SURFACE_HEADER: usize = 3 + BAND_COUNT + 1 + PATTERN_VALUES;

pub fn read_scene(settings: &[f64], surfaces: &[f64]) -> Result<Scene, String> {
    if settings.len() < 10 {
        return Err("settings need 10 numbers".into());
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
        let vertex_count = r[SURFACE_HEADER - 1] as usize;
        if SURFACE_HEADER + vertex_count * 3 != record {
            return Err(format!("surface record at {at} has the wrong length"));
        }
        let vertices: Vec<[f64; 3]> = r[SURFACE_HEADER..].chunks(3).map(|v| [v[0], v[1], v[2]]).collect();
        if let Some(polygon) = Polygon::new(&vertices) {
            list.push(Surface { polygon, strength, reflection, pattern });
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
    })
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
