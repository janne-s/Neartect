use crate::geom::V3;
use crate::space::{fitted_opening, Object, Point, Rig, Space, RELIEF_SHAPES};
use std::f64::consts::PI;

pub const DISC_SIDES: usize = 48;
pub const PIPE_SIDES: usize = 16;
const PIPE_BEND_SEGMENT_RADIANS: f64 = PI / 12.0;
const RECESS_MARGIN_METRES: f64 = 0.5;
const TIGHTEST_PIPE_BEND: f64 = 1.02;
const FLAT_CURVATURE: f64 = 1e-9;
const HALF: f64 = std::f64::consts::FRAC_1_SQRT_2;
// Woodworth radius for the table's largest interaural delay, 687 µs at 343 m/s.
const MEASURED_HEAD_WIDTH_METRES: f64 = 0.183;
const SPHERE_KINDS: [&str; 1] = ["disc"];

fn hypot3(a: V3) -> f64 {
    a[0].hypot(a[1]).hypot(a[2])
}

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn normalize(a: V3) -> V3 {
    let n = hypot3(a);
    if n > 0.0 {
        a.map(|v| v / n)
    } else {
        a
    }
}

fn along(p: V3, terms: &[(V3, f64)]) -> V3 {
    terms.iter().fold(p, |sum, (v, s)| std::array::from_fn(|i| sum[i] + v[i] * s))
}

fn sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        x
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub across: V3,
    pub front: V3,
    pub up: V3,
}

pub fn object_frame(yaw_degrees: f64, tilt_degrees: f64) -> Frame {
    let yaw = yaw_degrees * PI / 180.0;
    let tilt = tilt_degrees * PI / 180.0;
    let front = [tilt.cos() * yaw.sin(), tilt.cos() * yaw.cos(), tilt.sin()];
    let across = [yaw.cos(), -yaw.sin(), 0.0];
    let up = [across[1] * front[2] - across[2] * front[1], across[2] * front[0] - across[0] * front[2], across[0] * front[1] - across[1] * front[0]];
    Frame { across, front, up }
}

fn frame_of(object: &Object) -> Frame {
    object_frame(object.yaw_degrees, object.tilt_degrees)
}

pub fn to_world(object: &Object, [x, y, z]: V3) -> V3 {
    let Frame { across, front, up } = frame_of(object);
    std::array::from_fn(|i| object.position[i] + x * across[i] + y * front[i] + z * up[i])
}

pub fn to_local(object: &Object, point: V3) -> V3 {
    let Frame { across, front, up } = frame_of(object);
    let d = sub(point, object.position);
    [dot(d, across), dot(d, front), dot(d, up)]
}

#[derive(Clone, Copy, Debug)]
pub struct ShapeBend {
    pub curvature: f64,
    pub sphere: bool,
    pub arc: Option<V3>,
    pub axis: Option<V3>,
    pub front: V3,
    pub touching: V3,
}

fn sideways(front: V3) -> V3 {
    let level = [front[1], -front[0], 0.0];
    let l = hypot3(level);
    if l > 1e-9 {
        level.map(|v| v / l)
    } else {
        [1.0, 0.0, 0.0]
    }
}

fn outward(bend: &ShapeBend, p: V3) -> (V3, f64) {
    let d = sub(p, bend.touching);
    if !bend.sphere {
        let arc = bend.arc.unwrap_or([0.0; 3]);
        return (arc, dot(d, arc));
    }
    let radial = along(d, &[(bend.front, -dot(d, bend.front))]);
    let rho = hypot3(radial);
    if rho > 1e-12 {
        (radial.map(|v| v / rho), rho)
    } else {
        (sideways(bend.front), 0.0)
    }
}

