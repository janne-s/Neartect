use crate::geom::V3;
use crate::layout::{self, expand_object, face_normal, pipe_share, reflection_bands, Listener, PipeAround};
use crate::space::{js_round, Microphone, Space, RELIEF_SHAPES};
use crate::SETTINGS;
use std::f64::consts::PI;

const DISTANCE_REFERENCE_METRES: f64 = 1.0;
const NEAREST_LEVEL_METRES: f64 = 0.1;
const RELIEF_PARTS: usize = 16;
const MEASURED_HEAD_PARTS: usize = 4;

pub fn speed_of_sound(temperature_celsius: f64) -> f64 {
    331.3 * (1.0 + temperature_celsius / 273.15).sqrt()
}

pub fn arrival_vector(aim: &crate::space::Aim) -> V3 {
    let az = aim.azimuth_degrees * PI / 180.0;
    let el = aim.elevation_degrees * PI / 180.0;
    [el.cos() * az.sin(), el.cos() * az.cos(), el.sin()]
}

fn hearing<'a, T>(list: &'a [T], on: impl Fn(&T) -> bool + 'a) -> impl Iterator<Item = &'a T> + 'a {
    list.iter().filter(move |item| on(item))
}

pub fn pan_of(space: &Space, microphone: &Microphone) -> f64 {
    if let Some(pan) = microphone.pan {
        return pan;
    }
    let heard: Vec<&Microphone> = hearing(&space.microphones, |m| m.on).collect();
    match heard.iter().position(|m| m.id == microphone.id) {
        Some(index) if heard.len() > 1 => 2.0 * index as f64 / (heard.len() - 1) as f64 - 1.0,
        _ => 0.0,
    }
}

fn pan_gains(pan: f64, alone: bool) -> [f64; 2] {
    let angle = (pan + 1.0) * PI / 4.0;
    let gains = [angle.cos(), angle.sin()];
    let loudest = if alone { gains[0].max(gains[1]) } else { 1.0 };
    gains.map(|g| g / loudest)
}

pub fn listeners(space: &Space) -> Vec<Listener> {
    let microphones: Vec<&Microphone> = hearing(&space.microphones, |m| m.on).collect();
    let alone = microphones.len() == 1;
    let mut list: Vec<Listener> = microphones
        .iter()
        .map(|m| Listener {
            name: m.name.clone(),
            position: layout::point_position(space, &m.point),
            pattern: layout::pattern_value(m.pattern),
            facing: layout::aim_vector(space, &m.point, &m.aim),
            pan: pan_gains(pan_of(space, m), alone),
            ear: None,
        })
        .collect();
    for rig in hearing(&space.rigs, |r| r.on) {
        list.extend(layout::rig_members(space, rig).into_iter().map(|member| Listener { name: format!("{} {}", rig.name, member.name), ..member }));
    }
    list
}

#[derive(Clone, Copy, Debug)]
pub struct Aligned {
    pub delay_seconds: f64,
    pub gain: f64,
}

fn min_of(list: &[f64]) -> f64 {
    list.iter().copied().fold(f64::INFINITY, f64::min)
}

fn max_of(list: &[f64]) -> f64 {
    list.iter().copied().fold(f64::NEG_INFINITY, f64::max)
}

pub fn alignment(space: &Space, positions: &[V3]) -> Vec<Aligned> {
    let c = speed_of_sound(space.temperature_celsius);
    if space.source.kind == "point" {
        let source = layout::point_position(space, &space.source.point);
        let reach: Vec<f64> = positions.iter().map(|p| (source[0] - p[0]).hypot(source[1] - p[1]).hypot(source[2] - p[2])).collect();
        let nearest = min_of(&reach);
        let level = if space.source.distance { DISTANCE_REFERENCE_METRES } else { nearest.max(NEAREST_LEVEL_METRES) };
        return reach.iter().map(|&r| Aligned { delay_seconds: (r - nearest) / c, gain: level / r.max(NEAREST_LEVEL_METRES) }).collect();
    }
    let arrival = arrival_vector(&space.arrival);
    let lead: Vec<f64> = positions.iter().map(|p| arrival[0] * p[0] + arrival[1] * p[1] + arrival[2] * p[2]).collect();
    let first = max_of(&lead);
    lead.iter().map(|&l| Aligned { delay_seconds: (first - l) / c, gain: 1.0 }).collect()
}

pub struct Variant {
    pub settings: Vec<f64>,
    pub surfaces: Vec<f64>,
    pub parts: usize,
}

