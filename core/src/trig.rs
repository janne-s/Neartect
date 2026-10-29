// Sine and cosine together, with fdlibm's kernels and a three-part
// Cody–Waite reduction. WebAssembly has no trigonometric instructions, and
// the library's separate sin and cos each reduce the argument again; this
// also gives native and WebAssembly builds the same result.

const LARGEST_REDUCED: f64 = 823_550.0;
const PIO2_1: f64 = 1.570_796_326_734_125_6;
const PIO2_2: f64 = 6.077_100_506_303_966e-11;
const PIO2_2T: f64 = 2.022_266_248_795_950_6e-21;
const S: [f64; 6] =
    [-1.666_666_666_666_663_2e-1, 8.333_333_333_322_49e-3, -1.984_126_982_985_795e-4, 2.755_731_370_707_007e-6, -2.505_076_025_340_686_3e-8, 1.589_690_995_211_55e-10];
const C: [f64; 6] =
    [4.166_666_666_666_66e-2, -1.388_888_888_887_411e-3, 2.480_158_728_947_673e-5, -2.755_731_435_139_066_3e-7, 2.087_572_321_298_175e-9, -1.135_964_755_778_819_5e-11];

fn kernel_sin(x: f64) -> f64 {
    let z = x * x;
    x + x * z * (S[0] + z * (S[1] + z * (S[2] + z * (S[3] + z * (S[4] + z * S[5])))))
}

fn kernel_cos(x: f64) -> f64 {
    let z = x * x;
    let r = z * (C[0] + z * (C[1] + z * (C[2] + z * (C[3] + z * (C[4] + z * C[5])))));
    let half = 0.5 * z;
    let w = 1.0 - half;
    w + (((1.0 - w) - half) + z * r)
}

pub fn sin_cos(x: f64) -> (f64, f64) {
    if x.is_nan() || x.abs() >= LARGEST_REDUCED {
        return x.sin_cos();
    }
    let n = (x * std::f64::consts::FRAC_2_PI).round_ties_even();
    let r = ((x - n * PIO2_1) - n * PIO2_2) - n * PIO2_2T;
    let (s, c) = (kernel_sin(r), kernel_cos(r));
    match (n as i64).rem_euclid(4) {
        0 => (s, c),
        1 => (c, -s),
        2 => (-s, -c),
        _ => (-c, s),
    }
}

pub fn sin(x: f64) -> f64 {
    sin_cos(x).0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agrees_with_the_library_to_a_few_units_in_the_last_place() {
        let mut worst: f64 = 0.0;
        let mut x = -2000.0;
        while x < 2000.0 {
            let (s, c) = sin_cos(x);
            worst = worst.max((s - x.sin()).abs()).max((c - x.cos()).abs());
            x += 0.000_713_7;
        }
        assert!(worst < 5e-16, "worst {worst:e}");
        assert_eq!(sin_cos(1e9), 1e9_f64.sin_cos());
    }
}
