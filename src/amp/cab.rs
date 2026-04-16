use crate::modules::filter::{Biquad, FilterType};
use crate::params::XrossGuitarAmpParams;
use std::sync::Arc;

const PHASE_DELAY_SIZE: usize = 2048;
const PHASE_DELAY_MASK: usize = PHASE_DELAY_SIZE - 1;
const MAX_BUFFER_SIZE: usize = 192000;

pub struct CabProcessor {
    pub params: Arc<XrossGuitarAmpParams>,

    // マイク特性フィルタ
    mic_a_filters: [Biquad; 5],
    mic_b_filters: [Biquad; 5],

    // キャビネット物理特性
    impedance_resonance: Biquad,
    presence_shelf: Biquad,
    cabinet_thump: Biquad,
    tight_filter: Biquad,

    // スピーカーコーンの分割振動・共鳴
    cone_character: [Biquad; 4],

    // キャビネット内部の定在波 (Standing Waves) 抑制/強調
    internal_standing_wave: Biquad,

    // バッファ群
    phase_delay_buffer_b: Vec<f32>,
    room_delay_buffer: Vec<f32>,
    reverb_buffer: Vec<f32>,

    write_idx_phase: usize,
    write_idx_room: usize,
    write_idx_rev: usize,

    sample_rate: f32,

