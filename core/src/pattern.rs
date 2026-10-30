// A grating is a mask on the surface: bars of a stated width repeat at a
// stated period along the surface's two axes (a period of zero leaves that
// axis without bars, which makes slats). The Kirchhoff sum over the bars gives
// repetition pitch, chirps, and grating lobes. Where the period is much
// shorter than the wavelength the sheet is treated as a perforated mass layer
// (Crandall/Maa form, class B pending the original), blended over one octave
// below half a wavelength per period (class C).

use std::f64::consts::PI;

const AIR_DENSITY: f64 = 1.2;
const END_CORRECTION_PER_RADIUS: f64 = 0.85;

#[derive(Clone, Copy, Debug)]
pub enum Pattern {
    Solid,
    Grid { period: [f64; 2], bar: [f64; 2], thickness: f64 },
    Opening { centre: [f64; 2], half: [f64; 2], round: bool },
}

fn in_opening(q: [f64; 2], centre: [f64; 2], half: [f64; 2], round: bool) -> bool {
    let u = (q[0] - centre[0]) / half[0];
    let v = (q[1] - centre[1]) / half[1];
    if round {
        u * u + v * v < 1.0
    } else {
        u.abs() < 1.0 && v.abs() < 1.0
    }
}

fn near_opening_edge(q: [f64; 2], size: f64, centre: [f64; 2], half: [f64; 2], round: bool) -> bool {
    let reach = size * std::f64::consts::FRAC_1_SQRT_2;
    let du = (q[0] - centre[0]).abs();
    let dv = (q[1] - centre[1]).abs();
    if round {
        let radius = half[0].min(half[1]);
        let stretch = half[0].max(half[1]) - radius;
        let r = ((du * radius / half[0]).powi(2) + (dv * radius / half[1]).powi(2)).sqrt();
        (r - radius).abs() < reach + stretch
    } else {
        let inside_u = du < half[0] + reach;
        let inside_v = dv < half[1] + reach;
        (inside_u && inside_v) && ((du - half[0]).abs() < reach || (dv - half[1]).abs() < reach)
    }
}

fn on_bar(x: f64, period: f64, bar: f64) -> bool {
    period > 0.0 && x.rem_euclid(period) < bar
}

fn crosses(x: f64, size: f64, period: f64, bar: f64) -> bool {
    if period <= 0.0 {
        return false;
    }
    let a = (x - size / 2.0).rem_euclid(period);
    let b = a + size;
    size >= period.min(bar).min(period - bar) || (a < bar && b > bar) || b > period
}

impl Pattern {
    pub fn is_solid(&self, q: [f64; 2]) -> bool {
        match *self {
            Pattern::Solid => true,
            Pattern::Grid { period, bar, .. } => on_bar(q[0], period[0], bar[0]) || on_bar(q[1], period[1], bar[1]),
            Pattern::Opening { centre, half, round } => !in_opening(q, centre, half, round),
        }
    }

    // The distance from `q` to an opening's outline and the unit direction
    // away from the opening, for a point outside it.
    pub fn opening_edge(&self, q: [f64; 2]) -> Option<(f64, [f64; 2])> {
        let Pattern::Opening { centre, half, round } = *self else { return None };
        let offset = [q[0] - centre[0], q[1] - centre[1]];
        let length = offset[0].hypot(offset[1]);
        let away = if length > 1e-12 { [offset[0] / length, offset[1] / length] } else { [1.0, 0.0] };
        let gap = if round {
            let scaled = (offset[0] / half[0]).hypot(offset[1] / half[1]);
            ((scaled - 1.0) * half[0].min(half[1])).max(0.0)
        } else {
            (offset[0].abs() - half[0]).max(0.0).hypot((offset[1].abs() - half[1]).max(0.0))
        };
        Some((gap, away))
    }

    pub fn may_cross(&self, centre: [f64; 2], size: f64) -> bool {
        match *self {
            Pattern::Solid => false,
            Pattern::Grid { period, bar, .. } => crosses(centre[0], size, period[0], bar[0]) || crosses(centre[1], size, period[1], bar[1]),
            Pattern::Opening { centre: c, half, round } => near_opening_edge(centre, size, c, half, round),
        }
    }

    fn grid_open_fraction(&self) -> f64 {
        match *self {
            Pattern::Grid { period, bar, .. } => {
                let open = |i: usize| if period[i] > 0.0 { 1.0 - bar[i] / period[i] } else { 1.0 };
                open(0) * open(1)
            }
            _ => 0.0,
        }
    }

