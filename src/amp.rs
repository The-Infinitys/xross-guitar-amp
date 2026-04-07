use std::sync::Arc;

use nih_plug::prelude::*;

use crate::params::XrossGuitarAmpParams;

#[derive(Default)]
pub struct XrossGuitarAmp {
    pub params: Arc<XrossGuitarAmpParams>,
}
impl XrossGuitarAmp {
    pub fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        ProcessStatus::Normal
    }
    pub fn initialize(
        &mut self,
        audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        true
    }
    pub fn params(&self) -> Arc<XrossGuitarAmpParams> {
        self.params.clone()
    }
}
