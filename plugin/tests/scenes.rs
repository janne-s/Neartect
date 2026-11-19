use neartect_plugin::params::Latch;
use neartect_plugin::scenes::{chosen, resolve, with_active};
use serde_json::{json, Value};

fn document() -> String {
    json!({
        "kind": "neartect.space",
        "version": 1,
        "objects": [{ "id": "a" }],
        "scene": 3,
        "sceneNames": ["Dry", null, null, null, null, null, null, null],
        "scenes": [{ "objects": [{ "id": "one" }] }, null, { "objects": [{ "id": "stale" }] }, null, null, { "objects": [{ "id": "six" }] }, null, null]
    })
    .to_string()
}

fn ids(scene: &str) -> Vec<String> {
    let value: Value = serde_json::from_str(scene).unwrap();
    value["objects"].as_array().unwrap().iter().map(|o| o["id"].as_str().unwrap().to_string()).collect()
}

#[test]
fn an_empty_number_plays_the_nearest_scene_below_then_above() {
    let present = [false, true, false, true, false, false, false, false];
    assert_eq!(resolve(&present, 1), 2);
    assert_eq!(resolve(&present, 2), 2);
    assert_eq!(resolve(&present, 3), 2);
    assert_eq!(resolve(&present, 8), 4);
}

#[test]
fn the_body_is_the_active_scene_and_the_others_come_from_the_list() {
    let doc = document();
    let (n, scene) = chosen(&doc, 3).unwrap();
    assert_eq!((n, ids(&scene)), (3, vec!["a".to_string()]));
    let scene: Value = serde_json::from_str(&scene).unwrap();
    assert!(scene.get("scenes").is_none() && scene.get("scene").is_none());
    let (n, scene) = chosen(&doc, 7).unwrap();
    assert_eq!((n, ids(&scene)), (6, vec!["six".to_string()]));
    let scene: Value = serde_json::from_str(&scene).unwrap();
    assert_eq!(scene["kind"], "neartect.space");
    assert_eq!(chosen(&doc, 2).unwrap().0, 1);
}

#[test]
fn a_document_without_scenes_is_scene_one() {
    let doc = json!({ "kind": "neartect.space", "objects": [{ "id": "a" }] }).to_string();
    for n in 1..=8 {
        assert_eq!(chosen(&doc, n).unwrap().0, 1);
    }
}

#[test]
fn making_a_scene_active_keeps_the_previous_body_in_the_list() {
    let doc = document();
    let (_, six) = chosen(&doc, 6).unwrap();
    let next: Value = serde_json::from_str(&with_active(&doc, 6, &six).unwrap()).unwrap();
    assert_eq!(next["scene"], 6);
    assert_eq!(next["objects"][0]["id"], "six");
    assert_eq!(next["scenes"][2]["objects"][0]["id"], "a");
    assert_eq!(next["scenes"][5]["objects"][0]["id"], "six");
    assert_eq!(next["scenes"].as_array().unwrap().len(), 8);
    assert_eq!(next["sceneNames"][0], "Dry");
}

#[test]
fn a_switch_latches_the_host_values() {
    let mut latch = Latch::default();
    latch.enter(1, &[0.5, 2.0], |_| false);
    assert!(latch.overrides(0, 0.5) && latch.overrides(1, 2.0), "before any switch every host value counts");
    latch.enter(1, &[0.7, 2.0], |_| false);
    assert!(latch.overrides(0, 0.7));
    latch.enter(2, &[0.7, 2.0], |_| false);
    assert!(!latch.overrides(0, 0.7) && !latch.overrides(1, 2.0));
    assert!(latch.overrides(0, 0.8));
    latch.enter(2, &[0.8, 2.0], |_| false);
    assert!(latch.overrides(0, 0.8), "staying in the scene keeps the baseline");
}

#[test]
fn a_parameter_moving_at_the_switch_keeps_the_host_value() {
    let mut latch = Latch::default();
    latch.enter(1, &[0.5, 2.0], |_| false);
    latch.enter(2, &[0.6, 2.0], |i| i == 0);
    assert!(latch.overrides(0, 0.6), "a moving parameter goes on following the host");
    assert!(!latch.overrides(1, 2.0), "a parameter at rest takes the scene's value");
}
