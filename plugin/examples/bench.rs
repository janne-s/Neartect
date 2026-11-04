use neartect_plugin::convolver::{Convolver, Filter};
use std::time::Instant;

const RATE: f32 = 48000.0;
const SECONDS_RUN: f32 = 20.0;

fn noise(n: usize, seed: &mut u32) -> Vec<f32> {
    (0..n)
        .map(|_| {
            *seed ^= *seed << 13;
            *seed ^= *seed >> 17;
            *seed ^= *seed << 5;
            *seed as f32 / u32::MAX as f32 - 0.5
        })
        .collect()
}

fn main() {
    let mut seed = 1;
    let input = noise((SECONDS_RUN * RATE) as usize, &mut seed);
    for ir_seconds in [0.08, 0.2, 0.5, 2.0] {
        for gliding in [false, true] {
            let n = (ir_seconds * RATE) as usize;
            let filter = || Box::new(Filter::new(RATE, &noise(n, &mut 7), &noise(n, &mut 9)));
            let mut convolver = Convolver::new(RATE);
            convolver.set_now(filter());
            let mut sink = 0.0;
            let started = Instant::now();
            for (i, &x) in input.iter().enumerate() {
                if gliding && i % 2048 == 0 {
                    convolver.offer(filter());
                    while convolver.take_retired().is_some() {}
                }
                let [l, r] = convolver.process(x);
                sink += l + r;
            }
            let seconds = started.elapsed().as_secs_f64();
            let load = if gliding { "always gliding" } else { "steady" };
            println!("IR {ir_seconds:>4} s {load:<14} {:>6.2} % of one core  ({sink:.1})", 100.0 * seconds / SECONDS_RUN as f64);
        }
    }
}