pub fn bend_of(object: &Object) -> Option<ShapeBend> {
    let bend = object.bend?;
    if bend.angle_degrees == 0.0 || bend.angle_degrees.is_nan() {
        return None;
    }
    let frame = frame_of(object);
    let [w, d, h] = object.size;
    let up_axis = bend.axis == "up";
    if object.kind == "pipe" {
        if d.is_nan() || d <= 0.0 {
            return None;
        }
        let tightest = 1.0 / (TIGHTEST_PIPE_BEND * w / 2.0);
        let turn = bend.angle_degrees * PI / 180.0 / d;
        let curvature = sign(turn) * turn.abs().min(tightest);
        if curvature.abs() < FLAT_CURVATURE {
            return None;
        }
        let towards = if up_axis { frame.across } else { frame.up };
        return Some(ShapeBend {
            curvature,
            sphere: false,
            arc: Some(frame.front),
            axis: Some(if up_axis { frame.up } else { frame.across }),
            front: towards,
            touching: object.position,
        });
    }
    if SPHERE_KINDS.contains(&object.kind) {
        let span = w.max(h);
        if span.is_nan() || span <= 0.0 {
            return None;
        }
        let curvature = bend.angle_degrees * PI / 180.0 / span;
        if curvature.abs() < FLAT_CURVATURE {
            return None;
        }
        return Some(ShapeBend { curvature, sphere: true, arc: None, axis: None, front: frame.front, touching: object.position });
    }
    let span = if up_axis { w } else { h };
    if span.is_nan() || span <= 0.0 {
        return None;
    }
    let curvature = bend.angle_degrees * PI / 180.0 / span;
    if curvature.abs() < FLAT_CURVATURE {
        return None;
    }
    Some(ShapeBend {
        curvature,
        sphere: false,
        arc: Some(if up_axis { frame.across } else { frame.up }),
        axis: Some(if up_axis { frame.up } else { frame.across }),
        front: frame.front,
        touching: object.position,
    })
}

pub fn bent_point(bend: Option<&ShapeBend>, p: V3) -> V3 {
    let Some(bend) = bend else { return p };
    let gap = dot(sub(p, bend.touching), bend.front);
    let (e, s) = outward(bend, p);
    let k = bend.curvature;
    let (sin, cos) = ((k * s).sin(), (k * s).cos());
    along(p, &[(e, sin / k - s - gap * sin), (bend.front, (1.0 - cos) / k - gap + gap * cos)])
}

pub fn bent_normal(bend: &ShapeBend, p: V3) -> V3 {
    let (e, s) = outward(bend, p);
    let (sin, cos) = ((bend.curvature * s).sin(), (bend.curvature * s).cos());
    along([0.0; 3], &[(bend.front, cos), (e, -sin)])
}

pub fn bent_tangent(bend: &ShapeBend, p: V3) -> V3 {
    let (e, s) = outward(bend, p);
    let (sin, cos) = ((bend.curvature * s).sin(), (bend.curvature * s).cos());
    along([0.0; 3], &[(e, cos), (bend.front, sin)])
}

pub fn bent_turn(bend: &ShapeBend, p: V3, w: V3) -> V3 {
    let (e, s) = outward(bend, p);
    let (sin, cos) = ((bend.curvature * s).sin(), (bend.curvature * s).cos());
    let (we, wn) = (dot(w, e), dot(w, bend.front));
    along(w, &[(e, (cos - 1.0) * we - sin * wn), (bend.front, sin * we + (cos - 1.0) * wn)])
}

pub fn flat_point(bend: Option<&ShapeBend>, p: V3) -> V3 {
    let Some(bend) = bend else { return p };
    let k = bend.curvature;
    let sgn = sign(k);
    let centre = along(bend.touching, &[(bend.front, 1.0 / k)]);
    let w = sub(p, centre);
    if bend.sphere {
        let height = -sgn * dot(w, bend.front);
        let side = along(w, &[(bend.front, -dot(w, bend.front))]);
        let width = hypot3(side);
        let rho = width.atan2(height) / k.abs();
        let gap = sgn * (1.0 / k.abs() - hypot3(w));
        let e = if width > 1e-15 { side.map(|v| v / width) } else { sideways(bend.front) };
        return along(bend.touching, &[(e, rho), (bend.front, gap)]);
    }
    let axis = bend.axis.unwrap_or([0.0; 3]);
    let arc = bend.arc.unwrap_or([0.0; 3]);
    let t = dot(sub(p, bend.touching), axis);
    let radial = along(w, &[(axis, -dot(w, axis))]);
    let angle = (sgn * dot(radial, arc)).atan2(-sgn * dot(radial, bend.front));
    let s = angle / k;
    let gap = sgn * (1.0 / k.abs() - hypot3(radial));
    along(bend.touching, &[(arc, s), (bend.front, gap), (axis, t)])
}

