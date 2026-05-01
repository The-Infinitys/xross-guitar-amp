use std::sync::Arc;

use crate::params::XrossGuitarAmpParams;

pub struct GainProcessor {
    pub params: Arc<XrossGuitarAmpParams>,
    sample_rate: f32,
}

impl GainProcessor {
    pub fn new(params: Arc<XrossGuitarAmpParams>) -> Self {
        Self {
            params,
            sample_rate: 44100.0,
        }
    }

    pub fn initialize(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.reset();
    }
    pub fn reset(&mut self) {
        let p = self.params.clone();
        let sr = self.sample_rate;
        *self = Self::new(p);
        self.sample_rate = sr;
    }

    pub fn process(&mut self, input: f32) -> f32 {
        if !input.is_finite() {
            return 0.0;
        }
        input
    }
}
