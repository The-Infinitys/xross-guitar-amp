use nih_plug::prelude::*;
use std::sync::Arc;

pub mod cab;
pub mod eq;
pub mod gain;

pub use cab::CabProcessor;
pub use eq::EqProcessor;
pub use gain::GainProcessor;

use crate::params::XrossGuitarAmpParams;
pub struct XrossGuitarAmp {
    pub params: Arc<XrossGuitarAmpParams>,
    gain_proc: GainProcessor,
    eq_proc: EqProcessor,
    cab_proc: CabProcessor,
}

impl Default for XrossGuitarAmp {
    fn default() -> Self {
        let params = Arc::new(XrossGuitarAmpParams::default());
        Self {
            gain_proc: GainProcessor::new(params.clone()),
            eq_proc: EqProcessor::new(params.clone()),
            cab_proc: CabProcessor::new(params.clone()),
            params,
        }
    }
}

impl XrossGuitarAmp {
    pub fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        _buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        true
    }

    pub fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        ProcessStatus::Normal
    }

    pub fn params(&self) -> Arc<XrossGuitarAmpParams> {
        self.params.clone()
    }
}
