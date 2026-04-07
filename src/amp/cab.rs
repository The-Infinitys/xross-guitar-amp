use std::sync::Arc;

use crate::modules::filter::{Biquad, FilterType};
use crate::params::XrossGuitarAmpParams;

pub struct CabProcessor {
    pub params: Arc<XrossGuitarAmpParams>,
    // Path A
    mic_a_low_res: Biquad,
    mic_a_high_cut: Biquad,
    mic_a_mids: Biquad,
    // Path B
    mic_b_low_res: Biquad,
    mic_b_high_cut: Biquad,
    mic_b_mids: Biquad,
    // Room / Reverb
    room_delay_buffer: Vec<f32>,
    reverb_buffer: Vec<f32>, // Second buffer for diffuse reverb
    write_idx: usize,
    rev_write_idx: usize,
    sample_rate: f32,
}

impl CabProcessor {
    pub fn new(params: Arc<XrossGuitarAmpParams>) -> Self {
        Self {
            params,
            mic_a_low_res: Biquad::new(44100.0),
            mic_a_high_cut: Biquad::new(44100.0),
            mic_a_mids: Biquad::new(44100.0),
            mic_b_low_res: Biquad::new(44100.0),
            mic_b_high_cut: Biquad::new(44100.0),
            mic_b_mids: Biquad::new(44100.0),
            room_delay_buffer: vec![0.0; 44100],
            reverb_buffer: vec![0.0; 44100],
            write_idx: 0,
            rev_write_idx: 0,
            sample_rate: 44100.0,
        }
    }

    pub fn initialize(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.mic_a_low_res = Biquad::new(sample_rate);
        self.mic_a_high_cut = Biquad::new(sample_rate);
        self.mic_a_mids = Biquad::new(sample_rate);
        self.mic_b_low_res = Biquad::new(sample_rate);
        self.mic_b_high_cut = Biquad::new(sample_rate);
        self.mic_b_mids = Biquad::new(sample_rate);

        // At most 1 second of delay
        self.room_delay_buffer = vec![0.0; sample_rate as usize];
        self.reverb_buffer = vec![0.0; sample_rate as usize];
        self.write_idx = 0;
        self.rev_write_idx = 0;
    }

    pub fn reset(&mut self) {
        self.mic_a_low_res.reset();
        self.mic_a_high_cut.reset();
        self.mic_a_mids.reset();
        self.mic_b_low_res.reset();
        self.mic_b_high_cut.reset();
        self.mic_b_mids.reset();
        for x in self.room_delay_buffer.iter_mut() {
            *x = 0.0;
        }
        for x in self.reverb_buffer.iter_mut() {
            *x = 0.0;
        }
        self.write_idx = 0;
        self.rev_write_idx = 0;
    }

    pub fn process(&mut self, input: f32) -> (f32, f32) {
        let cab_params = &self.params.cab_section;
        let fx_params = &self.params.fx_section;

        // Path A (Mic A)
        let dist_a = cab_params.mic_a_distance.value();
        let axis_a = cab_params.mic_a_axis.value();

        // Path B (Mic B)
        let dist_b = cab_params.mic_b_distance.value();
        let axis_b = cab_params.mic_b_axis.value();

        // Speaker Size (8 to 15 inch) -> Resonant frequency
        let base_res_freq = 110.0 * (12.0 / cab_params.speaker_size.value());
        let count_boost = match cab_params.speaker_count.value() {
            1 => -3.0,
            2 => 0.0,
            4 => 3.0,
            _ => 6.0,
        };

        // Path A processing
        let high_cut_a = 8000.0 * (1.0 - dist_a * 0.5) * (1.0 - axis_a * 0.5);
        self.mic_a_high_cut
            .set_params(FilterType::LowPass, high_cut_a, 0.707);
        self.mic_a_low_res
            .set_params(FilterType::Peaking(count_boost + 3.0), base_res_freq, 1.0);
        self.mic_a_mids
            .set_params(FilterType::Peaking(-3.0 * axis_a), 2500.0, 0.5);

        let mut signal_a = input;
        signal_a = self.mic_a_low_res.process(signal_a);
        signal_a = self.mic_a_high_cut.process(signal_a);
        signal_a = self.mic_a_mids.process(signal_a);

        // Path B processing
        let high_cut_b = 8000.0 * (1.0 - dist_b * 0.5) * (1.0 - axis_b * 0.5);
        self.mic_b_high_cut
            .set_params(FilterType::LowPass, high_cut_b, 0.707);
        self.mic_b_low_res.set_params(
            FilterType::Peaking(count_boost + 3.0),
            base_res_freq * 1.05,
            1.0,
        );
        self.mic_b_mids
            .set_params(FilterType::Peaking(-3.0 * axis_b), 2500.0, 0.5);

        let mut signal_b = input;
        signal_b = self.mic_b_low_res.process(signal_b);
        signal_b = self.mic_b_high_cut.process(signal_b);
        signal_b = self.mic_b_mids.process(signal_b);

        // Stereo Output: panning Mic A and Mic B
        let mut out_l = signal_a * 0.8 + signal_b * 0.2;
        let mut out_r = signal_a * 0.2 + signal_b * 0.8;

        // 1. Room Effect (Short Delay / Early Reflections)
        let room_mix = cab_params.room_mix.value();
        let room_size = cab_params.room_size.value();

        if room_mix > 0.0 {
            let delay_samples = (room_size * 0.04 * self.sample_rate) as usize + 50;
            let read_idx = (self.write_idx + self.room_delay_buffer.len() - delay_samples)
                % self.room_delay_buffer.len();
            let room_signal = self.room_delay_buffer[read_idx];

            out_l = out_l * (1.0 - room_mix * 0.5) + room_signal * room_mix * 0.5;
            out_r = out_r * (1.0 - room_mix * 0.5) + room_signal * room_mix * 0.5;

            self.room_delay_buffer[self.write_idx] = (signal_a + signal_b) * 0.5;
            self.write_idx = (self.write_idx + 1) % self.room_delay_buffer.len();
        }

        // 2. Reverb Effect (Longer, diffuse tail)
        let reverb_mix = fx_params.reverb_mix.value();
        if reverb_mix > 0.0 {
            let rev_delay_samples = (0.15 * self.sample_rate) as usize; // ~150ms fixed tail
            let read_idx = (self.rev_write_idx + self.reverb_buffer.len() - rev_delay_samples)
                % self.reverb_buffer.len();
            let rev_signal = self.reverb_buffer[read_idx];

            out_l = out_l * (1.0 - reverb_mix * 0.3) + rev_signal * reverb_mix * 0.3;
            out_r = out_r * (1.0 - reverb_mix * 0.3) + rev_signal * reverb_mix * 0.3;

            // Simple feedback for tail
            self.reverb_buffer[self.rev_write_idx] = (signal_a + signal_b) * 0.5 + rev_signal * 0.6;
            self.rev_write_idx = (self.rev_write_idx + 1) % self.reverb_buffer.len();
        }

        (out_l, out_r)
    }
}
