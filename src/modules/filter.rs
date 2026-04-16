use std::f32::consts::PI;

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum FilterType {
    LowPass,
    HighPass,
    BandPass,
    Notch,
    Peaking(f32), // gain in dB
    LowShelf(f32),
    HighShelf(f32),
}

pub struct Biquad {
    a1: f32,
    a2: f32,
    b0: f32,
    b1: f32,
    b2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
    sample_rate: f32,
}

impl Biquad {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            a1: 0.0,
            a2: 0.0,
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
            sample_rate,
        }
    }

    pub fn set_params(&mut self, filter_type: FilterType, freq: f32, q: f32) {
        let omega = 2.0 * PI * freq / self.sample_rate;
        let sin_w = omega.sin();
        let cos_w = omega.cos();
        let alpha = sin_w / (2.0 * q);

        match filter_type {
            FilterType::LowPass => {
                let a0 = 1.0 + alpha;
                self.b0 = (1.0 - cos_w) / 2.0 / a0;
                self.b1 = (1.0 - cos_w) / a0;
                self.b2 = (1.0 - cos_w) / 2.0 / a0;
                self.a1 = -2.0 * cos_w / a0;
                self.a2 = (1.0 - alpha) / a0;
            }
            FilterType::HighPass => {
                let a0 = 1.0 + alpha;
                self.b0 = (1.0 + cos_w) / 2.0 / a0;
                self.b1 = -(1.0 + cos_w) / a0;
                self.b2 = (1.0 + cos_w) / 2.0 / a0;
                self.a1 = -2.0 * cos_w / a0;
                self.a2 = (1.0 - alpha) / a0;
            }
            FilterType::Peaking(gain_db) => {
                let a = 10.0f32.powf(gain_db / 40.0);
                let a0 = 1.0 + alpha / a;
                self.b0 = (1.0 + alpha * a) / a0;
                self.b1 = -2.0 * cos_w / a0;
                self.b2 = (1.0 - alpha * a) / a0;
                self.a1 = -2.0 * cos_w / a0;
                self.a2 = (1.0 - alpha / a) / a0;
            }
            FilterType::LowShelf(gain_db) => {
                let a = 10.0f32.powf(gain_db / 40.0);
                let a0 = (a + 1.0) + (a - 1.0) * cos_w + 2.0 * a.sqrt() * alpha;
                self.b0 = a * ((a + 1.0) - (a - 1.0) * cos_w + 2.0 * a.sqrt() * alpha) / a0;
                self.b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w) / a0;
                self.b2 = a * ((a + 1.0) - (a - 1.0) * cos_w - 2.0 * a.sqrt() * alpha) / a0;
                self.a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w) / a0;
                self.a2 = ((a + 1.0) + (a - 1.0) * cos_w - 2.0 * a.sqrt() * alpha) / a0;
            }
            FilterType::HighShelf(gain_db) => {
                let a = 10.0f32.powf(gain_db / 40.0);
                let a0 = (a + 1.0) - (a - 1.0) * cos_w + 2.0 * a.sqrt() * alpha;
                self.b0 = a * ((a + 1.0) + (a - 1.0) * cos_w + 2.0 * a.sqrt() * alpha) / a0;
                self.b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w) / a0;
                self.b2 = a * ((a + 1.0) + (a - 1.0) * cos_w - 2.0 * a.sqrt() * alpha) / a0;
                self.a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w) / a0;
                self.a2 = ((a + 1.0) - (a - 1.0) * cos_w - 2.0 * a.sqrt() * alpha) / a0;
            }
            _ => {}
        }
    }

    #[inline]
    pub fn process(&mut self, input: f32) -> f32 {
        let mut output = self.b0 * input + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;

        // デノーマル（極小値）対策
        if output.abs() < 1e-18 {
            output = 0.0;
        }

        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = output;

        output
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
    }
}
