use std::sync::Arc;

use crate::params::XrossGuitarAmpParams;
mod metal;
use metal::MetalDistortion;
mod noise_gate;
use noise_gate::NoiseGate;
use truce::params::FloatParamReadF32;

pub struct GainProcessor {
    pub params: Arc<XrossGuitarAmpParams>,
    metal: MetalDistortion,
    noise_gate: NoiseGate,
    sample_rate: f32,
}

impl GainProcessor {
    pub fn new(params: Arc<XrossGuitarAmpParams>) -> Self {
        Self {
            params,
            metal: MetalDistortion::new(44100.0),
            noise_gate: NoiseGate::new(44100.0),
            sample_rate: 44100.0,
        }
    }

    pub fn initialize(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.reset();
        self.metal.initialize(sample_rate);
        self.noise_gate.initialize(sample_rate);
    }
    pub fn reset(&mut self) {
        let p = self.params.clone();
        let sr = self.sample_rate;
        *self = Self::new(p);
        self.sample_rate = sr;
        self.metal.initialize(sr);
        self.noise_gate.initialize(sr);
    }

    pub fn process(&mut self, input: &mut [f32]) {
        input.iter_mut().for_each(|i| {
            if !i.is_finite() {
                *i = 0.0;
            }
        });

        // Noise Gate pre-processing with params
        let gate_th = self.params.gate_threshold.value();
        self.noise_gate.pre_process(input, gate_th);

        let input_factor = 10.0f32.powf(self.params.input_gain.value() / 20.0);
        input.iter_mut().for_each(|i| {
            *i *= input_factor;
        });

        let drive = self.params.drive.value();
        let dist = self.params.distortion.value();
        let s_low = self.params.style_low.value();
        let s_mid = self.params.style_mid.value();
        let s_high = self.params.style_high.value();
        let sag = self.params.sag.value();
        let tight = self.params.tight.value();

        let metal_params = metal::MetalParams {
            drive,
            dist,
            sag,
            tight,
            s_low,
            s_mid,
            s_high,
        };

        self.metal.process_slice(input, metal_params);

        // Noise Gate post-processing with params
        let gate_atk = self.params.gate_attack.value();
        let gate_hold = self.params.gate_hold.value();
        let gate_rel = self.params.gate_release.value();
        let gate_range = self.params.gate_range.value();
        self.noise_gate
            .post_process(input, gate_atk, gate_hold, gate_rel, gate_range);
    }
}
