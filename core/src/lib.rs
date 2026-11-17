pub mod bend;
pub mod edge;
pub mod filter;
pub mod geom;
pub mod hrtf;
pub mod images;
pub mod jobs;
pub mod kirchhoff;
pub mod layout;
pub mod pattern;
pub mod pipe;
pub mod relief;
pub mod scene;
pub mod space;
pub mod spectrum;
pub mod trig;

use geom::{normalize, Polygon};
use pattern::Pattern;
use scene::{Scene, Surface};
use spectrum::BAND_COUNT;
use wasm_bindgen::prelude::*;

pub const PATTERN_VALUES: usize = 6;
// Relief: shape, height, period, direction xyz, anchor xyz, offset, rough
// height, grain, seed.
pub const RELIEF_VALUES: usize = 13;
// Curvature (1/m), the arc's direction, a point where it touches the plane, and 1 for a sphere.
pub const BEND_VALUES: usize = 8;
// listenerKind: 0 microphone, 1 ear on the sphere, 2 ear on the measured head,
// 3 two capsules at one point. A pipe radius of 0 means no pipe.
pub const SETTINGS: [&str; 63] = [
    "sampleRateHz",
    "speedOfSoundMetresPerSecond",
    "lengthSamples",
    "order",
    "microphoneX",
    "microphoneY",
    "microphoneZ",
    "arrivalX",
    "arrivalY",
    "arrivalZ",
    "pointSource",
    "sourceX",
    "sourceY",
    "sourceZ",
    "gain",
    "delaySeconds",
    "pipeRadiusMetres",
    "pipeMouthAX",
    "pipeMouthAY",
    "pipeMouthAZ",
    "pipeMouthBX",
    "pipeMouthBY",
    "pipeMouthBZ",
    "pipeBendCentreX",
    "pipeBendCentreY",
    "pipeBendCentreZ",
    "pipeBendAxisX",
    "pipeBendAxisY",
    "pipeBendAxisZ",
    "pattern",
    "facingX",
    "facingY",
    "facingZ",
    "listenerKind",
    "headRadiusMetres",
    "reliefDetail",
    "elementLimit",
    "headFrontX",
    "headFrontY",
    "headFrontZ",
    "headUpX",
    "headUpY",
    "headUpZ",
    "secondPattern",
    "secondFacingX",
    "secondFacingY",
    "secondFacingZ",
    "pipeReflection63Hz",
    "pipeReflection125Hz",
    "pipeReflection250Hz",
    "pipeReflection500Hz",
    "pipeReflection1000Hz",
    "pipeReflection2000Hz",
    "pipeReflection4000Hz",
    "pipeReflection8000Hz",
    "pipeReliefShape",
    "pipeReliefHeightMetres",
    "pipeReliefPeriodMetres",
    "pipeReliefRadians",
    "pipeReliefOffsetMetres",
    "pipeRoughMetres",
    "pipeGrainMetres",
    "pipeReliefSeed",
];
const PIPE_REFLECTIONS: [&str; BAND_COUNT] = [
    "pipeReflection63Hz",
    "pipeReflection125Hz",
    "pipeReflection250Hz",
    "pipeReflection500Hz",
    "pipeReflection1000Hz",
    "pipeReflection2000Hz",
    "pipeReflection4000Hz",
    "pipeReflection8000Hz",
];
const MOST_COARSE_RELIEF_DETAIL: f64 = 1.0;
const MOST_FINE_RELIEF_DETAIL: f64 = 24.0;
const FEWEST_ELEMENTS: f64 = 1000.0;
const SURFACE_HEADER: usize = 3 + BAND_COUNT + 1 + PATTERN_VALUES + RELIEF_VALUES + BEND_VALUES;