pub struct Job {
    pub variants: Vec<Variant>,
    pub channels: usize,
}

fn pipe_wall(space: &Space, pipe: Option<&PipeAround>) -> Vec<(&'static str, f64)> {
    let object = pipe.map(|p| &space.objects[p.object]);
    let bands = object.map_or([1.0; 8], |o| reflection_bands(o.material));
    let names = [
        "pipeReflection63Hz",
        "pipeReflection125Hz",
        "pipeReflection250Hz",
        "pipeReflection500Hz",
        "pipeReflection1000Hz",
        "pipeReflection2000Hz",
        "pipeReflection4000Hz",
        "pipeReflection8000Hz",
    ];
    let mut values: Vec<(&'static str, f64)> = names.iter().zip(bands).map(|(&n, b)| (n, b)).collect();
    let r = object.map(|o| &o.relief);
    values.extend([
        ("pipeReliefShape", r.map_or(0.0, |r| RELIEF_SHAPES.iter().position(|&s| s == r.shape).unwrap_or(0) as f64)),
        ("pipeReliefHeightMetres", r.map_or(0.0, |r| if r.shape != "flat" { r.height_metres } else { 0.0 })),
        ("pipeReliefPeriodMetres", r.map_or(1.0, |r| r.period_metres)),
        ("pipeReliefRadians", r.map_or(0.0, |r| r.direction_degrees * PI / 180.0)),
        ("pipeReliefOffsetMetres", r.map_or(0.0, |r| r.offset_metres)),
        ("pipeRoughMetres", r.map_or(0.0, |r| r.rough_metres)),
        ("pipeGrainMetres", r.map_or(1.0, |r| r.grain_metres)),
        ("pipeReliefSeed", object.map_or(0.0, |o| layout::seed_of(&o.id))),
    ]);
    values
}

pub fn pack_space(space: &Space, listener: &Listener, aligned: Aligned, longest: f64, second: Option<&Listener>, pipe: Option<&PipeAround>, share: f64) -> (Vec<f64>, Vec<f64>) {
    let point = space.source.kind == "point";
    let rate = space.render.sample_rate_hz;
    let source = if point { layout::point_position(space, &space.source.point) } else { [0.0; 3] };
    let pattern_of = |l: &Listener| if l.ear.is_some() { 1.0 } else { l.pattern };
    let facing = listener.ear.map_or(listener.facing, |e| e.axis);
    let mut values: Vec<(&str, f64)> = Vec::with_capacity(SETTINGS.len());
    let vector = |values: &mut Vec<(&str, f64)>, names: [&'static str; 3], v: V3| values.extend(names.into_iter().zip(v));
    values.push(("sampleRateHz", rate));
    values.push(("speedOfSoundMetresPerSecond", speed_of_sound(space.temperature_celsius)));
    values.push(("lengthSamples", js_round((space.render.length_milliseconds / 1000.0 + longest) * rate)));
    values.push(("order", space.render.order));
    vector(&mut values, ["microphoneX", "microphoneY", "microphoneZ"], listener.position);
    vector(&mut values, ["arrivalX", "arrivalY", "arrivalZ"], arrival_vector(&space.arrival));
    values.push(("pointSource", if point { 1.0 } else { 0.0 }));
    vector(&mut values, ["sourceX", "sourceY", "sourceZ"], source);
    values.push(("gain", aligned.gain * share));
    values.push(("delaySeconds", aligned.delay_seconds));
    values.push(("pipeRadiusMetres", pipe.map_or(0.0, |p| p.radius)));
    vector(&mut values, ["pipeMouthAX", "pipeMouthAY", "pipeMouthAZ"], pipe.map_or([0.0; 3], |p| p.mouth_a));
    vector(&mut values, ["pipeMouthBX", "pipeMouthBY", "pipeMouthBZ"], pipe.map_or([0.0; 3], |p| p.mouth_b));
    vector(&mut values, ["pipeBendCentreX", "pipeBendCentreY", "pipeBendCentreZ"], pipe.and_then(|p| p.bend).map_or([0.0; 3], |b| b.centre));
    vector(&mut values, ["pipeBendAxisX", "pipeBendAxisY", "pipeBendAxisZ"], pipe.and_then(|p| p.bend).map_or([0.0; 3], |b| b.axis));
    values.push(("pattern", pattern_of(listener)));
    vector(&mut values, ["facingX", "facingY", "facingZ"], facing);
    values.push(("listenerKind", if second.is_some() { 3.0 } else { listener.ear.map_or(0.0, |e| if e.measured { 2.0 } else { 1.0 }) }));
    values.push(("headRadiusMetres", listener.ear.map_or(0.0, |e| e.radius)));
    values.push(("reliefDetail", space.render.quality.relief_detail));
    values.push(("elementLimit", space.render.quality.element_limit));
    vector(&mut values, ["headFrontX", "headFrontY", "headFrontZ"], listener.ear.map_or([0.0, 1.0, 0.0], |e| e.front));
    vector(&mut values, ["headUpX", "headUpY", "headUpZ"], listener.ear.map_or([0.0, 0.0, 1.0], |e| e.up));
    values.push(("secondPattern", second.map_or(1.0, |s| s.pattern)));
    vector(&mut values, ["secondFacingX", "secondFacingY", "secondFacingZ"], second.map_or([0.0, 1.0, 0.0], |s| s.facing));
    values.extend(pipe_wall(space, pipe));
    let settings = SETTINGS.iter().map(|name| values.iter().find(|(n, _)| n == name).map_or(f64::NAN, |v| v.1)).collect();

    let mut surfaces = Vec::new();
    for (index, object) in space.objects.iter().enumerate() {
        if pipe.is_some_and(|p| p.object == index) {
            continue;
        }
        for face in expand_object(object) {
            let mut body = vec![face.strength];
            body.extend(reflection_bands(face.material));
            body.push(face.pattern.code());
            body.extend(face.pattern.values());
            body.extend(face.relief);
            body.extend(layout::bend_values(face.bend.as_ref(), face_normal(&face.vertices)));
            body.push(face.vertices.len() as f64);
            body.extend(face.vertices.iter().flatten());
            surfaces.push((body.len() + 1) as f64);
            surfaces.extend(body);
        }
    }
    (settings, surfaces)
}

fn has_relief(object: &crate::space::Object) -> bool {
    let r = &object.relief;
    object.strength > 0.0 && ((r.shape != "flat" && r.height_metres > 0.0) || r.rough_metres > 0.0)
}

fn coincident(a: &Listener, b: Option<&Listener>) -> bool {
    b.is_some_and(|b| a.ear.is_none() && b.ear.is_none() && a.position == b.position)
}

pub fn channel_jobs(space: &Space) -> Vec<Job> {
    let list = listeners(space);
    let aligned = alignment(space, &list.iter().map(|l| l.position).collect::<Vec<_>>());
    let longest = max_of(&aligned.iter().map(|a| a.delay_seconds).collect::<Vec<_>>());
    let split = space.objects.iter().any(has_relief);
    let mut jobs = Vec::new();
    let mut index = 0;
    while index < list.len() {
        let listener = &list[index];
        let second = list.get(index + 1).filter(|b| coincident(listener, Some(b)));
        let models: Vec<(Option<PipeAround>, f64)> = match pipe_share(space, listener.position) {
            None => vec![(None, 1.0)],
            Some((_, share)) if share <= 0.0 => vec![(None, 1.0)],
            Some((pipe, share)) if share >= 1.0 => vec![(Some(pipe), 1.0)],
            Some((pipe, share)) => vec![(Some(pipe), share), (None, 1.0 - share)],
        };
        let variants = models
            .iter()
            .map(|(pipe, share)| {
                let (settings, surfaces) = pack_space(space, listener, aligned[index], longest, second, pipe.as_ref(), *share);
                let parts = if pipe.is_some() || !split {
                    1
                } else if listener.ear.is_some_and(|e| e.measured) {
                    MEASURED_HEAD_PARTS
                } else {
                    RELIEF_PARTS
                };
                Variant { settings, surfaces, parts }
            })
            .collect();
        jobs.push(Job { variants, channels: if second.is_some() { 2 } else { 1 } });
        index += if second.is_some() { 2 } else { 1 };
    }
    jobs
}

pub fn flat_jobs(jobs: &[Job]) -> Vec<f64> {
    let mut out = vec![jobs.len() as f64];
    for job in jobs {
        out.push(job.channels as f64);
        out.push(job.variants.len() as f64);
        for v in &job.variants {
            out.push(v.parts as f64);
            out.push(v.settings.len() as f64);
            out.extend(&v.settings);
            out.push(v.surfaces.len() as f64);
            out.extend(&v.surfaces);
        }
    }
    out
}