pub fn bend_values(bend: Option<&ShapeBend>, face_normal: V3) -> [f64; 8] {
    let Some(bend) = bend else { return [0.0; 8] };
    let s = sign(dot(face_normal, bend.front));
    let towards = if s == 0.0 || s.is_nan() { 1.0 } else { s };
    let arc = bend.arc.unwrap_or([0.0; 3]);
    [bend.curvature * towards, arc[0], arc[1], arc[2], bend.touching[0], bend.touching[1], bend.touching[2], if bend.sphere { 1.0 } else { 0.0 }]
}

pub fn surface_point(object: &Object, local: V3) -> V3 {
    bent_point(bend_of(object).as_ref(), to_world(object, local))
}

pub fn surface_local(object: &Object, point: V3) -> V3 {
    to_local(object, flat_point(bend_of(object).as_ref(), point))
}

pub fn surface_frame(object: &Object, local: V3) -> Frame {
    let frame = frame_of(object);
    let Some(bend) = bend_of(object) else { return frame };
    let flat = to_world(object, local);
    if bend.sphere {
        return Frame { across: bent_turn(&bend, flat, frame.across), front: bent_normal(&bend, flat), up: bent_turn(&bend, flat, frame.up) };
    }
    let tangent = bent_tangent(&bend, flat);
    let normal = bent_normal(&bend, flat);
    let up_axis = object.bend.is_some_and(|b| b.axis == "up");
    if object.kind == "pipe" {
        return if up_axis { Frame { across: normal, front: tangent, up: frame.up } } else { Frame { across: frame.across, front: tangent, up: normal } };
    }
    if up_axis {
        Frame { across: tangent, front: normal, up: frame.up }
    } else {
        Frame { across: frame.across, front: normal, up: tangent }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FacePattern {
    Solid,
    Grid([f64; 6]),
    Opening([f64; 6]),
}

impl FacePattern {
    pub fn code(&self) -> f64 {
        match self {
            FacePattern::Solid => 0.0,
            FacePattern::Grid(_) => 1.0,
            FacePattern::Opening(_) => 2.0,
        }
    }

    pub fn values(&self) -> [f64; 6] {
        match self {
            FacePattern::Solid => [0.0; 6],
            FacePattern::Grid(v) | FacePattern::Opening(v) => *v,
        }
    }
}

fn rectangle_xz(cx: f64, y: f64, cz: f64, w: f64, h: f64) -> Vec<V3> {
    vec![[cx - w / 2.0, y, cz - h / 2.0], [cx + w / 2.0, y, cz - h / 2.0], [cx + w / 2.0, y, cz + h / 2.0], [cx - w / 2.0, y, cz + h / 2.0]]
}

fn rectangle_yz(x: f64, cy: f64, cz: f64, d: f64, h: f64) -> Vec<V3> {
    vec![[x, cy - d / 2.0, cz - h / 2.0], [x, cy + d / 2.0, cz - h / 2.0], [x, cy + d / 2.0, cz + h / 2.0], [x, cy - d / 2.0, cz + h / 2.0]]
}

fn rectangle_xy(cx: f64, cy: f64, z: f64, w: f64, d: f64) -> Vec<V3> {
    vec![[cx - w / 2.0, cy - d / 2.0, z], [cx + w / 2.0, cy - d / 2.0, z], [cx + w / 2.0, cy + d / 2.0, z], [cx - w / 2.0, cy + d / 2.0, z]]
}

fn plane_pattern(object: &Object) -> FacePattern {
    let [w, _, h] = object.size;
    if let (Some(p), "grating") = (&object.pattern, object.kind) {
        return FacePattern::Grid([p.period_metres[0], p.period_metres[1], p.bar_metres[0], p.bar_metres[1], p.thickness_metres, 0.0]);
    }
    if let (Some(opening), "wall") = (&object.opening, object.kind) {
        let o = fitted_opening(*opening, object.size);
        return FacePattern::Opening([
            w / 2.0 + o.offset_metres[0],
            h / 2.0 + o.offset_metres[1],
            o.size_metres[0] / 2.0,
            o.size_metres[1] / 2.0,
            if o.shape == "round" { 1.0 } else { 0.0 },
            0.0,
        ]);
    }
    FacePattern::Solid
}

fn local_faces(object: &Object) -> Vec<(Vec<V3>, FacePattern)> {
    let [w, d, h] = object.size;
    let solid = |faces: Vec<Vec<V3>>| faces.into_iter().map(|f| (f, FacePattern::Solid)).collect();
    match object.kind {
        "plane" | "grating" | "wall" => vec![(rectangle_xz(0.0, 0.0, 0.0, w, h), plane_pattern(object))],
        "disc" => vec![(
            (0..DISC_SIDES)
                .map(|i| {
                    let a = 2.0 * PI * i as f64 / DISC_SIDES as f64;
                    [w / 2.0 * a.cos(), 0.0, h / 2.0 * a.sin()]
                })
                .collect(),
            FacePattern::Solid,
        )],
        "box" => solid(vec![
            rectangle_xz(0.0, d / 2.0, 0.0, w, h),
            rectangle_xz(0.0, -d / 2.0, 0.0, w, h),
            rectangle_yz(w / 2.0, 0.0, 0.0, d, h),
            rectangle_yz(-w / 2.0, 0.0, 0.0, d, h),
            rectangle_xy(0.0, 0.0, h / 2.0, w, d),
            rectangle_xy(0.0, 0.0, -h / 2.0, w, d),
        ]),
        "corner" => solid(vec![rectangle_xy(w / 2.0, d / 2.0, 0.0, w, d), rectangle_yz(0.0, d / 2.0, h / 2.0, d, h), rectangle_xz(w / 2.0, 0.0, h / 2.0, w, h)]),
        "recess" => {
            let m = RECESS_MARGIN_METRES;
            let mut faces = vec![(rectangle_xz(0.0, 0.0, 0.0, w + 2.0 * m, h + 2.0 * m), FacePattern::Opening([w / 2.0 + m, h / 2.0 + m, w / 2.0, h / 2.0, 0.0, 0.0]))];
            faces.extend(solid(vec![
                rectangle_xz(0.0, -d, 0.0, w, h),
                rectangle_yz(-w / 2.0, -d / 2.0, 0.0, d, h),
                rectangle_yz(w / 2.0, -d / 2.0, 0.0, d, h),
                rectangle_xy(0.0, -d / 2.0, h / 2.0, w, d),
                rectangle_xy(0.0, -d / 2.0, -h / 2.0, w, d),
            ]));
            faces
        }
        "pipe" => (0..PIPE_SIDES)
            .map(|i| {
                let [x0, z0] = pipe_ring(w / 2.0, i);
                let [x1, z1] = pipe_ring(w / 2.0, i + 1);
                (vec![[x0, -d / 2.0, z0], [x0, d / 2.0, z0], [x1, d / 2.0, z1], [x1, -d / 2.0, z1]], FacePattern::Solid)
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn pipe_ring(radius: f64, i: usize) -> [f64; 2] {
    let a = 2.0 * PI * i as f64 / PIPE_SIDES as f64;
    [radius * a.cos(), radius * a.sin()]
}

pub fn seed_of(id: &str) -> f64 {
    let mut hash: u32 = 2166136261;
    let mut units = [0u16; 2];
    for ch in id.chars() {
        let code = ch.encode_utf16(&mut units)[0] as u32;
        hash = (hash ^ code).wrapping_mul(16777619);
    }
    (hash % 1000003) as f64
}

fn pipe_relief_direction(object: &Object, vertices: &[V3]) -> V3 {
    let axial = normalize(sub(vertices[1], vertices[0]));
    let edge = sub(vertices[3], vertices[0]);
    let around = normalize(sub(edge, axial.map(|v| v * dot(edge, axial))));
    let a = object.relief.direction_degrees * PI / 180.0;
    std::array::from_fn(|i| around[i] * a.cos() + axial[i] * a.sin())
}

pub fn relief_values(object: &Object, shift: f64, face_direction: Option<V3>) -> [f64; 13] {
    let r = &object.relief;
    let Frame { across, up, .. } = frame_of(object);
    let a = r.direction_degrees * PI / 180.0;
    let direction = face_direction.unwrap_or_else(|| std::array::from_fn(|i| across[i] * a.cos() + up[i] * a.sin()));
    let shape = RELIEF_SHAPES.iter().position(|&s| s == r.shape).unwrap_or(0) as f64;
    [
        shape,
        if r.shape == "flat" { 0.0 } else { r.height_metres },
        r.period_metres,
        direction[0],
        direction[1],
        direction[2],
        object.position[0],
        object.position[1],
        object.position[2],
        r.offset_metres + shift,
        r.rough_metres,
        r.grain_metres,
        seed_of(&object.id),
    ]
}

pub struct Face {
    pub vertices: Vec<V3>,
    pub bend: Option<ShapeBend>,
    pub material: &'static str,
    pub strength: f64,
    pub pattern: FacePattern,
    pub relief: [f64; 13],
}

pub fn expand_object(object: &Object) -> Vec<Face> {
    let bend = bend_of(object);
    if let (Some(bend), "pipe") = (bend, object.kind) {
        let [w, d, _] = object.size;
        let segments = ((bend.curvature * d).abs() / PIPE_BEND_SEGMENT_RADIANS).ceil().max(1.0) as usize;
        let mut faces = Vec::new();
        for j in 0..segments {
            let y0 = -d / 2.0 + d * j as f64 / segments as f64;
            let y1 = -d / 2.0 + d * (j + 1) as f64 / segments as f64;
            for i in 0..PIPE_SIDES {
                let [x0, z0] = pipe_ring(w / 2.0, i);
                let [x1, z1] = pipe_ring(w / 2.0, i + 1);
                let vertices: Vec<V3> = [[x0, y0, z0], [x0, y1, z0], [x1, y1, z1], [x1, y0, z1]].iter().map(|&p| bent_point(Some(&bend), to_world(object, p))).collect();
                let relief = relief_values(object, 0.0, Some(pipe_relief_direction(object, &vertices)));
                faces.push(Face { vertices, bend: None, material: object.material, strength: object.strength, pattern: FacePattern::Solid, relief });
            }
        }
        return faces;
    }
    local_faces(object)
        .into_iter()
        .map(|(local, pattern)| {
            let vertices: Vec<V3> = local.iter().map(|&p| to_world(object, p)).collect();
            let direction = (object.kind == "pipe").then(|| pipe_relief_direction(object, &vertices));
            let relief = relief_values(object, 0.0, direction);
            Face { vertices, bend, material: object.material, strength: object.strength, pattern, relief }
        })
        .collect()
}

pub fn face_normal(vertices: &[V3]) -> V3 {
    let mut n = [0.0; 3];
    for (i, a) in vertices.iter().enumerate() {
        let b = vertices[(i + 1) % vertices.len()];
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    normalize(n)
}

pub fn anchor(object: &Object) -> V3 {
    if object.kind == "box" || object.kind == "pipe" {
        [0.0, object.size[1] / 2.0, 0.0]
    } else {
        [0.0; 3]
    }
}

pub fn reference_object<'a>(space: &'a Space, point: &Point) -> Option<&'a Object> {
    let id = point.reference.as_deref().filter(|id| !id.is_empty())?;
    space.objects.iter().find(|o| o.id == id)
}

pub fn point_position(space: &Space, point: &Point) -> V3 {
    let Some(object) = reference_object(space, point) else { return point.position };
    let base = anchor(object);
    surface_point(object, std::array::from_fn(|i| point.offset[i] + base[i]))
}

pub fn point_frame(object: &Object, point: &Point) -> Frame {
    let base = anchor(object);
    surface_frame(object, std::array::from_fn(|i| point.offset[i] + base[i]))
}

pub fn aim_vector(space: &Space, point: &Point, aim: &crate::space::Aim) -> V3 {
    let az = aim.azimuth_degrees * PI / 180.0;
    let el = aim.elevation_degrees * PI / 180.0;
    let local = [el.cos() * az.sin(), el.cos() * az.cos(), el.sin()];
    let Some(object) = reference_object(space, point) else { return local };
    let Frame { across, front, up } = point_frame(object, point);
    std::array::from_fn(|i| local[0] * across[i] + local[1] * front[i] + local[2] * up[i])
}

pub fn place_points(space: &mut Space) {
    let positions: Vec<V3> = space
        .microphones
        .iter()
        .map(|m| &m.point)
        .chain(space.rigs.iter().map(|r| &r.point))
        .chain(std::iter::once(&space.source.point))
        .map(|p| point_position(space, p))
        .collect();
    let mut at = positions.into_iter();
    for m in &mut space.microphones {
        m.point.position = at.next().unwrap_or(m.point.position);
    }
    for r in &mut space.rigs {
        r.point.position = at.next().unwrap_or(r.point.position);
    }
    space.source.point.position = at.next().unwrap_or(space.source.point.position);
}

#[derive(Clone, Copy, Debug)]
pub struct PipeBendLine {
    pub centre: V3,
    pub axis: V3,
}

#[derive(Clone, Copy, Debug)]
pub struct PipeAround {
    pub object: usize,
    pub mouth_a: V3,
    pub mouth_b: V3,
    pub bend: Option<PipeBendLine>,
    pub radius: f64,
}

pub fn pipe_centreline(object: &Object) -> (V3, V3, Option<PipeBendLine>) {
    let d = object.size[1];
    let bend = bend_of(object);
    let mouth_a = surface_point(object, [0.0, -d / 2.0, 0.0]);
    let mouth_b = surface_point(object, [0.0, d / 2.0, 0.0]);
    let Some(bend) = bend else { return (mouth_a, mouth_b, None) };
    let centre: V3 = std::array::from_fn(|i| bend.touching[i] + bend.front[i] / bend.curvature);
    let travel = bent_tangent(&bend, to_world(object, [0.0, -d / 2.0, 0.0]));
    let radial = sub(mouth_a, centre);
    let axis = cross(radial, travel);
    let n = hypot3(axis);
    (mouth_a, mouth_b, Some(PipeBendLine { centre, axis: axis.map(|v| v / n) }))
}

// Class C: smoothstep blend through the mouth.
pub fn pipe_share(space: &Space, point: V3) -> Option<(PipeAround, f64)> {
    for (index, object) in space.objects.iter().enumerate() {
        if object.kind != "pipe" || object.strength.is_nan() || object.strength <= 0.0 {
            continue;
        }
        let [x, y, z] = surface_local(object, point);
        let [diameter, length, _] = object.size;
        let radius = diameter / 2.0;
        let depth = length / 2.0 - y.abs();
        if x.hypot(z) >= radius || depth <= -radius {
            continue;
        }
        let t = ((depth + radius) / (2.0 * radius)).clamp(0.0, 1.0);
        let (mouth_a, mouth_b, bend) = pipe_centreline(object);
        return Some((PipeAround { object: index, mouth_a, mouth_b, bend, radius }, t * t * (3.0 - 2.0 * t)));
    }
    None
}

#[derive(Clone, Copy, Debug)]
pub struct Ear {
    pub axis: V3,
    pub radius: f64,
    pub measured: bool,
    pub front: V3,
    pub up: V3,
}

#[derive(Clone, Debug)]
pub struct Listener {
    pub name: String,
    pub position: V3,
    pub pattern: f64,
    pub facing: V3,
    pub pan: [f64; 2],
    pub ear: Option<Ear>,
}

pub fn pattern_value(name: &str) -> f64 {
    crate::space::PATTERNS.iter().find(|p| p.0 == name).map_or(1.0, |p| p.1)
}

fn head_width(rig: &Rig) -> f64 {
    if rig.head == Some("measured") {
        MEASURED_HEAD_WIDTH_METRES
    } else {
        rig.spacing_metres
    }
}

pub fn rig_frame(space: &Space, rig: &Rig) -> Frame {
    let front = aim_vector(space, &rig.point, &rig.aim);
    let mut right = normalize(cross(front, [0.0, 0.0, 1.0]));
    if hypot3(right) < 0.5 {
        right = [1.0, 0.0, 0.0];
    }
    Frame { across: right, front, up: cross(right, front) }
}

pub fn rig_members(space: &Space, rig: &Rig) -> Vec<Listener> {
    let centre = point_position(space, &rig.point);
    let Frame { across: right, front, up } = rig_frame(space, rig);
    let left = right.map(|v| -v);
    let half = (if rig.kind.id == "binaural" { head_width(rig) } else { rig.spacing_metres }) / 2.0;
    let place = |x: f64, y: f64| -> V3 { std::array::from_fn(|i| centre[i] + right[i] * x + front[i] * y) };
    let turn = |degrees: f64| -> V3 {
        let r = degrees * PI / 180.0;
        std::array::from_fn(|i| front[i] * r.cos() + right[i] * r.sin())
    };
    let member = |name: &str, position: V3, pattern: f64, facing: V3, pan: [f64; 2]| Listener { name: name.into(), position, pattern, facing, pan, ear: None };
    let pair = |pattern: &str, angle: f64| {
        let p = pattern_value(pattern);
        vec![member("L", place(-half, 0.0), p, turn(-angle / 2.0), [1.0, 0.0]), member("R", place(half, 0.0), p, turn(angle / 2.0), [0.0, 1.0])]
    };
    match rig.kind.id {
        "ms" => vec![member("M", centre, pattern_value("cardioid"), front, [HALF, HALF]), member("S", centre, pattern_value("figure8"), left, [HALF, -HALF])],
        "decca" => {
            let mut list = pair(rig.kind.pattern, rig.angle_degrees);
            list.push(member("C", place(0.0, rig.spacing_metres * 0.75), pattern_value(rig.kind.pattern), front, [HALF, HALF]));
            list
        }
        "binaural" => {
            let ear = |axis: V3| Ear { axis, radius: half.max(0.05), measured: rig.head == Some("measured"), front, up };
            vec![
                Listener { name: "L".into(), position: centre, pattern: 1.0, facing: left, pan: [1.0, 0.0], ear: Some(ear(left)) },
                Listener { name: "R".into(), position: centre, pattern: 1.0, facing: right, pan: [0.0, 1.0], ear: Some(ear(right)) },
            ]
        }
        _ => pair(rig.kind.pattern, rig.angle_degrees),
    }
}

pub struct Material {
    pub id: &'static str,
    pub label: &'static str,
    pub absorption: Absorption,
    pub reliability: &'static str,
}

pub enum Absorption {
    Table([f64; 6]),
    Impedance(f64),
    Ground(f64),
}

const AIR_IMPEDANCE: f64 = 413.0;
const TABLE_BANDS_HZ: [f64; 6] = [125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0];

// RoomTreat absorption table (CC BY 4.0); 63 Hz and 8 kHz repeat the nearest band. Grounds: Delany–Bazley, flow resistivity in kPa·s/m².
pub const MATERIALS: [Material; 20] = [
    Material { id: "rigid", label: "Rigid", absorption: Absorption::Table([0.0; 6]), reliability: "A" },
    Material { id: "water", label: "Water", absorption: Absorption::Impedance(1.48e6), reliability: "A" },
    Material { id: "metal", label: "Metal", absorption: Absorption::Impedance(2700.0 * 6320.0), reliability: "A" },
    Material { id: "concrete", label: "Concrete", absorption: Absorption::Table([0.01, 0.01, 0.01, 0.02, 0.02, 0.02]), reliability: "B" },
    Material { id: "tile", label: "Glazed tile", absorption: Absorption::Table([0.01, 0.01, 0.01, 0.01, 0.02, 0.02]), reliability: "B" },
    Material { id: "plaster", label: "Plaster", absorption: Absorption::Table([0.01, 0.01, 0.02, 0.03, 0.04, 0.05]), reliability: "B" },
    Material { id: "brick", label: "Brick", absorption: Absorption::Table([0.03, 0.03, 0.03, 0.04, 0.05, 0.07]), reliability: "B" },
    Material { id: "glass", label: "Plate glass", absorption: Absorption::Table([0.18, 0.06, 0.04, 0.03, 0.02, 0.02]), reliability: "B" },
    Material { id: "gypsum", label: "Gypsum board", absorption: Absorption::Table([0.29, 0.1, 0.05, 0.04, 0.07, 0.09]), reliability: "B" },
    Material { id: "woodFloor", label: "Wood floor", absorption: Absorption::Table([0.15, 0.11, 0.1, 0.07, 0.06, 0.07]), reliability: "B" },
    Material { id: "woodPanel", label: "Wood panelling", absorption: Absorption::Table([0.3, 0.25, 0.2, 0.17, 0.15, 0.1]), reliability: "B" },
    Material { id: "carpet", label: "Carpet", absorption: Absorption::Table([0.05, 0.15, 0.35, 0.55, 0.65, 0.7]), reliability: "B" },
    Material { id: "curtain", label: "Heavy curtain", absorption: Absorption::Table([0.14, 0.35, 0.55, 0.72, 0.7, 0.65]), reliability: "B" },
    Material { id: "foam", label: "Acoustic foam 50 mm", absorption: Absorption::Table([0.08, 0.25, 0.6, 0.9, 0.95, 0.98]), reliability: "B" },
    Material { id: "mineralWool", label: "Mineral wool 50 mm", absorption: Absorption::Table([0.15, 0.55, 0.9, 1.0, 1.0, 1.0]), reliability: "B" },
    Material { id: "stonyGround", label: "Stony ground", absorption: Absorption::Ground(550.0), reliability: "B" },
    Material { id: "soil", label: "Soil", absorption: Absorption::Ground(278.0), reliability: "B" },
    Material { id: "grass", label: "Grass", absorption: Absorption::Ground(212.5), reliability: "B" },
    Material { id: "forestFloor", label: "Forest floor", absorption: Absorption::Ground(50.0), reliability: "B" },
    Material { id: "snow", label: "Snow", absorption: Absorption::Ground(20.0), reliability: "B" },
];

fn ground_reflection_magnitude(frequency_hz: f64, flow_resistivity: f64) -> f64 {
    let ratio = frequency_hz / flow_resistivity;
    let (z_re, z_im) = (1.0 + 9.08 * ratio.powf(-0.75), 11.9 * ratio.powf(-0.73));
    let z_scale = z_re * z_re + z_im * z_im;
    let beta_re = z_re / z_scale;
    let beta_im = -z_im / z_scale;
    let sum_re = 1.0 + beta_re;
    let difference_re = 1.0 - beta_re;
    let sum_scale = sum_re * sum_re + beta_im * beta_im;
    let plane_re = (difference_re * sum_re - beta_im * beta_im) / sum_scale;
    let plane_im = (-beta_im * sum_re - difference_re * beta_im) / sum_scale;
    plane_re.hypot(plane_im)
}

pub fn absorption(material: &Material) -> [f64; 6] {
    match material.absorption {
        Absorption::Table(list) => list,
        Absorption::Impedance(z) => {
            let r = (z - AIR_IMPEDANCE) / (z + AIR_IMPEDANCE);
            [1.0 - r.powi(2); 6]
        }
        Absorption::Ground(sigma) => TABLE_BANDS_HZ.map(|hz| 1.0 - ground_reflection_magnitude(hz, sigma).powi(2)),
    }
}

pub fn reflection_bands(material_id: &str) -> [f64; 8] {
    let material = MATERIALS.iter().find(|m| m.id == material_id).or_else(|| MATERIALS.iter().find(|m| m.id == crate::space::DEFAULT_MATERIAL)).expect("the default material");
    let six = absorption(material).map(|alpha| (0.0f64).max(1.0 - alpha.min(1.0)).sqrt());
    [six[0], six[0], six[1], six[2], six[3], six[4], six[5], six[5]]
}
