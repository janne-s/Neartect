use crate::geom::V3;
use crate::layout;
use serde_json::{json, Map, Value};
use std::collections::HashSet;

pub const SPACE_KIND: &str = "neartect.space";
pub const SPACE_VERSION: f64 = 1.0;
pub const MOST_MICROPHONES: usize = 8;
pub const MOST_RIGS: usize = 4;
pub const OBJECT_SLOTS: usize = 16;

pub const OBJECT_KINDS: [&str; 8] = ["plane", "disc", "grating", "wall", "box", "corner", "recess", "pipe"];
pub const BEND_KINDS: [&str; 5] = ["plane", "wall", "grating", "pipe", "disc"];
pub const BEND_AXES: [&str; 2] = ["up", "across"];
pub const RELIEF_SHAPES: [&str; 5] = ["flat", "waves", "arches", "scallops", "egg"];
pub const OPENING_SHAPES: [&str; 2] = ["round", "rectangle"];
pub const SOURCE_KINDS: [&str; 2] = ["far", "point"];
pub const HEADS: [&str; 2] = ["measured", "sphere"];
pub const PATTERNS: [(&str, f64); 6] = [("omni", 1.0), ("wide", 0.75), ("cardioid", 0.5), ("supercardioid", 0.37), ("hypercardioid", 0.25), ("figure8", 0.0)];
pub const DEFAULT_MATERIAL: &str = "concrete";

pub struct RigKind {
    pub id: &'static str,
    pub label: &'static str,
    pub pattern: &'static str,
    pub spacing_metres: f64,
    pub angle_degrees: f64,
}

pub const RIG_KINDS: [RigKind; 8] = [
    RigKind { id: "xy", label: "XY", pattern: "cardioid", spacing_metres: 0.0, angle_degrees: 90.0 },
    RigKind { id: "blumlein", label: "Blumlein", pattern: "figure8", spacing_metres: 0.0, angle_degrees: 90.0 },
    RigKind { id: "ms", label: "Mid-side", pattern: "", spacing_metres: 0.0, angle_degrees: 0.0 },
    RigKind { id: "ortf", label: "ORTF", pattern: "cardioid", spacing_metres: 0.17, angle_degrees: 110.0 },
    RigKind { id: "nos", label: "NOS", pattern: "cardioid", spacing_metres: 0.3, angle_degrees: 90.0 },
    RigKind { id: "ab", label: "Spaced pair", pattern: "omni", spacing_metres: 0.6, angle_degrees: 0.0 },
    RigKind { id: "decca", label: "Decca tree", pattern: "omni", spacing_metres: 2.0, angle_degrees: 0.0 },
    RigKind { id: "binaural", label: "Head", pattern: "", spacing_metres: 0.175, angle_degrees: 0.0 },
];

pub type Range = (f64, f64);
pub const POSITION_METRES: Range = (-50.0, 50.0);
pub const SIZE_METRES: Range = (0.01, 50.0);
pub const BEND_DEGREES: Range = (-180.0, 180.0);
pub const STRENGTH: Range = (0.0, 2.0);
pub const ELEVATION_DEGREES: Range = (-90.0, 90.0);
pub const TEMPERATURE_CELSIUS: Range = (-40.0, 50.0);
pub const SAMPLE_RATE_HZ: Range = (8000.0, 192000.0);
pub const LENGTH_MILLISECONDS: Range = (10.0, 1000.0);
pub const ORDER: Range = (1.0, 6.0);
pub const PERIOD_METRES: Range = (0.0, 1.0);
pub const THICKNESS_METRES: Range = (0.0, 0.2);
pub const RELIEF_HEIGHT_METRES: Range = (0.0, 0.5);
pub const RELIEF_PERIOD_METRES: Range = (0.01, 5.0);
pub const ROUGH_METRES: Range = (0.0, 0.05);
pub const GRAIN_METRES: Range = (0.01, 1.0);
pub const SPEED_METRES_PER_SECOND: Range = (-5.0, 5.0);
pub const RELIEF_DETAIL: Range = (1.0, 24.0);
pub const ELEMENT_LIMIT: Range = (1000.0, 5000000.0);
pub const MOVING_DETAIL: Range = (0.1, 1.0);
pub const PAN: Range = (-1.0, 1.0);
pub const RIG_SPACING_METRES: Range = (0.0, 5.0);
pub const RIG_ANGLE_DEGREES: Range = (0.0, 180.0);

