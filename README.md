# Neartect: Design the near space

Neartect designs what a microphone's surroundings do to sound. Build walls,
floors, niches, pipes, and gratings around a microphone, give each a material, place
the microphone anywhere among them, even a millimetre from a surface, and hear
the boundary's lift, the comb of a nearby wall, a corner's bass, and the
chirp of a grating. Export the near filter or the filtered sound as WAV. Made
for sound design, built on physics.

Neartect is part of the Tect family with Boomtect, Rumbletect, and Echotect.
It is in early development.

## How it works

Sound arrives as a plane wave from a chosen direction. Each surface scatters it
as a sum of small surface elements in the Kirchhoff approximation, so a
surface's size, its free edges, and the shadow behind it come from the same
sum. Where two surfaces meet at a right angle in a corner the microphone can
see, the edge is continued, so the corner keeps its exact image solution. Higher orders are specular images with a
finite-size high-pass; a surface with relief keeps its lobes in the second
order through a flat surface, joined or free. The filter is computed by a Rust core compiled to
WebAssembly and heard live through a crossfading convolver.

- **Objects** — planes, discs, walls with a round or square opening, gratings,
  boxes, corners, niches, and pipes, added from a menu of shapes. Each has a
  material from published absorption tables or a porous ground model, and a
  creative strength (1 is physical).
- **Bend** — a plane, wall, or grating bends into an arc about its up or
  across axis, a disc into a bowl or a dome, concave or convex at the front,
  and a pipe bends along its length. A concave wall or bowl gathers its
  reflection towards its focus, a convex one spreads it; a microphone bound to
  it keeps its gap along the arc.
- **Situations** — start from a microphone by a wall, in a corner, in a niche,
  under a grating, at a pipe mouth, in a doorway, by a curved wall, by a corrugated wall, over
  moving waves, or over water.
- **Handles** — hover or select an object to move it along east, north, and
  up (or its own across, front, and up with the cube button), resize it from
  its faces, and turn or tilt it a full circle on its rings; values snap to
  the grid and to 5°, and Shift moves freely. On a face, Shift resizes in
  proportion, Alt about the centre, and Cmd or Ctrl freely; bound microphones
  keep their place in the object. The magnet snaps an object's sides to other
  objects. Everything else dims while you work. Exact values sit in Details,
  and Undo steps back.
- **Binding** — a microphone, layout, or point source bound to an object moves
  with it, and the target button centres it on the object (a wall on its
  opening, at the same gap).
- **Scrolling** — the mouse/hand button chooses whether scrolling zooms or
  pans; a pinch always zooms.
- **Selecting several** — arm the area tool and drag a rectangle, or
  Shift-click to add or remove one. The selection moves along east, north,
  and up and turns on its ring as one; microphones bound to a selected object
  follow it. Delete removes the selection.
- **Situations** add their objects to those you have built and keep your
  microphones and point source, moving them with the first microphone.
- **Relief** — any object's faces can be waves, arches, scallops, or an egg
  crate at a set height, period, and direction, plus random roughness with a
  height and grain. Each piece of the surface moves to its height and tilts to
  its slope, so rough surfaces lose their coherent treble and scatter it, and
  repeating shapes send their own lobes. A speed moves the relief while the
  preview plays (moving water), and each new filter glides in over the time
  since the last one. The view shades the relief by height, lighter where it
  rises and darker where it sinks, with true-scale profiles across it.
- **Gratings** — bars or slats at a set period, width, and thickness; each bar
  reflects and shades on its own, and fine gratings let bass through.
- **Microphones** — up to eight, each bound to an object of your choice and
  moving with it, or free in the world. Across, gap, and up are signed (a
  negative gap is behind the face or inside the object) on sliders from 1 mm
  to 5 m. Microphones keep their time and level differences; the preview
  spreads them from left to right, and export writes one channel each. Each
  has a pickup pattern — omni, wide cardioid, cardioid, supercardioid,
  hypercardioid, or figure of eight — aimed on its ring or sliders; a new
  pattern starts aimed at the source. Directional patterns have the proximity
  effect: a near source or a near reflection lifts their bass.
