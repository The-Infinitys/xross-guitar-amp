use truce::core::AudioBuffer;

use crate::modules::filter::{Biquad, FilterType};
use crate::params::XrossGuitarAmpParams;
use std::sync::Arc;

const PHASE_DELAY_SIZE: usize = 2048;
const PHASE_DELAY_MASK: usize = PHASE_DELAY_SIZE - 1;
// 最大サンプリングレート(192kHz)での約1秒分を確保
const MAX_ROOM_DELAY: usize = 192000;

pub struct CabProcessor {
    pub params: Arc<XrossGuitarAmpParams>,

    mic_a_filters: [Biquad; 5],
    mic_b_filters: [Biquad; 5],
    impedance_resonance: Biquad,
    presence_shelf: Biquad,
    cabinet_thump: Biquad,
    box_resonance: Biquad,
    tight_filter: Biquad,
    floor_reflection: Biquad,
    phase_smearer: [Biquad; 3],
    cone_character: [Biquad; 4],
    internal_standing_wave: Biquad,

    // Boxの代わりにVecを使用（ただしサイズは固定運用）
    phase_delay_buffer_b: Vec<f32>,
    room_delay_buffer: Vec<f32>,

    write_idx_phase: usize,
    write_idx_room: usize,

    sample_rate: f32,
    prev_out: f32,

    last_speaker_size: f32,
    last_speaker_count: i64,
    last_mic_params: [f32; 4],
    last_eq_extras: [f32; 2],
}

impl CabProcessor {
    pub fn new(params: Arc<XrossGuitarAmpParams>) -> Self {
        let sr = 44100.0;
        Self {
            params,
            mic_a_filters: std::array::from_fn(|_| Biquad::new(sr)),
            mic_b_filters: std::array::from_fn(|_| Biquad::new(sr)),
            impedance_resonance: Biquad::new(sr),
            presence_shelf: Biquad::new(sr),
            cabinet_thump: Biquad::new(sr),
            box_resonance: Biquad::new(sr),
            tight_filter: Biquad::new(sr),
            floor_reflection: Biquad::new(sr),
            phase_smearer: std::array::from_fn(|_| Biquad::new(sr)),
            cone_character: std::array::from_fn(|_| Biquad::new(sr)),
            internal_standing_wave: Biquad::new(sr),

            // 事前に最大容量を確保し、0で埋める
            phase_delay_buffer_b: vec![0.0; PHASE_DELAY_SIZE],
            room_delay_buffer: vec![0.0; MAX_ROOM_DELAY],

            write_idx_phase: 0,
            write_idx_room: 0,
            sample_rate: sr,
            prev_out: 0.0,
            last_speaker_size: -1.0,
            last_speaker_count: -1,
            last_mic_params: [-1.0; 4],
            last_eq_extras: [-1.0; 2],
        }
    }

    pub fn initialize(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.update_all_filter_rates(sample_rate);
        // Vecのサイズを変更せず、中身だけリセット
        self.reset();
    }

    fn update_all_filter_rates(&mut self, sr: f32) {
        let filters: &mut [&mut Biquad] = &mut [
            &mut self.impedance_resonance,
            &mut self.presence_shelf,
            &mut self.cabinet_thump,
            &mut self.box_resonance,
            &mut self.tight_filter,
            &mut self.internal_standing_wave,
            &mut self.floor_reflection,
        ];
        for f in filters {
            f.set_sample_rate(sr);
        }
        for f in &mut self.mic_a_filters {
            f.set_sample_rate(sr);
        }
        for f in &mut self.mic_b_filters {
            f.set_sample_rate(sr);
        }
        for f in &mut self.cone_character {
            f.set_sample_rate(sr);
        }
        for f in &mut self.phase_smearer {
            f.set_sample_rate(sr);
        }
    }

    pub fn reset(&mut self) {
        self.phase_delay_buffer_b.fill(0.0);
        self.room_delay_buffer.fill(0.0);
        self.write_idx_phase = 0;
        self.write_idx_room = 0;
        self.prev_out = 0.0;
    }