    pub fn solid_area(&self, polygon_area: f64) -> f64 {
        match *self {
            Pattern::Solid => polygon_area,
            Pattern::Grid { .. } => polygon_area * (1.0 - self.grid_open_fraction()),
            Pattern::Opening { half, round, .. } => {
                let hole = if round { PI * half[0] * half[1] } else { 4.0 * half[0] * half[1] };
                (polygon_area - hole).max(polygon_area * 0.01)
            }
        }
    }

    pub fn low_frequency_key(&self) -> [u64; 5] {
        match *self {
            Pattern::Grid { period, bar, thickness } => [period[0].to_bits(), period[1].to_bits(), bar[0].to_bits(), bar[1].to_bits(), thickness.to_bits()],
            _ => [0; 5],
        }
    }

    pub fn low_frequency_factor(&self, frequency_hz: f64, speed_of_sound: f64) -> f64 {
        let Pattern::Grid { period, bar, thickness } = *self else { return 1.0 };
        let open = self.grid_open_fraction();
        let solid = 1.0 - open;
        if open <= 0.0 || solid <= 0.0 {
            return 1.0;
        }
        let gap = |i: usize| if period[i] > 0.0 { period[i] - bar[i] } else { f64::INFINITY };
        let longest_period = period[0].max(period[1]);
        let hole_radius = if gap(0).is_finite() && gap(1).is_finite() { (gap(0) * gap(1) / PI).sqrt() } else { gap(0).min(gap(1)) / 2.0 };
        let mass = AIR_DENSITY * (thickness + 2.0 * END_CORRECTION_PER_RADIUS * hole_radius) / open;
        let omega_mass = 2.0 * PI * frequency_hz * mass;
        let sheet = omega_mass / (omega_mass.powi(2) + (2.0 * AIR_DENSITY * speed_of_sound).powi(2)).sqrt();
        let homogeneous = (sheet / solid).min(1.0);
        let crossover = speed_of_sound / (2.0 * longest_period);
        let blend = ((frequency_hz / crossover).log2() + 1.0).clamp(0.0, 1.0);
        homogeneous * (1.0 - blend) + blend
    }

    pub fn from_numbers(kind: f64, values: &[f64]) -> Pattern {
        match kind as i64 {
            1 => {
                let period = [values[0].max(0.0), values[1].max(0.0)];
                let bar = [values[2].clamp(0.0, period[0]), values[3].clamp(0.0, period[1])];
                Pattern::Grid { period, bar, thickness: values[4].max(0.0) }
            }
            2 => Pattern::Opening { centre: [values[0], values[1]], half: [values[2].max(1e-4), values[3].max(1e-4)], round: values[4] > 0.5 },
            _ => Pattern::Solid,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_is_solid_on_bars_and_open_between() {
        let p = Pattern::Grid { period: [0.04, 0.04], bar: [0.01, 0.01], thickness: 0.002 };
        assert!(p.is_solid([0.005, 0.02]));
        assert!(!p.is_solid([0.02, 0.02]));
        assert!((p.solid_area(1.0) - 0.4375).abs() < 1e-12);
    }

    #[test]
    fn slats_have_bars_along_one_axis() {
        let p = Pattern::Grid { period: [0.04, 0.0], bar: [0.02, 0.0], thickness: 0.01 };
        assert!(!p.is_solid([0.03, 0.0]) && p.is_solid([0.01, 0.5]));
        assert!((p.solid_area(1.0) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn opening_is_open_inside_its_outline() {
        let round = Pattern::Opening { centre: [0.5, 0.5], half: [0.1, 0.1], round: true };
        assert!(!round.is_solid([0.55, 0.55]) && round.is_solid([0.59, 0.59]));
        let square = Pattern::Opening { centre: [0.5, 0.5], half: [0.1, 0.1], round: false };
        assert!(!square.is_solid([0.59, 0.59]) && square.is_solid([0.61, 0.5]));
        assert!(square.may_cross([0.6, 0.5], 0.01) && !square.may_cross([0.5, 0.5], 0.01));
    }

    #[test]
    fn fine_grid_turns_transparent_at_low_frequency() {
        let p = Pattern::Grid { period: [0.02, 0.02], bar: [0.004, 0.004], thickness: 0.002 };
        assert!(p.low_frequency_factor(100.0, 343.0) < 0.1);
        assert!((p.low_frequency_factor(20000.0, 343.0) - 1.0).abs() < 1e-12);
    }
}