#[derive(Clone, Debug, PartialEq)]
pub struct Point {
    pub position: V3,
    pub reference: Option<String>,
    pub offset: V3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aim {
    pub azimuth_degrees: f64,
    pub elevation_degrees: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Microphone {
    pub id: String,
    pub name: String,
    pub point: Point,
    pub pattern: &'static str,
    pub aim: Aim,
    pub pan: Option<f64>,
    pub on: bool,
    pub slot: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rig {
    pub id: String,
    pub kind: &'static RigKind,
    pub name: String,
    pub point: Point,
    pub aim: Aim,
    pub spacing_metres: f64,
    pub angle_degrees: f64,
    pub head: Option<&'static str>,
    pub on: bool,
    pub slot: usize,
}

impl PartialEq for RigKind {
    fn eq(&self, other: &RigKind) -> bool {
        self.id == other.id
    }
}

impl std::fmt::Debug for RigKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.id)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub kind: &'static str,
    pub point: Point,
    pub distance: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Relief {
    pub shape: &'static str,
    pub height_metres: f64,
    pub period_metres: f64,
    pub direction_degrees: f64,
    pub rough_metres: f64,
    pub grain_metres: f64,
    pub offset_metres: f64,
    pub speed_metres_per_second: f64,
}

pub const FLAT_RELIEF: Relief = Relief {
    shape: "flat",
    height_metres: 0.02,
    period_metres: 0.2,
    direction_degrees: 0.0,
    rough_metres: 0.0,
    grain_metres: 0.05,
    offset_metres: 0.0,
    speed_metres_per_second: 0.0,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bend {
    pub angle_degrees: f64,
    pub axis: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    pub period_metres: [f64; 2],
    pub bar_metres: [f64; 2],
    pub thickness_metres: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Opening {
    pub shape: &'static str,
    pub size_metres: [f64; 2],
    pub offset_metres: [f64; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    pub id: String,
    pub kind: &'static str,
    pub name: String,
    pub position: V3,
    pub yaw_degrees: f64,
    pub tilt_degrees: f64,
    pub size: V3,
    pub material: &'static str,
    pub strength: f64,
    pub relief: Relief,
    pub bend: Option<Bend>,
    pub pattern: Option<Grid>,
    pub opening: Option<Opening>,
    pub slot: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quality {
    pub relief_detail: f64,
    pub element_limit: f64,
    pub moving_detail: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Render {
    pub sample_rate_hz: f64,
    pub length_milliseconds: f64,
    pub order: f64,
    pub quality: Quality,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Space {
    pub temperature_celsius: f64,
    pub microphones: Vec<Microphone>,
    pub rigs: Vec<Rig>,
    pub source: Source,
    pub arrival: Aim,
    pub render: Render,
    pub objects: Vec<Object>,
}

#[derive(Debug, PartialEq)]
pub struct SpaceError(pub &'static str);

impl std::fmt::Display for SpaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

pub fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

pub fn signed_turn(degrees: f64) -> f64 {
    degrees - 360.0 * js_round(degrees / 360.0)
}

fn clamp(value: f64, (lo, hi): Range) -> f64 {
    hi.min(lo.max(value))
}

fn finite(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64).filter(|n| n.is_finite())
}

fn number(value: Option<&Value>, range: Range, fallback: f64) -> f64 {
    clamp(finite(value).unwrap_or(fallback), range)
}

fn angle(value: Option<&Value>, fallback: f64) -> f64 {
    let n = finite(value).unwrap_or(fallback);
    ((n % 360.0) + 360.0) % 360.0
}

fn signed_angle(value: Option<&Value>, fallback: f64) -> f64 {
    signed_turn(finite(value).unwrap_or(fallback))
}

fn triple(value: Option<&Value>, range: Range, fallback: V3) -> V3 {
    match value.and_then(Value::as_array).filter(|list| list.len() == 3) {
        Some(list) => std::array::from_fn(|i| number(list.get(i), range, fallback[i])),
        None => fallback.map(|v| clamp(v, range)),
    }
}

fn pair(value: Option<&Value>, range: Range, fallback: [f64; 2]) -> [f64; 2] {
    match value.and_then(Value::as_array).filter(|list| list.len() == 2) {
        Some(list) => std::array::from_fn(|i| number(list.get(i), range, fallback[i])),
        None => fallback.map(|v| clamp(v, range)),
    }
}

fn choice(value: Option<&Value>, options: &[&'static str], fallback: &'static str) -> &'static str {
    value.and_then(Value::as_str).and_then(|s| options.iter().find(|&&o| o == s).copied()).unwrap_or(fallback)
}

fn boolean(value: Option<&Value>, fallback: bool) -> bool {
    value.and_then(Value::as_bool).unwrap_or(fallback)
}

fn record(value: Option<&Value>) -> &Map<String, Value> {
    static EMPTY: std::sync::OnceLock<Map<String, Value>> = std::sync::OnceLock::new();
    value.and_then(Value::as_object).unwrap_or_else(|| EMPTY.get_or_init(Map::new))
}

fn truncated(text: &str, most: usize) -> String {
    let units: Vec<u16> = text.encode_utf16().take(most).collect();
    String::from_utf16_lossy(&units)
}

fn name(value: Option<&Value>, fallback: String) -> String {
    match value.and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => truncated(s, 60),
        None => fallback,
    }
}

struct Ids {
    taken: HashSet<String>,
    next: usize,
}

impl Ids {
    fn of(document: &Map<String, Value>) -> Ids {
        let mut taken = HashSet::new();
        for list in ["objects", "microphones", "rigs"] {
            for item in document.get(list).and_then(Value::as_array).into_iter().flatten() {
                if let Some(id) = item.get("id").and_then(Value::as_str) {
                    taken.insert(truncated(id, 40));
                }
            }
        }
        Ids { taken, next: 1 }
    }

    fn fresh(&mut self) -> String {
        loop {
            let id = format!("n{}", self.next);
            self.next += 1;
            if self.taken.insert(id.clone()) {
                return id;
            }
        }
    }

    fn read(&mut self, value: Option<&Value>) -> String {
        match value.and_then(Value::as_str).filter(|s| !s.is_empty()) {
            Some(s) => truncated(s, 40),
            None => self.fresh(),
        }
    }
}

pub fn material_ids() -> impl Iterator<Item = &'static str> {
    crate::layout::MATERIALS.iter().map(|m| m.id)
}

fn material(value: Option<&Value>) -> &'static str {
    let wanted = value.and_then(Value::as_str);
    material_ids().find(|&id| Some(id) == wanted).unwrap_or(DEFAULT_MATERIAL)
}

pub fn object_default_size(kind: &str) -> V3 {
    match kind {
        "disc" => [0.6, 0.0, 0.6],
        "grating" => [1.0, 0.0, 1.0],
        "wall" => [3.0, 0.0, 2.5],
        "box" => [0.6, 0.6, 0.6],
        "corner" => [2.0, 2.0, 2.5],
        "recess" => [0.6, 0.4, 0.8],
        "pipe" => [0.3, 2.0, 0.3],
        _ => [2.0, 0.0, 2.0],
    }
}

fn read_relief(value: Option<&Value>) -> Relief {
    let r = record(value);
    let d = FLAT_RELIEF;
    Relief {
        shape: choice(r.get("shape"), &RELIEF_SHAPES, "flat"),
        height_metres: number(r.get("heightMetres"), RELIEF_HEIGHT_METRES, d.height_metres),
        period_metres: number(r.get("periodMetres"), RELIEF_PERIOD_METRES, d.period_metres),
        direction_degrees: angle(r.get("directionDegrees"), d.direction_degrees),
        rough_metres: number(r.get("roughMetres"), ROUGH_METRES, d.rough_metres),
        grain_metres: number(r.get("grainMetres"), GRAIN_METRES, d.grain_metres),
        offset_metres: number(r.get("offsetMetres"), POSITION_METRES, 0.0),
        speed_metres_per_second: number(r.get("speedMetresPerSecond"), SPEED_METRES_PER_SECOND, 0.0),
    }
}

fn read_bend(value: Option<&Value>) -> Bend {
    let b = record(value);
    Bend { angle_degrees: number(b.get("angleDegrees"), BEND_DEGREES, 0.0), axis: choice(b.get("axis"), &BEND_AXES, "up") }
}

fn read_grid(value: Option<&Value>) -> Grid {
    let p = record(value);
    let period_metres = pair(p.get("periodMetres"), PERIOD_METRES, [0.034, 0.034]);
    let bars = pair(p.get("barMetres"), PERIOD_METRES, [0.004, 0.004]);
    Grid { period_metres, bar_metres: std::array::from_fn(|i| bars[i].min(period_metres[i])), thickness_metres: number(p.get("thicknessMetres"), THICKNESS_METRES, 0.03) }
}

pub fn fitted_opening(opening: Opening, size: V3) -> Opening {
    let span = [size[0], size[2]];
    let size_metres: [f64; 2] = std::array::from_fn(|i| opening.size_metres[i].min(span[i]));
    let offset_metres = std::array::from_fn(|i| {
        let room = (span[i] - size_metres[i]) / 2.0;
        (-room).max(room.min(opening.offset_metres[i]))
    });
    Opening { shape: opening.shape, size_metres, offset_metres }
}

fn read_opening(value: Option<&Value>, size: V3) -> Opening {
    let o = record(value);
    let size_metres = pair(o.get("sizeMetres"), SIZE_METRES, [0.4, 0.4]);
    let offset_metres = pair(o.get("offsetMetres"), POSITION_METRES, [0.0, 0.0]);
    fitted_opening(Opening { shape: choice(o.get("shape"), &OPENING_SHAPES, "round"), size_metres, offset_metres }, size)
}

fn wanted_slot(value: Option<&Value>, most: usize) -> Option<usize> {
    let n = finite(value)?;
    (n.fract() == 0.0 && n >= 1.0 && n <= most as f64).then_some(n as usize)
}

fn read_object(value: &Value, index: usize, ids: &mut Ids) -> Object {
    let o = record(Some(value));
    let kind = choice(o.get("kind"), &OBJECT_KINDS, "plane");
    let defaults = object_default_size(kind);
    let given = o.get("size").and_then(Value::as_array).filter(|list| list.len() == 3);
    let mut size: V3 = std::array::from_fn(|i| if defaults[i] == 0.0 { 0.0 } else { number(given.and_then(|list| list.get(i)), SIZE_METRES, defaults[i]) });
    if kind == "pipe" {
        size[2] = size[0];
    }
    Object {
        id: ids.read(o.get("id")),
        kind,
        name: name(o.get("name"), format!("Object {}", index + 1)),
        position: triple(o.get("position"), POSITION_METRES, [0.0, 0.0, 1.0]),
        yaw_degrees: angle(o.get("yawDegrees"), 0.0),
        tilt_degrees: signed_angle(o.get("tiltDegrees"), 0.0),
        size,
        material: material(o.get("material")),
        strength: number(o.get("strength"), STRENGTH, 1.0),
        relief: read_relief(o.get("relief")),
        bend: BEND_KINDS.contains(&kind).then(|| read_bend(o.get("bend"))),
        pattern: (kind == "grating").then(|| read_grid(o.get("pattern"))),
        opening: (kind == "wall").then(|| read_opening(o.get("opening"), size)),
        slot: None,
    }
}

fn read_point(p: &Map<String, Value>, fallback: V3) -> Point {
    Point {
        position: triple(p.get("position"), POSITION_METRES, fallback),
        reference: p.get("reference").and_then(Value::as_str).map(str::to_string),
        offset: triple(p.get("offset"), POSITION_METRES, [0.0, 0.0, 0.0]),
    }
}

fn read_aim(value: Option<&Value>) -> Aim {
    let a = record(value);
    Aim { azimuth_degrees: angle(a.get("azimuthDegrees"), 0.0), elevation_degrees: number(a.get("elevationDegrees"), ELEVATION_DEGREES, 0.0) }
}

fn read_microphone(value: &Value, index: usize, ids: &mut Ids) -> Microphone {
    let m = record(Some(value));
    let wanted = m.get("pattern").and_then(Value::as_str);
    Microphone {
        id: ids.read(m.get("id")),
        name: name(m.get("name"), format!("Mic {}", index + 1)),
        point: read_point(m, [0.0, 0.0, 1.0]),
        pattern: PATTERNS.iter().map(|p| p.0).find(|&p| Some(p) == wanted).unwrap_or("omni"),
        aim: read_aim(m.get("aim")),
        pan: finite(m.get("pan")).map(|v| clamp(v, PAN)),
        on: boolean(m.get("on"), true),
        slot: 0,
    }
}

fn read_rig(value: &Value, ids: &mut Ids) -> Rig {
    let r = record(Some(value));
    let wanted = r.get("kind").and_then(Value::as_str);
    let kind = RIG_KINDS.iter().find(|k| Some(k.id) == wanted).unwrap_or(&RIG_KINDS[0]);
    Rig {
        id: ids.read(r.get("id")),
        kind,
        name: name(r.get("name"), kind.label.to_string()),
        point: read_point(r, [0.0, 0.0, 1.0]),
        aim: read_aim(r.get("aim")),
        spacing_metres: number(r.get("spacingMetres"), RIG_SPACING_METRES, kind.spacing_metres),
        angle_degrees: number(r.get("angleDegrees"), RIG_ANGLE_DEGREES, kind.angle_degrees),
        head: (kind.id == "binaural").then(|| choice(r.get("head"), &HEADS, "measured")),
        on: boolean(r.get("on"), true),
        slot: 0,
    }
}

fn read_source(value: Option<&Value>) -> Source {
    let s = record(value);
    Source { kind: choice(s.get("kind"), &SOURCE_KINDS, "far"), point: read_point(s, [0.0, 2.0, 1.5]), distance: boolean(s.get("distance"), true) }
}

fn assign_slots(wanted: &[Option<Option<usize>>], most: usize) -> Vec<Option<usize>> {
    let mut taken = vec![false; most + 1];
    let mut slots: Vec<Option<usize>> = wanted
        .iter()
        .map(|w| match w {
            Some(Some(n)) if !taken[*n] => {
                taken[*n] = true;
                Some(*n)
            }
            _ => None,
        })
        .collect();
    for (slot, w) in slots.iter_mut().zip(wanted) {
        let cleared = matches!(w, Some(None));
        if slot.is_none() && !cleared {
            *slot = (1..=most).find(|&n| !taken[n]);
            if let Some(n) = *slot {
                taken[n] = true;
            }
        }
    }
    slots
}

fn slot_wish(item: &Value, most: usize) -> Option<Option<usize>> {
    match item.get("slot") {
        Some(Value::Null) => Some(None),
        value => wanted_slot(value, most).map(Some),
    }
}

pub fn read_space(value: &Value) -> Result<Space, SpaceError> {
    let document = value.as_object().filter(|d| d.get("kind").and_then(Value::as_str) == Some(SPACE_KIND)).ok_or(SpaceError("This is not a Neartect space."))?;
    let objects_value = document.get("objects");
    if objects_value.is_some_and(|o| !o.is_array()) {
        return Err(SpaceError("The objects are not a list."));
    }
    let mut ids = Ids::of(document);
    let air = record(document.get("air"));
    let rig_values: Vec<Value> = document.get("rigs").and_then(Value::as_array).map(|list| list.iter().take(MOST_RIGS).cloned().collect()).unwrap_or_default();
    let microphone_values: Vec<Value> = match document.get("microphones").and_then(Value::as_array) {
        Some(list) => list.iter().take(MOST_MICROPHONES).cloned().collect(),
        None => vec![document.get("microphone").filter(|m| m.is_object()).cloned().unwrap_or_else(|| json!({}))],
    };
    let render = record(document.get("render"));
    let quality = record(render.get("quality"));
    let object_values: Vec<Value> = objects_value.and_then(Value::as_array).cloned().unwrap_or_default();
    let mut space = Space {
        temperature_celsius: number(air.get("temperatureCelsius"), TEMPERATURE_CELSIUS, 20.0),
        microphones: microphone_values.iter().enumerate().map(|(i, m)| read_microphone(m, i, &mut ids)).collect(),
        rigs: rig_values.iter().map(|r| read_rig(r, &mut ids)).collect(),
        source: read_source(document.get("source")),
        arrival: read_aim(document.get("arrival")),
        render: Render {
            sample_rate_hz: js_round(number(render.get("sampleRateHz"), SAMPLE_RATE_HZ, 48000.0)),
            length_milliseconds: number(render.get("lengthMilliseconds"), LENGTH_MILLISECONDS, 80.0),
            order: js_round(number(render.get("order"), ORDER, 3.0)),
            quality: Quality {
                relief_detail: number(quality.get("reliefDetail"), RELIEF_DETAIL, 6.0),
                element_limit: js_round(number(quality.get("elementLimit"), ELEMENT_LIMIT, 400000.0)),
                moving_detail: number(quality.get("movingDetail"), MOVING_DETAIL, 0.5),
            },
        },
        objects: object_values.iter().enumerate().map(|(i, o)| read_object(o, i, &mut ids)).collect(),
    };
    let object_slots = assign_slots(&object_values.iter().map(|o| slot_wish(o, OBJECT_SLOTS)).collect::<Vec<_>>(), OBJECT_SLOTS);
    space.objects.iter_mut().zip(object_slots).for_each(|(o, slot)| o.slot = slot);
    let microphone_slots = assign_slots(&microphone_values.iter().map(|m| slot_wish(m, MOST_MICROPHONES).filter(Option::is_some)).collect::<Vec<_>>(), MOST_MICROPHONES);
    space.microphones.iter_mut().zip(microphone_slots).for_each(|(m, slot)| m.slot = slot.unwrap_or(0));
    let rig_slots = assign_slots(&rig_values.iter().map(|r| slot_wish(r, MOST_RIGS).filter(Option::is_some)).collect::<Vec<_>>(), MOST_RIGS);
    space.rigs.iter_mut().zip(rig_slots).for_each(|(r, slot)| r.slot = slot.unwrap_or(0));

    let mut seen = HashSet::new();
    for i in 0..space.microphones.len() {
        if !seen.insert(space.microphones[i].id.clone()) {
            space.microphones[i].id = ids.fresh();
            seen.insert(space.microphones[i].id.clone());
        }
    }
    for i in 0..space.rigs.len() {
        if !seen.insert(space.rigs[i].id.clone()) {
            space.rigs[i].id = ids.fresh();
            seen.insert(space.rigs[i].id.clone());
        }
    }
    let object_ids: HashSet<String> = space.objects.iter().map(|o| o.id.clone()).collect();
    let release = |point: &mut Point| {
        if !point.reference.as_ref().is_some_and(|r| object_ids.contains(r)) {
            point.reference = None;
            point.offset = [0.0, 0.0, 0.0];
        }
    };
    space.microphones.iter_mut().for_each(|m| release(&mut m.point));
    space.rigs.iter_mut().for_each(|r| release(&mut r.point));
    release(&mut space.source.point);
    layout::place_points(&mut space);
    Ok(space)
}

fn point_json(map: &mut Map<String, Value>, p: &Point) {
    map.insert("position".into(), json!(p.position));
    map.insert("reference".into(), json!(p.reference));
    map.insert("offset".into(), json!(p.offset));
}

fn aim_json(a: &Aim) -> Value {
    json!({ "azimuthDegrees": a.azimuth_degrees, "elevationDegrees": a.elevation_degrees })
}

fn object_json(o: &Object) -> Value {
    let r = &o.relief;
    let mut map = Map::new();
    map.insert("id".into(), json!(o.id));
    map.insert("kind".into(), json!(o.kind));
    map.insert("name".into(), json!(o.name));
    map.insert("position".into(), json!(o.position));
    map.insert("yawDegrees".into(), json!(o.yaw_degrees));
    map.insert("tiltDegrees".into(), json!(o.tilt_degrees));
    map.insert("size".into(), json!(o.size));
    map.insert("material".into(), json!(o.material));
    map.insert("strength".into(), json!(o.strength));
    map.insert(
        "relief".into(),
        json!({
            "shape": r.shape, "heightMetres": r.height_metres, "periodMetres": r.period_metres, "directionDegrees": r.direction_degrees,
            "roughMetres": r.rough_metres, "grainMetres": r.grain_metres, "offsetMetres": r.offset_metres, "speedMetresPerSecond": r.speed_metres_per_second
        }),
    );
    if let Some(b) = &o.bend {
        map.insert("bend".into(), json!({ "angleDegrees": b.angle_degrees, "axis": b.axis }));
    }
    if let Some(p) = &o.pattern {
        map.insert("pattern".into(), json!({ "kind": "grid", "periodMetres": p.period_metres, "barMetres": p.bar_metres, "thicknessMetres": p.thickness_metres }));
    }
    if let Some(p) = &o.opening {
        map.insert("opening".into(), json!({ "shape": p.shape, "sizeMetres": p.size_metres, "offsetMetres": p.offset_metres }));
    }
    map.insert("slot".into(), json!(o.slot));
    Value::Object(map)
}

fn microphone_json(m: &Microphone) -> Value {
    let mut map = Map::new();
    map.insert("id".into(), json!(m.id));
    map.insert("name".into(), json!(m.name));
    point_json(&mut map, &m.point);
    map.insert("pattern".into(), json!(m.pattern));
    map.insert("aim".into(), aim_json(&m.aim));
    map.insert("pan".into(), json!(m.pan));
    map.insert("on".into(), json!(m.on));
    map.insert("slot".into(), json!(m.slot));
    Value::Object(map)
}

fn rig_json(r: &Rig) -> Value {
    let mut map = Map::new();
    map.insert("id".into(), json!(r.id));
    map.insert("kind".into(), json!(r.kind.id));
    map.insert("name".into(), json!(r.name));
    point_json(&mut map, &r.point);
    map.insert("aim".into(), aim_json(&r.aim));
    map.insert("spacingMetres".into(), json!(r.spacing_metres));
    map.insert("angleDegrees".into(), json!(r.angle_degrees));
    if let Some(head) = r.head {
        map.insert("head".into(), json!(head));
    }
    map.insert("on".into(), json!(r.on));
    map.insert("slot".into(), json!(r.slot));
    Value::Object(map)
}

pub fn space_json(space: &Space) -> Value {
    let mut source = Map::new();
    source.insert("kind".into(), json!(space.source.kind));
    point_json(&mut source, &space.source.point);
    source.insert("distance".into(), json!(space.source.distance));
    let q = &space.render.quality;
    json!({
        "kind": SPACE_KIND,
        "version": SPACE_VERSION,
        "air": { "temperatureCelsius": space.temperature_celsius },
        "microphones": space.microphones.iter().map(microphone_json).collect::<Vec<_>>(),
        "rigs": space.rigs.iter().map(rig_json).collect::<Vec<_>>(),
        "source": source,
        "arrival": aim_json(&space.arrival),
        "render": {
            "sampleRateHz": space.render.sample_rate_hz,
            "lengthMilliseconds": space.render.length_milliseconds,
            "order": space.render.order,
            "quality": { "reliefDetail": q.relief_detail, "elementLimit": q.element_limit, "movingDetail": q.moving_detail }
        },
        "objects": space.objects.iter().map(object_json).collect::<Vec<_>>()
    })
}

pub fn moving(space: &Space) -> bool {
    space.objects.iter().any(|o| o.relief.speed_metres_per_second != 0.0 && (o.relief.shape != "flat" || o.relief.rough_metres > 0.0))
}

pub fn set_in_motion(space: &mut Space, seconds: f64) {
    for object in &mut space.objects {
        object.relief.offset_metres += object.relief.speed_metres_per_second * seconds;
    }
    let quality = &mut space.render.quality;
    quality.relief_detail = (quality.relief_detail * quality.moving_detail).max(RELIEF_DETAIL.0);
    quality.element_limit = js_round(quality.element_limit * quality.moving_detail).max(ELEMENT_LIMIT.0);
}
