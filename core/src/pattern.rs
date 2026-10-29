#[derive(Clone, Copy, Debug)]
pub enum Pattern {
    Solid,
}

impl Pattern {
    pub fn is_solid(&self, _q: [f64; 2]) -> bool {
        match self {
            Pattern::Solid => true,
        }
    }

    pub fn may_cross(&self, _centre: [f64; 2], _size: f64) -> bool {
        match self {
            Pattern::Solid => false,
        }
    }

    pub fn solid_fraction(&self) -> f64 {
        match self {
            Pattern::Solid => 1.0,
        }
    }

    pub fn from_numbers(kind: f64, _values: &[f64]) -> Pattern {
        match kind as i64 {
            _ => Pattern::Solid,
        }
    }
}
