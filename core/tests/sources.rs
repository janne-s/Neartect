use neartect_core::{jobs, space};
use serde_json::{json, Value};

fn read(value: Value) -> space::Space {
    space::read_space(&value).unwrap()
}

fn base() -> Value {
    json!({
        "kind": "neartect.space",
        "objects": [{ "kind": "plane", "id": "w", "position": [0, -1, 1.25], "size": [4, 0, 2.5] }],
        "microphones": [{ "position": [0, -0.7, 1.2] }],
        "render": { "lengthMilliseconds": 20 }
    })
}

#[test]
fn one_source_gives_the_same_filter_as_before() {
    let space = read(base());
    let list = jobs::source_spaces(&space);
    assert_eq!(list.len(), 1);
    assert_eq!((list[0].number, list[0].input), (1, "main"));
    assert_eq!(jobs::near_filter(&list[0].space).unwrap(), jobs::near_filter(&space).unwrap());
}

#[test]
fn more_sources_read_with_defaults_slots_and_levels() {
    let mut doc = base();
    doc["moreSources"] = json!([
        { "kind": "point", "position": [1, 1, 1], "slot": 4, "levelDb": 99 },
        { "arrival": { "azimuthDegrees": 90 }, "input": "sideLeft", "on": false },
        { "slot": 4 },
        { "kind": "point" },
        "broken"
    ]);
    let space = read(doc);
    let slots: Vec<usize> = space.more_sources.iter().map(|s| s.slot).collect();
    assert_eq!(slots, vec![4, 2, 3]);
    assert_eq!(space.more_sources[0].source.level_db, 12.0);
    assert_eq!(space.more_sources[0].source.name, "Source 4");
    assert_eq!(space.more_sources[1].source.input, "sideLeft");
    assert_eq!(space.more_sources[2].source.input, "side");
    let again = read(space::space_json(&space));
    assert_eq!(again, space);
    let heard: Vec<usize> = jobs::source_spaces(&space).iter().map(|s| s.number).collect();
    assert_eq!(heard, vec![1, 3, 4]);
}

#[test]
fn each_source_hears_its_own_direction_and_level() {
    let mut doc = base();
    doc["moreSources"] = json!([{ "arrival": { "azimuthDegrees": 90, "elevationDegrees": 0 }, "levelDb": -6 }]);
    let list = jobs::source_spaces(&read(doc));
    let first = jobs::near_filter(&list[0].space).unwrap();
    let second = jobs::near_filter(&list[1].space).unwrap();
    assert_ne!(first, second);
    let mut quieter = base();
    quieter["arrival"] = json!({ "azimuthDegrees": 90, "elevationDegrees": 0 });
    let reference = jobs::near_filter(&read(quieter)).unwrap();
    let ratio = second[0].iter().map(|x| x.abs()).fold(0.0f32, f32::max) / reference[0].iter().map(|x| x.abs()).fold(0.0f32, f32::max);
    assert!((ratio - 0.501).abs() < 0.01, "{ratio}");
}

#[test]
fn a_bound_source_follows_its_object() {
    let mut doc = base();
    doc["moreSources"] = json!([{ "kind": "point", "reference": "w", "offset": [0, 0.5, 0] }, { "kind": "point", "reference": "gone", "offset": [1, 1, 1] }]);
    let space = read(doc);
    let at = space.more_sources[0].source.point.position;
    assert!((at[1] - -0.5).abs() < 1e-9, "{at:?}");
    assert_eq!(space.more_sources[1].source.point.reference, None);
}
