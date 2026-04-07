use nih_plug::plugin::Plugin;
use nih_plug::prelude::*;

use crate::XrossGuitarAmp;
use crate::editor::create_editor;

impl Plugin for XrossGuitarAmp {
    const NAME: &'static str = "Xross Guitar Amp";

    const VENDOR: &'static str = "The Infinitys";

    const URL: &'static str = "https://github.com/The-Infinitys/xross-guitar-amp";

    const EMAIL: &'static str = "the.infinity.s.infinity@gmail.com";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    fn params(&self) -> std::sync::Arc<dyn nih_plug::prelude::Params> {
        self.params()
    }

    const MIDI_INPUT: MidiConfig = MidiConfig::None;

    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;

    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    const HARD_REALTIME_ONLY: bool = false;

    fn task_executor(&mut self) -> TaskExecutor<Self> {
        // In the default implementation we can simply ignore the value
        Box::new(|_| ())
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        create_editor(self.params())
    }

    fn initialize(
        &mut self,
        audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        self.initialize(audio_io_layout, buffer_config, context)
    }

    fn reset(&mut self) {}

    fn deactivate(&mut self) {}

    const AUDIO_IO_LAYOUTS: &'static [nih_plug::prelude::AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];
    type SysExMessage = ();

    type BackgroundTask = ();

    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.process(buffer, aux, context)
    }
}
