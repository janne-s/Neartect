use crossbeam_channel::bounded;
use neartect_plugin::params::{apply, specs, specs_json, SlotParams};
use nih_plug::prelude::Params;
use serde_json::{json, Value};

fn read(value: &Value) -> Value {
    neartect_core::space::space_json(&neartect_core::space::read_space(value).unwrap())
}

#[test]
fn ids_are_unique_and_kept() {
    let (wake, _woken) = bounded(1);
    let ids: Vec<String> = SlotParams::new(wake).param_map().into_iter().map(|(id, _, _)| id).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len(), "duplicate parameter IDs");
    let now = ids.join("\n") + "\n";
    let list = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/param_ids.txt");
    let described = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/parameters.json");
    let description = serde_json::to_string_pretty(&specs_json(&specs())).unwrap() + "\n";
    if std::env::var_os("NEARTECT_BLESS_IDS").is_some() {
        std::fs::write(list, &now).unwrap();
        std::fs::write(described, &description).unwrap();
    }
    let kept = std::fs::read_to_string(list).expect("tests/param_ids.txt");
    let missing: Vec<&str> = kept.lines().filter(|id| !ids.iter().any(|i| i == id)).collect();
    assert!(missing.is_empty(), "parameter IDs removed or renamed: {missing:?}");
    let added: Vec<&String> = ids.iter().filter(|i| !kept.lines().any(|k| k == i.as_str())).collect();
    assert!(added.is_empty(), "new parameter IDs not yet in param_ids.txt (NEARTECT_BLESS_IDS=1): {added:?}");
    assert_eq!(std::fs::read_to_string(described).expect("tests/parameters.json"), description, "the page's parameter description is out of date (NEARTECT_BLESS_IDS=1)");
}

fn space() -> Value {
    read(&json!({
        "kind": "neartect.space",
        "objects": [
            { "kind": "plane", "id": "a", "slot": 3, "size": [2, 0, 2] },
            { "kind": "plane", "id": "b", "slot": null },
            { "kind": "pipe", "id": "c" }
        ],
        "microphones": [{ "id": "m", "reference": "a", "offset": [0.1, 0.2, 0.3] }, { "id": "n", "position": [1, 1, 1], "pan": 0.4 }],
        "source": { "kind": "point", "position": [0, 2, 1.5] }
    }))
}

fn values_from(space: &Value) -> Vec<f64> {
    let list = specs();
    let mut values: Vec<f64> = list.iter().map(|s| s.default).collect();
    for (value, spec) in values.iter_mut().zip(&list) {
        if spec.id == "object3_x" {
            *value = 1.25;
        }
        if spec.id == "object3_material" {
            *value = 0.0;
        }
        if spec.id == "object1_x" {
            *value = -2.0;
        }
        if spec.id == "mic1_y" {
            *value = 0.004;
        }
        if spec.id == "mic2_pan_auto" {
            *value = 0.0;
        }
        if spec.id == "mic2_pan" {
            *value = -0.5;
        }
        if spec.id == "source_z" {
            *value = 2.5;
        }
    }
    let _ = space;
    values
}

#[test]
fn parameters_land_on_the_slotted_items_and_nowhere_else() {
    let mut s = space();
    let before = s.clone();
    assert_eq!(s["objects"][2]["slot"], json!(1));
    apply(&mut s, &specs(), &values_from(&before));
    let s = read(&s);
    assert_eq!(s["objects"][0]["position"][0], json!(1.25));
    assert_eq!(s["objects"][0]["material"], json!("rigid"));
    assert_eq!(s["objects"][1], before["objects"][1], "an object without a slot is not touched");
    assert_eq!(s["objects"][2]["position"][0], json!(-2.0));
    assert_eq!(s["microphones"][0]["offset"], json!([0.0, 0.004, 1.0]), "a bound microphone takes its offset");
    assert_eq!(s["microphones"][0]["reference"], json!("a"));
    assert_eq!(s["microphones"][1]["pan"], json!(-0.5));
    assert_eq!(s["source"]["position"][2], json!(2.5));
}

#[test]
fn automatic_pan_wins_over_the_pan_value() {
    let mut s = space();
    let list = specs();
    let values: Vec<f64> = list.iter().map(|s| if s.id == "mic2_pan" { 0.7 } else { s.default }).collect();
    apply(&mut s, &list, &values);
    assert_eq!(read(&s)["microphones"][1]["pan"], Value::Null);
}
