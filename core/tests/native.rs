use neartect_core::{jobs, space};

#[test]
fn the_native_filter_matches_the_web_assembly_filter() {
    let recorded: serde_json::Value = serde_json::from_str(include_str!("fixtures/wasm-filters.json")).expect("the fixture");
    for (name, case) in recorded.as_object().expect("cases") {
        let space = space::read_space(&case["space"]).expect("a space");
        let native = jobs::near_filter(&space).expect("a filter");
        let wasm = case["channels"].as_array().expect("channels");
        assert_eq!(native.len(), wasm.len(), "{name} channels");
        for (channel, (ours, theirs)) in native.iter().zip(wasm).enumerate() {
            let theirs: Vec<f32> = theirs.as_array().expect("samples").iter().map(|v| v.as_f64().expect("a sample") as f32).collect();
            assert_eq!(ours.len(), theirs.len(), "{name} {channel} length");
            let peak = theirs.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            let worst = ours.iter().zip(&theirs).fold(0.0f32, |m, (a, b)| m.max((a - b).abs()));
            assert!(worst <= 1e-5 * peak.max(1.0), "{name} {channel}: {worst} against peak {peak}");
        }
    }
}
