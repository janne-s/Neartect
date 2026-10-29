# Neartect: Design the near space

Neartect designs what a microphone's surroundings do to sound. Build walls,
floors, panels, and gratings around a microphone, give each a material, place
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
sum. Edges where two surfaces meet are continued, so a right-angled corner
keeps its exact image solution. Higher orders are specular images with a
finite-size high-pass. The filter is computed by a Rust core compiled to
WebAssembly and heard live through a crossfading convolver.

- **Surfaces** — rectangles of any size, facing, and tilt, with a material
  from published absorption tables and a creative strength (1 is physical).
- **Gratings** — bars or slats at a set period, width, and thickness; each bar
  reflects and shades on its own, and fine gratings let bass through.
- **Microphone** — free placement, with the gap to the nearest surface on a
  slider from 1 mm to 3 m.
- **Arrival** — the direction the sound comes from.
- **Listening** — pink noise, clicks, a sweep, or your own file, with a
  free-field comparison.
- **Plots** — the near filter and its response relative to the free field.
- **Export** — the near filter or the filtered signal as 32-bit float WAV, and
  the space as JSON.

## Reliability

Specular reflection from rigid surfaces is class A, absorption tables and the
finite-size correction class B, the grating's low-frequency blend, the
continuation of joined edges at other than right angles, and the strength
control class C. Not modelled: the microphone capsule's own size, room modes
of closed small spaces, edge diffraction into the shadow beyond the Kirchhoff
approximation, angle-dependent absorption, and surface vibration.

## Development

`npm start` serves the app at http://localhost:8003/. `npm test` runs the
JavaScript tests, `npm run test:core` the Rust core tests, and
`npm run build:core` rebuilds `src/core/` (Rust with the
`wasm32-unknown-unknown` target and `wasm-bindgen`). `src/shared/` is a synced
copy of tect-shared.

## License

MIT. Absorption coefficients: RoomTreat, CC BY 4.0.