    // キャッシュ
    last_speaker_size: f32,
    last_speaker_count: i32,
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
            tight_filter: Biquad::new(sr),
            cone_character: std::array::from_fn(|_| Biquad::new(sr)),
            internal_standing_wave: Biquad::new(sr),
            phase_delay_buffer_b: vec![0.0; PHASE_DELAY_SIZE],
            room_delay_buffer: vec![0.0; MAX_BUFFER_SIZE],
            reverb_buffer: vec![0.0; MAX_BUFFER_SIZE],
            write_idx_phase: 0,
            write_idx_room: 0,
            write_idx_rev: 0,
            sample_rate: sr,
            last_speaker_size: -1.0,
            last_speaker_count: -1,
            last_mic_params: [-1.0; 4],
            last_eq_extras: [-1.0; 2],
        }
    }

    pub fn initialize(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.get_all_filters_mut()
            .iter_mut()
            .for_each(|f| f.set_sample_rate(sample_rate));

        self.room_delay_buffer.resize(sample_rate as usize, 0.0);
        self.reverb_buffer.resize(sample_rate as usize, 0.0);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.get_all_filters_mut()
            .iter_mut()
            .for_each(|f| f.reset());
        self.phase_delay_buffer_b.fill(0.0);
        self.room_delay_buffer.fill(0.0);
        self.reverb_buffer.fill(0.0);
        self.write_idx_phase = 0;
        self.write_idx_room = 0;
        self.write_idx_rev = 0;
        self.last_speaker_size = -1.0;
    }

    fn get_all_filters_mut(&mut self) -> Vec<&mut Biquad> {
        let mut filters = Vec::new();
        filters.extend(self.mic_a_filters.iter_mut());
        filters.extend(self.mic_b_filters.iter_mut());
        filters.extend(self.cone_character.iter_mut());
        filters.push(&mut self.impedance_resonance);
        filters.push(&mut self.presence_shelf);
        filters.push(&mut self.cabinet_thump);
        filters.push(&mut self.tight_filter);
        filters.push(&mut self.internal_standing_wave);
        filters
    }

    fn update_coefficients(&mut self) {
        let cab = &self.params.cab_section;
        let eq = &self.params.eq_section;

        let s_size = cab.speaker_size.value();
        let s_count = cab.speaker_count.value();
        let d_a = cab.mic_a_distance.value();
        let a_a = cab.mic_a_axis.value();
        let d_b = cab.mic_b_distance.value();
        let a_b = cab.mic_b_axis.value();
        let res_val = eq.resonance.value();
        let pres_val = eq.presence.value();

        if (s_size - self.last_speaker_size).abs() > 0.001
            || s_count != self.last_speaker_count
            || (d_a - self.last_mic_params[0]).abs() > 0.001
            || (a_a - self.last_mic_params[1]).abs() > 0.001
            || (res_val - self.last_eq_extras[0]).abs() > 0.001
        {
            let speaker_res_freq = 82.0 * (12.0 / s_size);
            let count_scale = (s_count as f32).sqrt();

            // 1. パワーアンプ相互作用
            self.impedance_resonance.set_params(
                FilterType::Peaking(res_val * 2.2),
                speaker_res_freq,
                1.2,
            );
            self.presence_shelf
                .set_params(FilterType::HighShelf(pres_val * 1.5), 4200.0, 0.7);

            // 2. キャビネット内部の定在波 (サイズ依存)
            // キャビ内の奥行きによる干渉をシミュレート
            let internal_res = 1100.0 * (12.0 / s_size);
            self.internal_standing_wave
                .set_params(FilterType::Peaking(-4.0), internal_res, 2.0);

            // 3. スピーカー個体差 (Cone Breakup)
            self.cone_character[0].set_params(FilterType::Peaking(-4.5), 800.0, 1.5);
            self.cone_character[1].set_params(FilterType::Peaking(3.5), 2400.0, 1.0);
            self.cone_character[2].set_params(FilterType::Peaking(4.0), 3600.0, 2.5);
            self.cone_character[3].set_params(FilterType::Peaking(-5.0), 6000.0, 3.0);

            // 4. Mic A (Dynamic: SM57-like)
            let prox_a = (1.0 - d_a).powi(3) * 15.0;
            self.mic_a_filters[0].set_params(
                FilterType::Peaking(prox_a),
                speaker_res_freq * 1.1,
                1.0,
            );
            self.mic_a_filters[1].set_params(FilterType::Peaking(2.5), 500.0, 0.8);
            let edge_a = (1.0 - a_a) * 7.0;
            self.mic_a_filters[2].set_params(FilterType::Peaking(edge_a), 3500.0, 1.2);
            self.mic_a_filters[3].set_params(FilterType::Peaking(edge_a * 0.5), 5000.0, 2.0);
            let hc_a = 14000.0 * (1.0 - a_a * 0.6) * (1.0 - d_a * 0.2);
            self.mic_a_filters[4].set_params(FilterType::LowPass, hc_a.max(2500.0), 0.707);

            // 5. Mic B (Ribbon: R121-like)
            let prox_b = (1.0 - d_b).powi(2) * 20.0;
            self.mic_b_filters[0].set_params(
                FilterType::Peaking(prox_b),
                speaker_res_freq * 0.9,
                0.7,
            );
            self.mic_b_filters[1].set_params(FilterType::Peaking(5.0), 300.0, 0.5);
            self.mic_b_filters[2].set_params(FilterType::Peaking(2.0), 2000.0, 0.8);
            self.mic_b_filters[3].set_params(FilterType::Peaking(-4.0), 4500.0, 1.5);
            let hc_b = 10000.0 * (1.0 - a_b * 0.75) * (1.0 - d_b * 0.4);
            self.mic_b_filters[4].set_params(FilterType::LowPass, hc_b.max(1800.0), 0.707);

            // 6. Overall Cabinet Thump
            self.cabinet_thump.set_params(
                FilterType::Peaking(4.0 * count_scale),
                130.0 * (12.0 / s_size),
                2.5,
            );
            self.tight_filter
                .set_params(FilterType::HighPass, 60.0, 0.6);

            self.last_speaker_size = s_size;
            self.last_speaker_count = s_count;
            self.last_mic_params = [d_a, a_a, d_b, a_b];
            self.last_eq_extras = [res_val, pres_val];
        }
    }

    /// スピーカーの物理的な非線形挙動
    fn apply_physical_compression(&self, input: f32) -> f32 {
        // 非対称なサチュレーション（スピーカーの押し出しと引き戻しの物理的な差）
        if input > 0.0 {
            (input * 1.1).tanh()
        } else {
            (input * 1.05).tanh() * 0.98
        }
    }

    pub fn process(&mut self, input: f32) -> (f32, f32) {
        self.update_coefficients();

        // --- 1. スピーカーの物理的伝達関数 ---
        let mut sig = self.apply_physical_compression(input);

        sig = self.impedance_resonance.process(sig);
        sig = self.presence_shelf.process(sig);
        sig = self.cabinet_thump.process(sig);
        sig = self.internal_standing_wave.process(sig);
        sig = self.tight_filter.process(sig);

        for f in &mut self.cone_character {
            sig = f.process(sig);
        }

        // --- 2. Parallel Mic Processing ---
        let mut sig_a = sig;
        for f in &mut self.mic_a_filters {
            sig_a = f.process(sig_a);
        }

        let mut sig_b = sig;
        for f in &mut self.mic_b_filters {
            sig_b = f.process(sig_b);
        }

        // --- 3. マイキングの位相干渉 (Time Alignment) ---
        let delay_a = self.params.cab_section.mic_a_distance.value() * 3.0; // ms
        let delay_b = self.params.cab_section.mic_b_distance.value() * 6.0; // ms

        let diff_samples = (delay_b - delay_a).abs() * 0.001 * self.sample_rate;
        let delay_int = diff_samples as usize;
        let frac = diff_samples - (delay_int as f32);

        self.phase_delay_buffer_b[self.write_idx_phase] = sig_b;
        let r1 = (self.write_idx_phase + PHASE_DELAY_SIZE - delay_int) & PHASE_DELAY_MASK;
        let r2 = (r1 + PHASE_DELAY_SIZE - 1) & PHASE_DELAY_MASK;

        sig_b = self.phase_delay_buffer_b[r1] * (1.0 - frac) + self.phase_delay_buffer_b[r2] * frac;
        self.write_idx_phase = (self.write_idx_phase + 1) & PHASE_DELAY_MASK;

        // --- 4. Stereo Mixing ---
        let mut out_l = sig_a * 0.8 + sig_b * 0.4;
        let mut out_r = sig_a * 0.4 + sig_b * 0.8;

        // --- 5. Room (初期反射のマルチタップ) ---
        let room_mix = self.params.cab_section.room_mix.value();
        if room_mix > 0.0 {
            let room_size = self.params.cab_section.room_size.value();
            let taps = [0.015, 0.028, 0.042, 0.060]; // 4反射
            let mut reflections = 0.0;

            for &t in &taps {
                let d_samples = ((t + room_size * 0.05) * self.sample_rate) as usize;
                let r_idx = (self.write_idx_room + self.room_delay_buffer.len() - d_samples)
                    % self.room_delay_buffer.len();
                reflections += self.room_delay_buffer[r_idx] * 0.25;
            }

            out_l += reflections * room_mix;
            out_r += reflections * room_mix * 0.8; // 微かに定位をずらす

            self.room_delay_buffer[self.write_idx_room] = (sig_a + sig_b) * 0.5;
            self.write_idx_room = (self.write_idx_room + 1) % self.room_delay_buffer.len();
        }

        // --- 6. Reverb (フィードバック・コムフィルタ・ネットワークの簡易版) ---
        let reverb_mix = self.params.fx_section.reverb_mix.value();
        if reverb_mix > 0.0 {
            let rev_time = 0.085 * self.sample_rate; // 固定の残響密度
            let r_idx = (self.write_idx_rev + self.reverb_buffer.len() - rev_time as usize)
                % self.reverb_buffer.len();
            let rev_sig = self.reverb_buffer[r_idx];

            out_l += rev_sig * reverb_mix * 0.4;
            out_r += rev_sig * reverb_mix * 0.4;

            // フィードバックループにローパスをかけ、高域を減衰させる（空気吸収の再現）
            let feedback = (sig_a + sig_b) * 0.5 + rev_sig * 0.65;
            self.reverb_buffer[self.write_idx_rev] = feedback;
            self.write_idx_rev = (self.write_idx_rev + 1) % self.reverb_buffer.len();
        }

        (out_l * 1.3, out_r * 1.3)
    }
}
