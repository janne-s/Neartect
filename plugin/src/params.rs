use crossbeam_channel::Sender;
use neartect_core::layout::MATERIALS;
use neartect_core::space::{self, MOST_MICROPHONES, MOST_RIGS, OBJECT_SLOTS, PATTERNS, RELIEF_SHAPES};
use nih_plug::prelude::*;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Holder {
    Space,
    Source,
    Microphone(usize),
    Rig(usize),
    Object(usize),
}

#[derive(Clone, Copy, Debug)]
pub enum Key {
    Field(&'static str),
    Index(usize),
    Place(usize),
}

#[derive(Clone, Copy, Debug)]
pub enum Scale {
    Linear,
    Low,
    Centred,
}

#[derive(Clone, Debug)]
pub enum Kind {
    Float { min: f64, max: f64, unit: &'static str, scale: Scale },
    Choice(Vec<&'static str>),
    Toggle,
    PanAuto,
}

#[derive(Clone, Debug)]
pub struct Spec {
    pub id: String,
    pub name: String,
    pub group: String,
    pub holder: Holder,
    pub path: Vec<Key>,
    pub kind: Kind,
    pub default: f64,
}

const POSITION: (f64, f64) = space::POSITION_METRES;

fn float(min_max: (f64, f64), unit: &'static str, scale: Scale) -> Kind {
    Kind::Float { min: min_max.0, max: min_max.1, unit, scale }
}

fn place(holder: Holder, prefix: &str, group: &str, defaults: [f64; 3], list: &mut Vec<Spec>) {
    for (i, axis) in ["x", "y", "z"].iter().enumerate() {
        list.push(Spec {
            id: format!("{prefix}{axis}"),
            name: format!("{group} {}", axis.to_uppercase()),
            group: group.into(),
            holder,
            path: vec![Key::Place(i)],
            kind: float(POSITION, " m", Scale::Centred),
            default: defaults[i],
        });
    }
}

fn aim(holder: Holder, prefix: &str, group: &str, field: &'static str, list: &mut Vec<Spec>) {
    let spec = |id: &str, name: &str, key: &'static str, kind: Kind| Spec {
        id: format!("{prefix}{id}"),
        name: format!("{group} {name}"),
        group: group.into(),
        holder,
        path: vec![Key::Field(field), Key::Field(key)],
        kind,
        default: 0.0,
    };
    list.push(spec("azimuth", "Azimuth", "azimuthDegrees", float((0.0, 360.0), "°", Scale::Linear)));
    list.push(spec("elevation", "Elevation", "elevationDegrees", float(space::ELEVATION_DEGREES, "°", Scale::Linear)));
}

fn field(holder: Holder, prefix: &str, group: &str, id: &str, name: &str, path: Vec<Key>, kind: Kind, default: f64) -> Spec {
    Spec { id: format!("{prefix}{id}"), name: format!("{group} {name}"), group: group.into(), holder, path, kind, default }
}

pub fn specs() -> Vec<Spec> {
    use Key::{Field as F, Index as I};
    let mut list = Vec::new();
    let g = "Space";
    list.push(field(Holder::Space, "", g, "temperature", "Temperature", vec![F("air"), F("temperatureCelsius")], float(space::TEMPERATURE_CELSIUS, " °C", Scale::Linear), 20.0));
    aim(Holder::Space, "arrival_", "Arrival", "arrival", &mut list);
    list.push(field(Holder::Space, "", g, "length", "Filter Length", vec![F("render"), F("lengthMilliseconds")], float(space::LENGTH_MILLISECONDS, " ms", Scale::Low), 80.0));
    place(Holder::Source, "source_", "Source", [0.0, 2.0, 1.5], &mut list);
    list.push(field(Holder::Source, "source_", "Source", "distance", "1/r", vec![F("distance")], Kind::Toggle, 1.0));
    for n in 1..=MOST_MICROPHONES {
        let (h, p, g) = (Holder::Microphone(n), format!("mic{n}_"), format!("Mic {n}"));
        list.push(field(h, &p, &g, "on", "On", vec![F("on")], Kind::Toggle, 1.0));
        place(h, &p, &g, [0.0, 0.0, 1.0], &mut list);
        aim(h, &p, &g, "aim", &mut list);
        list.push(field(h, &p, &g, "pattern", "Pattern", vec![F("pattern")], Kind::Choice(PATTERNS.iter().map(|p| p.0).collect()), 0.0));
        list.push(field(h, &p, &g, "pan", "Pan", vec![F("pan")], float(space::PAN, "", Scale::Linear), 0.0));
        list.push(field(h, &p, &g, "pan_auto", "Pan Auto", vec![F("pan")], Kind::PanAuto, 1.0));
    }
    for n in 1..=MOST_RIGS {
        let (h, p, g) = (Holder::Rig(n), format!("layout{n}_"), format!("Layout {n}"));
        list.push(field(h, &p, &g, "on", "On", vec![F("on")], Kind::Toggle, 1.0));
        place(h, &p, &g, [0.0, 0.0, 1.0], &mut list);
        aim(h, &p, &g, "aim", &mut list);
        list.push(field(h, &p, &g, "spacing", "Spacing", vec![F("spacingMetres")], float(space::RIG_SPACING_METRES, " m", Scale::Low), 0.0));
        list.push(field(h, &p, &g, "angle", "Angle", vec![F("angleDegrees")], float(space::RIG_ANGLE_DEGREES, "°", Scale::Linear), 90.0));
    }
    let materials: Vec<&'static str> = MATERIALS.iter().map(|m| m.id).collect();
    let concrete = materials.iter().position(|&m| m == space::DEFAULT_MATERIAL).unwrap_or(0) as f64;
    for n in 1..=OBJECT_SLOTS {
        let (h, p, g) = (Holder::Object(n), format!("object{n}_"), format!("Object {n}"));
        for (i, axis) in ["x", "y", "z"].iter().enumerate() {
            list.push(field(h, &p, &g, axis, &axis.to_uppercase(), vec![F("position"), I(i)], float(POSITION, " m", Scale::Centred), if i == 2 { 1.0 } else { 0.0 }));
        }
        list.push(field(h, &p, &g, "yaw", "Facing", vec![F("yawDegrees")], float((0.0, 360.0), "°", Scale::Linear), 0.0));
        list.push(field(h, &p, &g, "tilt", "Tilt", vec![F("tiltDegrees")], float((-180.0, 180.0), "°", Scale::Linear), 0.0));
        for (i, (id, name)) in [("width", "Width"), ("depth", "Depth"), ("height", "Height")].iter().enumerate() {
            list.push(field(h, &p, &g, id, name, vec![F("size"), I(i)], float(space::SIZE_METRES, " m", Scale::Low), 2.0));
        }
        list.push(field(h, &p, &g, "bend", "Bend", vec![F("bend"), F("angleDegrees")], float(space::BEND_DEGREES, "°", Scale::Linear), 0.0));
        list.push(field(h, &p, &g, "strength", "Strength", vec![F("strength")], float(space::STRENGTH, "", Scale::Linear), 1.0));
        list.push(field(h, &p, &g, "material", "Material", vec![F("material")], Kind::Choice(materials.clone()), concrete));
        let relief = |key: &'static str| vec![F("relief"), F(key)];
        list.push(field(h, &p, &g, "relief", "Relief", relief("shape"), Kind::Choice(RELIEF_SHAPES.to_vec()), 0.0));
        list.push(field(h, &p, &g, "relief_height", "Relief Height", relief("heightMetres"), float(space::RELIEF_HEIGHT_METRES, " m", Scale::Low), 0.02));
        list.push(field(h, &p, &g, "relief_period", "Relief Period", relief("periodMetres"), float(space::RELIEF_PERIOD_METRES, " m", Scale::Low), 0.2));
        list.push(field(h, &p, &g, "relief_direction", "Relief Direction", relief("directionDegrees"), float((0.0, 360.0), "°", Scale::Linear), 0.0));
        list.push(field(h, &p, &g, "roughness", "Roughness", relief("roughMetres"), float(space::ROUGH_METRES, " m", Scale::Low), 0.0));
        list.push(field(h, &p, &g, "grain", "Grain", relief("grainMetres"), float(space::GRAIN_METRES, " m", Scale::Low), 0.05));
        list.push(field(h, &p, &g, "relief_offset", "Relief Offset", relief("offsetMetres"), float(POSITION, " m", Scale::Centred), 0.0));
        list.push(field(h, &p, &g, "motion", "Motion", relief("speedMetresPerSecond"), float(space::SPEED_METRES_PER_SECOND, " m/s", Scale::Centred), 0.0));
    }
    list
}

fn range(min: f64, max: f64, scale: Scale) -> FloatRange {
    let (min, max) = (min as f32, max as f32);
    match scale {
        Scale::Linear => FloatRange::Linear { min, max },
        Scale::Low => FloatRange::Skewed { min, max, factor: FloatRange::skew_factor(-2.0) },
        Scale::Centred => FloatRange::SymmetricalSkewed { min, max, factor: FloatRange::skew_factor(-2.0), center: 0.0 },
    }
}

fn metres(v: f32) -> String {
    let a = v.abs();
    if a < 0.1 {
        format!("{:.1} mm", v * 1000.0)
    } else if a < 10.0 {
        format!("{v:.3}")
    } else {
        format!("{v:.2}")
    }
}

pub enum Entry {
    Float(FloatParam),
    Int(IntParam),
    Bool(BoolParam),
}

impl Entry {
    fn ptr(&self) -> ParamPtr {
        match self {
            Entry::Float(p) => p.as_ptr(),
            Entry::Int(p) => p.as_ptr(),
            Entry::Bool(p) => p.as_ptr(),
        }
    }

