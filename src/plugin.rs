use crate::amp::XrossGuitarAmp;
use truce::prelude::*;

impl PluginLogic for XrossGuitarAmp {
    fn reset(&mut self, sample_rate: f64, max_block_size: usize) {
        let params = self.params();
        params.set_sample_rate(sample_rate);
        params.snap_smoothers();

        self.initialize_truce(sample_rate, max_block_size);
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        _events: &EventList,
        _context: &mut ProcessContext,
    ) -> ProcessStatus {
        self.process_truce(buffer)
    }

    fn custom_editor(&self) -> Option<Box<dyn Editor>> {
        Some(self.ui())
    }
}
