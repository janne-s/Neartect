use neartect_core::{jobs, space};
use neartect_plugin::compute::{compute, Builder, Outcome};
use std::sync::mpsc;
use std::sync::Arc;

fn corrugated() -> String {
    serde_json::json!({
        "kind": "neartect.space",
        "objects": [{ "kind": "plane", "id": "w", "position": [0, -1, 1.5], "size": [3, 0, 2], "relief": { "shape": "arches", "heightMetres": 0.04, "periodMetres": 0.15 } }],
        "rigs": [{ "kind": "xy", "id": "x", "position": [0, -0.7, 1.2] }],
        "microphones": [],
        "render": { "lengthMilliseconds": 20 }
    })
    .to_string()
}

#[test]
fn the_pool_builds_the_same_filter_as_the_core_and_mixes_it_to_stereo() {
    let json = corrugated();
    let built = compute(&json, None, Arc::new(|| true)).expect("a filter").expect("not cancelled");
    let space = space::read_space(&serde_json::from_str(&json).unwrap()).unwrap();
    assert_eq!(built.channels, jobs::near_filter(&space).unwrap());
    assert_eq!(built.channels.len(), 2);
    assert_eq!(built.stereo[0], built.channels[0]);
    assert_eq!(built.stereo[1], built.channels[1]);
}

#[test]
fn a_build_no_longer_wanted_is_dropped_and_the_rate_can_be_set() {
    let json = corrugated().replace("\"lengthMilliseconds\":20", "\"lengthMilliseconds\":21");
    assert!(compute(&json, None, Arc::new(|| false)).unwrap().is_none());
    let built = compute(&json, Some(44100.0), Arc::new(|| true)).unwrap().unwrap();
    assert_eq!(built.sample_rate_hz, 44100.0);
}

#[test]
fn a_space_without_listeners_builds_silence() {
    let json = serde_json::json!({ "kind": "neartect.space", "microphones": [] }).to_string();
    let built = compute(&json, None, Arc::new(|| true)).unwrap().unwrap();
    assert!(built.channels.is_empty());
    assert_eq!(built.stereo, [vec![0.0], vec![0.0]]);
}

#[test]
fn a_newer_request_supersedes_the_waiting_ones_and_each_gets_an_answer() {
    let (sent, heard) = mpsc::channel();
    let (_retired, retired) = crossbeam_channel::bounded::<()>(1);
    let (_wake, woken) = crossbeam_channel::bounded::<()>(1);
    let sent = std::sync::Mutex::new(sent);
    let builder = Builder::new(retired, woken, || None, move |outcome| sent.lock().unwrap().send(outcome).unwrap());
    let json = corrugated();
    let generations: Vec<u64> = (0..5).map(|i| builder.request(json.replace("1.2]", &format!("1.{}]", 3 + i)), None, Some(i))).collect();
    assert_eq!(builder.latest(), *generations.last().unwrap());
    let answers: Vec<Outcome> = (0..5).map(|_| heard.recv().unwrap()).collect();
    let built: Vec<u64> = answers
        .iter()
        .filter_map(|o| if let Outcome::Built { generation, .. } = o { Some(*generation) } else { None })
        .collect();
    assert_eq!(built.last(), generations.last(), "the newest is built");
    assert!(built.len() < 5, "older requests were dropped");
}