    pub fn plain(&self) -> f64 {
        match self {
            Entry::Float(p) => p.value() as f64,
            Entry::Int(p) => p.value() as f64,
            Entry::Bool(p) => p.value() as u8 as f64,
        }
    }
}

fn entry(spec: &Spec, wake: &Sender<()>) -> Entry {
    let wake = wake.clone();
    let woken = move || {
        let _ = wake.try_send(());
    };
    match &spec.kind {
        Kind::Float { min, max, unit, scale } => {
            let mut p = FloatParam::new(spec.name.clone(), spec.default as f32, range(*min, *max, *scale)).with_callback(Arc::new(move |_| woken()));
            if *unit == " m" {
                p = p.with_value_to_string(Arc::new(metres));
            } else {
                p = p.with_unit(unit).with_value_to_string(formatters::v2s_f32_rounded(2));
            }
            Entry::Float(p)
        }
        Kind::Choice(names) => {
            let shown = names.clone();
            let read = names.clone();
            Entry::Int(
                IntParam::new(spec.name.clone(), spec.default as i32, IntRange::Linear { min: 0, max: names.len() as i32 - 1 })
                    .with_callback(Arc::new(move |_| woken()))
                    .with_value_to_string(Arc::new(move |v| shown.get(v as usize).copied().unwrap_or("").to_string()))
                    .with_string_to_value(Arc::new(move |s| read.iter().position(|n| n.eq_ignore_ascii_case(s.trim())).map(|i| i as i32))),
            )
        }
        Kind::Toggle | Kind::PanAuto => Entry::Bool(BoolParam::new(spec.name.clone(), spec.default > 0.5).with_callback(Arc::new(move |_| woken()))),
    }
}

pub struct SlotParams {
    pub specs: Vec<Spec>,
    pub entries: Vec<Entry>,
}

impl SlotParams {
    pub fn new(wake: Sender<()>) -> SlotParams {
        let specs = specs();
        let entries = specs.iter().map(|s| entry(s, &wake)).collect();
        SlotParams { specs, entries }
    }

