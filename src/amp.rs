use std::sync::Arc;
use truce::prelude::*;

pub mod cab;
pub mod eq;
pub mod gain;

pub use cab::CabProcessor;
pub use eq::EqProcessor;
pub use gain::GainProcessor;

use crate::params::XrossGuitarAmpParams;

pub struct XrossGuitarAmp {
    params: Arc<XrossGuitarAmpParams>,
    gain_proc: GainProcessor,
    eq_proc: EqProcessor,
    cab_proc: CabProcessor,
}

impl XrossGuitarAmp {
    pub fn new(params: Arc<XrossGuitarAmpParams>) -> Self {
        Self {
            gain_proc: GainProcessor::new(params.clone()),
            eq_proc: EqProcessor::new(params.clone()),
            cab_proc: CabProcessor::new(params.clone()),
            params,
        }
    }

    pub fn initialize_truce(&mut self, sr: f64, _bs: usize) {
        let sample_rate = sr as f32;
        self.gain_proc.initialize(sample_rate);
        self.eq_proc.initialize(sample_rate);
        self.cab_proc.initialize(sample_rate);
    }

    pub fn process_truce(&mut self, buffer: &mut AudioBuffer) -> ProcessStatus {
        let num_channels = buffer.num_input_channels();
        let num_samples = buffer.num_samples();

        for i in 0..num_samples {
            for channel in 0..num_channels {
                let input = {
                    let ins = buffer.input(channel);
                    ins[i]
                };

                // 1. Gain & EQ (Mono)
                let mut mono_signal = self.gain_proc.process(input);
                mono_signal = self.eq_proc.process(mono_signal);

                // 2. Cab (Mono to Stereo)
                let (left_out, right_out) = self.cab_proc.process(mono_signal);

                // 3. Write outputs
                // L channel
                let num_channels = buffer.num_output_channels();
                if num_channels >= 1 {
                    let outs = buffer.output(0);
                    outs[i] = left_out;
                }
                if num_channels >= 2 {
                    let outs = buffer.output(1);
                    outs[i] = right_out;
                }
            }
        }

        ProcessStatus::Normal
    }

    pub fn params(&self) -> Arc<XrossGuitarAmpParams> {
        self.params.clone()
    }

    pub fn ui(&self) -> Box<dyn Editor> {
        crate::editor::create_editor(self.params())
    }
}
