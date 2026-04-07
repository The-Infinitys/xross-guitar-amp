use std::sync::Arc;

use crate::modules::filter::{Biquad, FilterType};
use crate::params::XrossGuitarAmpParams;

pub struct EqProcessor {
    pub params: Arc<XrossGuitarAmpParams>,
    low_filter: Biquad,
    mid_filter: Biquad,
    high_filter: Biquad,
    presence_filter: Biquad,
    resonance_filter: Biquad,
}

impl EqProcessor {
    pub fn new(params: Arc<XrossGuitarAmpParams>) -> Self {
        Self {
            params,
            low_filter: Biquad::new(44100.0),
            mid_filter: Biquad::new(44100.0),
            high_filter: Biquad::new(44100.0),
            presence_filter: Biquad::new(44100.0),
            resonance_filter: Biquad::new(44100.0),
        }
    }

    pub fn initialize(&mut self, sample_rate: f32) {
        self.low_filter = Biquad::new(sample_rate);
        self.mid_filter = Biquad::new(sample_rate);
        self.high_filter = Biquad::new(sample_rate);
        self.presence_filter = Biquad::new(sample_rate);
        self.resonance_filter = Biquad::new(sample_rate);
    }

    pub fn reset(&mut self) {
        self.low_filter.reset();
        self.mid_filter.reset();
        self.high_filter.reset();
        self.presence_filter.reset();
        self.resonance_filter.reset();
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let eq_params = &self.params.eq_section;

        // Frequencies typical for guitar amps
        let low_freq = 100.0;
        let mid_freq = 750.0;
        let high_freq = 3000.0;
        let presence_freq = 6000.0;
        let resonance_freq = 80.0;

        // Set filter parameters
        self.low_filter
            .set_params(FilterType::LowShelf(eq_params.low.value()), low_freq, 0.707);
        self.mid_filter
            .set_params(FilterType::Peaking(eq_params.mid.value()), mid_freq, 0.5);
        self.high_filter.set_params(
            FilterType::HighShelf(eq_params.high.value()),
            high_freq,
            0.707,
        );
        self.presence_filter.set_params(
            FilterType::HighShelf(eq_params.presence.value()),
            presence_freq,
            0.707,
        );
        self.resonance_filter.set_params(
            FilterType::Peaking(eq_params.resonance.value()),
            resonance_freq,
            1.0,
        );

        // Process sequentially
        let mut signal = input;
        signal = self.low_filter.process(signal);
        signal = self.mid_filter.process(signal);
        signal = self.high_filter.process(signal);
        signal = self.presence_filter.process(signal);
        signal = self.resonance_filter.process(signal);

        signal
    }
}