pub fn read_scene(settings: &[f64], surfaces: &[f64]) -> Result<Scene, String> {
    if settings.len() != SETTINGS.len() {
        return Err(format!("settings need {} numbers", SETTINGS.len()));
    }
    let get = |name: &str| settings[SETTINGS.iter().position(|&n| n == name).expect("a named setting")];
    let vector = |name: &str| [get(&format!("{name}X")), get(&format!("{name}Y")), get(&format!("{name}Z"))];
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
        let bend_values = &r[relief_at + RELIEF_VALUES..relief_at + RELIEF_VALUES + BEND_VALUES];
        let vertex_count = r[SURFACE_HEADER - 1] as usize;
        if SURFACE_HEADER + vertex_count * 3 != record {
            return Err(format!("surface record at {at} has the wrong length"));
        }
        let vertices: Vec<[f64; 3]> = r[SURFACE_HEADER..].chunks(3).map(|v| [v[0], v[1], v[2]]).collect();
        if let Some(polygon) = Polygon::new(&vertices) {
            let relief = surface_relief(&polygon, relief_values);
            let bend =
                bend::Bend::new(&polygon, bend_values[0], [bend_values[1], bend_values[2], bend_values[3]], [bend_values[4], bend_values[5], bend_values[6]], bend_values[7] > 0.5);
            list.push(Surface { polygon, strength, reflection, pattern, relief, bend });
        }
        at += record;
    }
    Ok(Scene {
        surfaces: list,
        sample_rate: get("sampleRateHz"),
        speed_of_sound: get("speedOfSoundMetresPerSecond"),
        length: get("lengthSamples") as usize,
        max_order: get("order") as usize,
        microphone: vector("microphone"),
        arrival: normalize(vector("arrival")),
        source: (get("pointSource") > 0.0).then(|| vector("source")),
        gain: get("gain"),
        delay_seconds: get("delaySeconds"),
        pipe: (get("pipeRadiusMetres") > 0.0).then(|| pipe::PipeInside {
            radius: get("pipeRadiusMetres"),
            mouth_a: vector("pipeMouthA"),
            mouth_b: vector("pipeMouthB"),
            bend: (geom::norm(vector("pipeBendAxis")) > 0.5).then(|| pipe::PipeBend { centre: vector("pipeBendCentre"), axis: normalize(vector("pipeBendAxis")) }),
            wall: pipe::PipeWall {
                reflection: std::array::from_fn(|i| get(PIPE_REFLECTIONS[i]).clamp(0.0, 1.0)),
                relief: relief::Relief::new(
                    get("pipeReliefShape"),
                    get("pipeReliefHeightMetres"),
                    get("pipeReliefPeriodMetres"),
                    [get("pipeReliefRadians").cos(), get("pipeReliefRadians").sin()],
                    [0.0, 0.0],
                    get("pipeReliefOffsetMetres"),
                    get("pipeRoughMetres"),
                    get("pipeGrainMetres"),
                    get("pipeReliefSeed"),
                ),
            },
        }),
        listener: match get("listenerKind").round() as i64 {
            3 => scene::Listener::Pair { patterns: [get("pattern"), get("secondPattern")], facings: [normalize(vector("facing")), normalize(vector("secondFacing"))] },
            2 => scene::Listener::MeasuredEar { front: normalize(vector("headFront")), outward: normalize(vector("facing")), up: normalize(vector("headUp")) },
            1 => scene::Listener::Ear { axis: normalize(vector("facing")), radius: get("headRadiusMetres") },
            _ => scene::Listener::Microphone { pattern: get("pattern"), facing: normalize(vector("facing")) },
        },
        quality: scene::Quality {
            relief_detail: get("reliefDetail").clamp(MOST_COARSE_RELIEF_DETAIL, MOST_FINE_RELIEF_DETAIL),
            element_limit: get("elementLimit").max(FEWEST_ELEMENTS),
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
        if norm(level) > 1e-6 {
            normalize(level)
        } else {
            [1.0, 0.0, 0.0]
        }
    };
    let across = geom::cross(n, along);
    let anchor = [v[6], v[7], v[8]];
    let from_anchor = sub(polygon.origin, anchor);
    relief::Relief::new(v[0], v[1], v[2], [dot(along, polygon.u), dot(along, polygon.v)], [dot(from_anchor, along), dot(from_anchor, across)], v[9], v[10], v[11], v[12])
}

// Each capsule in turn, as interleaved real and imaginary parts of bins 0 to n/2.
#[wasm_bindgen(js_name = nearSpectrumPart)]
pub fn near_spectrum_part(settings: &[f64], surfaces: &[f64], part: usize, parts: usize) -> Result<Vec<f64>, JsError> {
    let scene = read_scene(settings, surfaces).map_err(|e| JsError::new(&e))?;
    if part >= parts {
        return Err(JsError::new("part out of range"));
    }
    let spectra = filter::near_spectra_part(&scene, kirchhoff::Part { index: part, count: parts });
    Ok(spectra.iter().flatten().flat_map(|x| [x.re, x.im]).collect())
}

#[wasm_bindgen(js_name = nearFilterFromParts)]
pub fn near_filter_from_parts(settings: &[f64], spectra: &[f64]) -> Result<Vec<f32>, JsError> {
    let scene = read_scene(settings, &[]).map_err(|e| JsError::new(&e))?;
    let half = filter::half_length(&scene);
    let size = 2 * half * scene.capsules();
    if spectra.is_empty() || !spectra.len().is_multiple_of(size) {
        return Err(JsError::new("spectra do not match the filter length"));
    }
    let mut sum = vec![rustfft::num_complex::Complex64::new(0.0, 0.0); half * scene.capsules()];
    for part in spectra.chunks(size) {
        for (value, pair) in sum.iter_mut().zip(part.chunks(2)) {
            *value += rustfft::num_complex::Complex64::new(pair[0], pair[1]);
        }
    }
    Ok(sum.chunks(half).flat_map(|spectrum| filter::filter_from_spectrum(&scene, spectrum)).collect())
}

#[wasm_bindgen(js_name = settingNames)]
pub fn setting_names() -> Vec<String> {
    SETTINGS.iter().map(|name| name.to_string()).collect()
}

#[wasm_bindgen(js_name = coreVersion)]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").into()
}

#[wasm_bindgen(js_name = surfaceHeaderLength)]
pub fn surface_header_length() -> usize {
    SURFACE_HEADER
}

fn space_from_text(text: &str) -> Result<space::Space, JsError> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| JsError::new("This is not a Neartect space."))?;
    space::read_space(&value).map_err(|e| JsError::new(e.0))
}

#[wasm_bindgen(js_name = readSpace)]
pub fn read_space_text(text: &str) -> Result<String, JsError> {
    Ok(space::space_json(&space_from_text(text)?).to_string())
}

#[wasm_bindgen(js_name = sourceSpaces)]
pub fn source_spaces_text(text: &str) -> Result<String, JsError> {
    let list = jobs::source_spaces(&space_from_text(text)?);
    Ok(serde_json::Value::Array(
        list.iter()
            .map(|s| serde_json::json!({ "number": s.number, "name": s.name, "input": s.input, "space": space::space_json(&s.space) }))
            .collect(),
    )
    .to_string())
}

// Jobs, then per job: channels, variants, then per variant: parts, settings length, settings, surfaces length, surfaces.
#[wasm_bindgen(js_name = nearJobs)]
pub fn near_jobs(text: &str) -> Result<Vec<f64>, JsError> {
    Ok(jobs::flat_jobs(&jobs::channel_jobs(&space_from_text(text)?)))
}