    pub fn values(&self) -> Vec<f64> {
        self.entries.iter().map(Entry::plain).collect()
    }

    pub fn ptr(&self, index: usize) -> ParamPtr {
        self.entries[index].ptr()
    }

    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.specs.iter().position(|s| s.id == id)
    }
}

unsafe impl Params for SlotParams {
    fn param_map(&self) -> Vec<(String, ParamPtr, String)> {
        self.specs.iter().zip(&self.entries).map(|(s, e)| (s.id.clone(), e.ptr(), s.group.clone())).collect()
    }

    fn serialize_fields(&self) -> BTreeMap<String, String> {
        BTreeMap::new()
    }
}

fn holder_json(holder: Holder) -> Value {
    match holder {
        Holder::Space => json!({ "list": null }),
        Holder::Source => json!({ "list": "source" }),
        Holder::Microphone(n) => json!({ "list": "microphones", "slot": n }),
        Holder::Rig(n) => json!({ "list": "rigs", "slot": n }),
        Holder::Object(n) => json!({ "list": "objects", "slot": n }),
    }
}

pub fn specs_json(specs: &[Spec]) -> Value {
    Value::Array(
        specs
            .iter()
            .map(|s| {
                let path: Vec<Value> = s
                    .path
                    .iter()
                    .map(|k| match k {
                        Key::Field(f) => json!(f),
                        Key::Index(i) => json!(i),
                        Key::Place(i) => json!({ "place": i }),
                    })
                    .collect();
                let (kind, choices) = match &s.kind {
                    Kind::Float { .. } => ("number", None),
                    Kind::Choice(names) => ("choice", Some(names.clone())),
                    Kind::Toggle => ("toggle", None),
                    Kind::PanAuto => ("panAuto", None),
                };
                json!({ "id": s.id, "holder": holder_json(s.holder), "path": path, "kind": kind, "choices": choices })
            })
            .collect(),
    )
}

fn holder_mut(space: &mut Value, holder: Holder) -> Option<&mut Map<String, Value>> {
    let (list, slot) = match holder {
        Holder::Space => return space.as_object_mut(),
        Holder::Source => return space.get_mut("source")?.as_object_mut(),
        Holder::Microphone(n) => ("microphones", n),
        Holder::Rig(n) => ("rigs", n),
        Holder::Object(n) => ("objects", n),
    };
    space.get_mut(list)?.as_array_mut()?.iter_mut().find(|item| item.get("slot").and_then(Value::as_u64) == Some(slot as u64))?.as_object_mut()
}

fn bound(target: &Map<String, Value>) -> bool {
    target.get("reference").and_then(Value::as_str).is_some_and(|r| !r.is_empty())
}

fn put(map: &mut Map<String, Value>, path: &[Key], value: Value, bound: bool) {
    match path {
        [Key::Field(f)] => {
            map.insert(f.to_string(), value);
        }
        [Key::Place(i)] => {
            let name = if bound { "offset" } else { "position" };
            if let Some(slot) = map.get_mut(name).and_then(Value::as_array_mut).and_then(|a| a.get_mut(*i)) {
                *slot = value;
            }
        }
        [Key::Field(f), Key::Index(i)] => {
            if let Some(slot) = map.get_mut(*f).and_then(Value::as_array_mut).and_then(|a| a.get_mut(*i)) {
                *slot = value;
            }
        }
        [Key::Field(f), rest @ ..] => {
            if let Some(child) = map.entry(f.to_string()).or_insert_with(|| json!({})).as_object_mut() {
                put(child, rest, value, bound);
            }
        }
        _ => {}
    }
}

pub fn apply(space: &mut Value, specs: &[Spec], values: &[f64]) {
    apply_where(space, specs, values, |_| true);
}

pub fn apply_where(space: &mut Value, specs: &[Spec], values: &[f64], applies: impl Fn(usize) -> bool) {
    for (index, (spec, &v)) in specs.iter().zip(values).enumerate() {
        if !applies(index) {
            continue;
        }
        let Some(target) = holder_mut(space, spec.holder) else { continue };
        let value = match &spec.kind {
            Kind::Float { .. } => json!(v),
            Kind::Choice(names) => json!(names.get(v.round().max(0.0) as usize).copied().unwrap_or(names[0])),
            Kind::Toggle => json!(v > 0.5),
            Kind::PanAuto if v > 0.5 => Value::Null,
            Kind::PanAuto => continue,
        };
        let is_bound = bound(target);
        put(target, &spec.path, value, is_bound);
    }
}

pub fn applied(json: &str, specs: &[Spec], values: &[f64]) -> Option<String> {
    applied_where(json, specs, values, |_| true)
}

pub fn applied_where(json: &str, specs: &[Spec], values: &[f64], applies: impl Fn(usize) -> bool) -> Option<String> {
    let mut value: Value = serde_json::from_str(json).ok()?;
    apply_where(&mut value, specs, values, applies);
    Some(value.to_string())
}

const SAME: f64 = 1e-6;

#[derive(Default, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Latch {
    pub scene: usize,
    pub baseline: Vec<f64>,
}

impl Latch {
    pub fn enter(&mut self, scene: usize, values: &[f64]) {
        if self.scene == 0 {
            self.scene = scene;
        } else if self.scene != scene {
            self.scene = scene;
            self.baseline = values.to_vec();
        }
    }

    pub fn overrides(&self, index: usize, value: f64) -> bool {
        self.baseline.get(index).is_none_or(|&b| (value - b).abs() > SAME * b.abs().max(1.0))
    }
}