- **Layouts** — XY, Blumlein, mid-side, ORTF, NOS, spaced pair, Decca tree,
  and a head with two ears, either the measured KU100 (SADIE II) or a
  spherical model of adjustable width; each is one element that binds, moves, and turns
  as a whole, with its spacing and angle on sliders. Mid-side decodes to left
  and right in the preview; export writes every capsule, or a stereo mix.
  Microphones and layouts share one list: any of them can be removed, and a
  layout added to a lone microphone takes its place. A layout shows its front
  as a bold arrow, coincident capsules fan out with their names at the tips,
  and a head is drawn with its nose and ears. A selected microphone or layout
  can switch to another layout in place, keeping its spot, binding, and aim;
  its aim turns on a level ring and tilts on an upright one.
- **Inside a pipe** — the microphone hears the pipe as a waveguide: its
  resonances at the end-corrected length, the open ends' radiation, the
  walls' losses, and in wide pipes the cross modes, which depend on where
  across the pipe the microphone sits.
- **Source** — far away, as a plane wave from a direction (drag the arrow's
  tail), or a point in the scene that can be bound to an object like a
  microphone. A point source's level falls with distance (1 m reference)
  unless 1/r is off.
- **Listening** — pink noise, clicks, a sweep, or your own file, with a
  free-field comparison.
- **Quality** — relief detail and an element limit for surfaces with relief,
  and a lower detail while relief moves or something is dragged; the
  computation time shows in the corner of the view.
- **Plots** — the near filter and its response relative to the free field.
- **Grid** — a fixed metre grid on the ground, with tenths when close; the
  grid button hides it.
- **Export** — the impulse response (IR) for a convolution plugin, stereo as heard or one channel per capsule and optionally peak-normalised, or the filtered signal, as 32-bit float WAV, and
  the space as JSON.

## Reliability

Specular reflection from rigid surfaces is class A for a plane wave and a point source alike, absorption tables and the
finite-size correction class B, the grating's low-frequency blend, the
continuation of joined edges at other than right angles, and the strength
control class C. Both heads are class B: the measured one as minimum-phase
responses with delays interpolated between directions, the sphere as Brown
and Duda's model. A bent surface's first order is class B (Kirchhoff on the arc or sphere); its higher
orders take the specular points on the arc or sphere with the ray-tube gain (class B),
limited near a focus (class C), and paths over two bent surfaces are left out.
A bent pipe's main mode runs as in Cummings' equivalent rectangular bend
(class B): a little faster than in a straight pipe for long waves, slower
above half the first cross-mode cutoff, its wave admittance shifting with it;
the bend's own reflection comes only through that admittance at the mouths,
and its cross modes run as in a straight pipe without mixing (class C). The pipe interior is class B, with its cross modes' coupling at the mouth
and the blend between outside and inside within a radius of the mouth
class C. Not modelled: the
microphone capsule's own size, room modes
of closed small spaces, edge diffraction into the shadow beyond the Kirchhoff
approximation, angle-dependent absorption, and surface vibration. A panel
small against the wavelength takes a low-frequency edge correction: exact
in its limits (Lamb's disc), class C between.

## Development

`npm start` serves the app at http://localhost:8003/. `npm test` runs the
JavaScript tests, `npm run test:core` the Rust core tests, and
`npm run build:core` rebuilds `src/core/` (Rust with the
`wasm32-unknown-unknown` target, `wasm-bindgen`, and Binaryen's `wasm-opt`).
`node tools/bench.mjs` times the core over the situations and can save and
compare filters. `src/shared/` is a synced copy of tect-shared.

## License

MIT. Absorption coefficients: RoomTreat, CC BY 4.0. Measured head: SADIE II
(KU100), © 2018 University of York, Apache 2.0, derived table in `core/data/`
with its license (Armstrong et al. 2018, Applied Sciences 8(11), 2029).