    fn update_coefficients_if_needed(&mut self) {
        let s_size = self.params.speaker_size.value();
        let s_count = self.params.speaker_count.value() as f32;
        let d_a = self.params.mic_a_distance.value();
        let a_a = self.params.mic_a_axis.value();
        let d_b = self.params.mic_b_distance.value();
        let a_b = self.params.mic_b_axis.value();
        let res_val = self.params.resonance.value();
        let pres_val = self.params.presence.value();

        let changed = (s_size - self.last_speaker_size).abs() > 0.001
            || (s_count - self.last_speaker_count as f32).abs() > 0.1
            || (d_a - self.last_mic_params[0]).abs() > 0.001
            || (a_a - self.last_mic_params[1]).abs() > 0.001
            || (d_b - self.last_mic_params[2]).abs() > 0.001
            || (a_b - self.last_mic_params[3]).abs() > 0.001
            || (res_val - self.last_eq_extras[0]).abs() > 0.001
            || (pres_val - self.last_eq_extras[1]).abs() > 0.001;

        if !changed {
            return;
        }

        let speaker_res_freq = 82.0 * (12.0 / s_size);
        let count_scale = s_count.sqrt();

        self.impedance_resonance.set_params(
            FilterType::Peaking(res_val * 3.0),
            speaker_res_freq,
            1.0,
        );
        self.presence_shelf
            .set_params(FilterType::HighShelf(pres_val * 2.0), 3500.0, 0.7);

        let box_res_freq = 220.0 * (12.0 / s_size);
        self.box_resonance.set_params(
            FilterType::Peaking(2.5 * count_scale.min(2.0)),
            box_res_freq,
            1.8,
        );

        let internal_res = 1100.0 * (12.0 / s_size);
        self.internal_standing_wave
            .set_params(FilterType::Peaking(-6.0), internal_res, 4.0);

        let floor_notch_freq = 400.0 / (d_a + 0.1);
        self.floor_reflection.set_params(
            FilterType::Notch,
            floor_notch_freq.clamp(100.0, 800.0),
            1.5,
        );

        self.phase_smearer[0].set_params(FilterType::AllPass, 1400.0, 0.4);
        self.phase_smearer[1].set_params(FilterType::AllPass, 3800.0, 0.5);
        self.phase_smearer[2].set_params(FilterType::AllPass, 8500.0, 0.3);

        self.cone_character[0].set_params(FilterType::Peaking(-4.0), 800.0, 1.2);
        self.cone_character[1].set_params(FilterType::Peaking(4.0), 2400.0, 1.5);
        self.cone_character[2].set_params(FilterType::Peaking(3.0), 4200.0, 2.0);
        self.cone_character[3].set_params(FilterType::Peaking(-8.0), 7000.0, 2.5);

        let prox_a = (1.0 - d_a).powi(3) * 14.0;
        self.mic_a_filters[0].set_params(FilterType::Peaking(prox_a), 120.0, 0.7);
        let air_loss_a = 20000.0 - (d_a * 8000.0);
        self.mic_a_filters[4].set_params(FilterType::LowPass, air_loss_a.max(4000.0), 0.707);

        let prox_b = (1.0 - d_b).powi(2) * 20.0;
        self.mic_b_filters[0].set_params(FilterType::Peaking(prox_b), 90.0, 0.6);
        let dark_b = (1.0 - d_b) * -6.0 - (a_b * 10.0);
        self.mic_b_filters[3].set_params(FilterType::HighShelf(dark_b), 3000.0, 0.7);
        self.mic_b_filters[4].set_params(FilterType::LowPass, 12000.0 * (1.0 - a_b * 0.5), 0.707);

        self.cabinet_thump
            .set_params(FilterType::Peaking(4.0), 100.0, 2.5);
        self.tight_filter
            .set_params(FilterType::HighPass, 65.0, 0.707);

        self.last_speaker_size = s_size;
        self.last_speaker_count = s_count as i64;
        self.last_mic_params = [d_a, a_a, d_b, a_b];
        self.last_eq_extras = [res_val, pres_val];
    }
    pub fn process_truce(&mut self, buffer: &mut AudioBuffer) {
        self.update_coefficients_if_needed();

        let num_samples = buffer.num_samples();
        let out_channels = buffer.num_output_channels();

        let room_mix = self.params.room_mix.value();
        let room_size = self.params.room_size.value();
        let mic_a_dist = self.params.mic_a_distance.value();
        let mic_b_dist = self.params.mic_b_distance.value();

        let delay_a = mic_a_dist * 3.0;
        let delay_b = mic_b_dist * 6.0;
        let diff_samples = (delay_b - delay_a).abs() * 0.001 * self.sample_rate;
        let delay_int = diff_samples as usize;
        let frac = diff_samples - (delay_int as f32);

        let buf_len = self.room_delay_buffer.len();
        let taps_l = [0.012, 0.025, 0.045];
        let taps_r = [0.015, 0.028, 0.052];

        for i in 0..num_samples {
            // output(0) には既に Gain/Eq 済みのモノラル信号が入っている
            let mono_input = buffer.output(0)[i];

            // --- 共通信号処理 (スピーカー/キャビネット特性) ---
            let x = mono_input * 1.05;
            let target = if x > 0.0 {
                x.tanh()
            } else {
                (x * 0.98).tanh() * 1.02
            };
            let mut sig = self.prev_out + 0.9 * (target - self.prev_out);
            self.prev_out = sig;

            sig = self.impedance_resonance.process(sig);
            sig = self.presence_shelf.process(sig);
            sig = self.box_resonance.process(sig);
            sig = self.cabinet_thump.process(sig);
            sig = self.internal_standing_wave.process(sig);
            sig = self.floor_reflection.process(sig);
            sig = self.tight_filter.process(sig);
            for ap in &mut self.phase_smearer {
                sig = ap.process(sig);
            }
            for f in &mut self.cone_character {
                sig = f.process(sig);
            }

            // --- マイク分岐とステレオ化 ---
            let mut sig_a = sig;
            for f in &mut self.mic_a_filters {
                sig_a = f.process(sig_a);
            }

            let mut sig_b = sig;
            for f in &mut self.mic_b_filters {
                sig_b = f.process(sig_b);
            }

            // フェイズ・アライメント (B側を遅延)
            self.phase_delay_buffer_b[self.write_idx_phase] = sig_b;
            let r1 = (self.write_idx_phase + PHASE_DELAY_SIZE - delay_int) & PHASE_DELAY_MASK;
            let r2 = (r1 + PHASE_DELAY_SIZE - 1) & PHASE_DELAY_MASK;
            sig_b =
                self.phase_delay_buffer_b[r1] * (1.0 - frac) + self.phase_delay_buffer_b[r2] * frac;
            self.write_idx_phase = (self.write_idx_phase + 1) & PHASE_DELAY_MASK;

            let mut out_l = sig_a * 0.8 + sig_b * 0.4;
            let mut out_r = sig_a * 0.8 - sig_b * 0.2;

            // ルーム・シミュレーション
            if room_mix > 0.0 {
                let mut ref_l = 0.0;
                let mut ref_r = 0.0;
                for j in 0..3 {
                    let dl = ((taps_l[j] + room_size * 0.04) * self.sample_rate) as usize;
                    let dr = ((taps_r[j] + room_size * 0.04) * self.sample_rate) as usize;
                    let idx_l = (self.write_idx_room + buf_len - dl) % buf_len;
                    let idx_r = (self.write_idx_room + buf_len - dr) % buf_len;
                    ref_l += self.room_delay_buffer[idx_l] * (1.0 / (j + 1) as f32);
                    ref_r += self.room_delay_buffer[idx_r] * (1.0 / (j + 1) as f32);
                }
                out_l += ref_l * room_mix * 0.35;
                out_r += ref_r * room_mix * 0.35;

                self.room_delay_buffer[self.write_idx_room] = (sig_a + sig_b) * 0.5;
                self.write_idx_room = (self.write_idx_room + 1) % buf_len;
            }

            // --- AudioBuffer への書き戻し ---
            if out_channels >= 2 {
                buffer.output(0)[i] = out_l * 1.1;
                buffer.output(1)[i] = out_r * 1.1;
            } else {
                // モノラル時は L/R を混ぜて出力
                buffer.output(0)[i] = (out_l + out_r) * 0.55;
            }
        }
    }
}
