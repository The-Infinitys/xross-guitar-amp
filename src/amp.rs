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

    pub fn initialize_truce(&mut self, sr: f64, _max_block_size: usize) {
        let sample_rate = sr as f32;
        self.gain_proc.initialize(sample_rate);
        self.eq_proc.initialize(sample_rate);
        self.cab_proc.initialize(sample_rate);
    }

    pub fn process_truce(&mut self, buffer: &mut AudioBuffer) -> ProcessStatus {
        let num_samples = buffer.num_samples();
        let input_channels = buffer.num_input_channels();
        let out_channels = buffer.num_output_channels();

        if num_samples == 0 || input_channels == 0 || out_channels == 0 {
            return ProcessStatus::Normal;
        }

        // 入力(Mono)を出力(L)にコピー
        {
            let (input, output) = buffer.io(0);
            output[..num_samples].copy_from_slice(&input[..num_samples]);
        }

        let output_l = buffer.output(0);
        self.gain_proc.process(output_l);
        self.eq_proc.process(output_l);

        // キャビネット・ステレオ展開処理
        self.cab_proc.process_truce(buffer);

        ProcessStatus::Normal
    }

    pub fn params(&self) -> Arc<XrossGuitarAmpParams> {
        self.params.clone()
    }

    pub fn ui(&self) -> Box<dyn Editor> {
        crate::editor::create_editor(self.params())
    }
}
