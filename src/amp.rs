use std::sync::Arc;

use nih_plug::prelude::*;

use crate::params::XrossGuitarAmpParams;

#[derive(Default)]
pub struct XrossGuitarAmp {
    pub params: Arc<XrossGuitarAmpParams>,
}
impl XrossGuitarAmp {
    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        ProcessStatus::Normal
    }
}
