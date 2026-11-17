use serde_json::{Map, Value};

pub const SCENES: usize = 8;

fn body(document: &Map<String, Value>) -> Value {
    let mut scene = document.clone();
    scene.remove("scenes");
    scene.remove("scene");
    Value::Object(scene)
}

fn active_of(document: &Map<String, Value>) -> usize {
    document.get("scene").and_then(Value::as_f64).map_or(1, |n| n.round().clamp(1.0, SCENES as f64) as usize)
}

fn defined(document: &Map<String, Value>) -> [bool; SCENES] {
    let active = active_of(document);
    let list = document.get("scenes").and_then(Value::as_array);
    std::array::from_fn(|i| i + 1 == active || list.and_then(|l| l.get(i)).is_some_and(Value::is_object))
}

pub fn resolve(present: &[bool; SCENES], number: usize) -> usize {
    let number = number.clamp(1, SCENES);
    (1..=number).rev().chain(number + 1..=SCENES).find(|&n| present[n - 1]).unwrap_or(1)
}

pub fn chosen(document: &str, number: usize) -> Option<(usize, String)> {
    let value: Value = serde_json::from_str(document).ok()?;
    let document = value.as_object()?;
    let resolved = resolve(&defined(document), number);
    let scene = if resolved == active_of(document) {
        body(document)
    } else {
        let mut scene = document.get("scenes")?.as_array()?.get(resolved - 1)?.as_object()?.clone();
        for key in ["kind", "version"] {
            if let Some(v) = document.get(key) {
                scene.entry(key).or_insert_with(|| v.clone());
            }
        }
        Value::Object(scene)
    };
    Some((resolved, scene.to_string()))
}

pub fn with_active(document: &str, number: usize, scene: &str) -> Option<String> {
    let mut value: Value = serde_json::from_str(document).ok()?;
    let scene: Value = serde_json::from_str(scene).ok()?;
    let mut list: Vec<Value> = match value.get("scenes").and_then(Value::as_array) {
        Some(list) => list.iter().take(SCENES).cloned().collect(),
        None => Vec::new(),
    };
    let previous = value.as_object().map(active_of).unwrap_or(1);
    list.resize(SCENES, Value::Null);
    list[previous - 1] = value.as_object().map(body).unwrap_or(Value::Null);
    list[number - 1] = scene.clone();
    let mut document = scene.as_object()?.clone();
    document.insert("scene".into(), number.into());
    document.insert("scenes".into(), Value::Array(list));
    if let Some(names) = value.get("sceneNames") {
        document.insert("sceneNames".into(), names.clone());
    }
    value = Value::Object(document);
    Some(value.to_string())
}
