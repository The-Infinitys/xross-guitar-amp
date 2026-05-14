use crate::modules::filter::{Biquad, FilterType};
use crate::params::XrossGuitarAmpParams;
use std::sync::Arc;
use truce::core::AudioBuffer;

const PHASE_DELAY_SIZE: usize = 2048;
const PHASE_DELAY_MASK: usize = PHASE_DELAY_SIZE - 1;
const MAX_ROOM_DELAY: usize = 192000;

pub struct CabProcessor {
    pub params: Arc<XrossGuitarAmpParams>,

    // --- 物理モデリング・フィルター ---
    // キャビネットの筐体共鳴 (低域の重み、中域の箱鳴り、高域の反射)
    body_resonators: [Biquad; 3],
    // スピーカーユニットの固有特性
    unit_character: Biquad,

    // --- マイクロフォン・セクション ---
    // Mic A: 57スタイルのダイナミックマイク (芯とアタック)
    mic_a_tone: [Biquad; 3],
    // Mic B: 121スタイルのリボンマイク (暖かさと奥行き)
    mic_b_tone: [Biquad; 3],

    // --- 最終補正 (Studio Mastering Logic) ---
    thump_filter: Biquad,      // 100Hz以下の物理的な「押し」
    air_shelf: Biquad,         // 10kHz以上の空気感
    tight_filter: Biquad,      // 濁り（Mud）を取るためのハイパス
    smoothing_lowpass: Biquad, // 耳障りな高域(Fizz)を抑える

    // 遅延・空間系
    phase_alignment_delay: Vec<f32>,
    room_reflection: Vec<f32>,
    write_idx_phase: usize,
    write_idx_room: usize,

    sample_rate: f32,
    speaker_compression: f32, // スピーカーの物理的な限界による圧縮

    // キャッシュ
    last_params: [f32; 8],
}

impl CabProcessor {
    pub fn new(params: Arc<XrossGuitarAmpParams>) -> Self {
        let sr = 44100.0;
        Self {
            params,
            body_resonators: std::array::from_fn(|_| Biquad::new(sr)),
            unit_character: Biquad::new(sr),
            mic_a_tone: std::array::from_fn(|_| Biquad::new(sr)),
            mic_b_tone: std::array::from_fn(|_| Biquad::new(sr)),
            thump_filter: Biquad::new(sr),
            air_shelf: Biquad::new(sr),
            tight_filter: Biquad::new(sr),
            smoothing_lowpass: Biquad::new(sr),

            phase_alignment_delay: vec![0.0; PHASE_DELAY_SIZE],
            room_reflection: vec![0.0; MAX_ROOM_DELAY],
            write_idx_phase: 0,
            write_idx_room: 0,
            sample_rate: sr,
            speaker_compression: 0.0,
            last_params: [-1.0; 8],
        }
    }

    pub fn initialize(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        let all_filters: &mut [&mut Biquad] = &mut [
            &mut self.unit_character,
            &mut self.thump_filter,
            &mut self.air_shelf,
            &mut self.tight_filter,
            &mut self.smoothing_lowpass,
        ];
        for f in all_filters {
            f.set_sample_rate(sample_rate);
            f.reset();
        }
        for f in &mut self.body_resonators {
            f.set_sample_rate(sample_rate);
            f.reset();
        }
        for f in &mut self.mic_a_tone {
            f.set_sample_rate(sample_rate);
            f.reset();
        }
        for f in &mut self.mic_b_tone {
            f.set_sample_rate(sample_rate);
            f.reset();
        }
        self.reset();
    }

    pub fn reset(&mut self) {
        self.phase_alignment_delay.fill(0.0);
        self.room_reflection.fill(0.0);
        self.speaker_compression = 0.0;
    }

    fn update_coefficients_if_needed(&mut self) {
        let size = self.params.speaker_size.value();
        let count = self.params.speaker_count.value() as f32;
        let resonance = self.params.resonance.value();
        let presence = self.params.presence.value();
        let mic_a_dist = self.params.mic_a_distance.value();
        let mic_a_axis = self.params.mic_a_axis.value();
        let mic_b_dist = self.params.mic_b_distance.value();
        let mic_b_axis = self.params.mic_b_axis.value();

        let current_params = [
            size, count, resonance, presence, mic_a_dist, mic_a_axis, mic_b_dist, mic_b_axis,
        ];

        let mut changed = false;
        for (image, &current) in current_params.iter().enumerate() {
            if (current - self.last_params[image]).abs() > 0.001 {
                changed = true;
                break;
            }
        }

        if !changed {
            return;
        }

        let res_mod = 1.0 + resonance * 0.05; // 0-18dB -> 倍率へ
        let pres_mod = 1.0 + presence * 0.05;

        // 1. キャビネット共鳴の設計 (物理的サイズに基づく)
        // 底鳴り (Thump) - サイズが大きいほど低く、共鳴が深くなる
        let thump_freq = 80.0 * (12.0 / size);
        self.body_resonators[0].set_params(
            FilterType::Peaking(6.0 * res_mod),
            thump_freq,
            1.2 + (size - 8.0) * 0.1,
        );
        // 内部定在波 (Boxiness)
        self.body_resonators[1].set_params(
            FilterType::Peaking(3.0 * res_mod),
            400.0 * (12.0 / size),
            2.0,
        );
        // ウッドパネルの振動 - 複数スピーカーだと干渉で複雑に
        self.body_resonators[2].set_params(
            FilterType::Peaking(2.0 * res_mod),
            1200.0 / (count.sqrt()),
            1.0,
        );

        // 2. スピーカーユニットの「コーンの硬さ」
        let unit_freq = 2500.0 + (size * 150.0);
        self.unit_character
            .set_params(FilterType::Peaking(pres_mod * 4.0), unit_freq, 0.6);

        // 3. Mic A (Dynamic: SM57 style)
        let dist_a = mic_a_dist;
        let axis_a = mic_a_axis;
        let prox_a = (1.0 - dist_a).powi(2) * 12.0; // 近接効果をより強調
        self.mic_a_tone[0].set_params(FilterType::Peaking(prox_a), 110.0, 0.5);
        self.mic_a_tone[1].set_params(FilterType::Peaking((1.0 - axis_a) * 6.0), 4000.0, 0.8);
        self.mic_a_tone[2].set_params(FilterType::LowPass, 15000.0 - (axis_a * 8000.0), 0.7);

        // 4. Mic B (Ribbon: R-121 style)
        let dist_b = mic_b_dist;
        let axis_b = mic_b_axis;
        let prox_b = (1.0 - dist_b).powi(2) * 15.0;
        self.mic_b_tone[0].set_params(FilterType::Peaking(prox_b), 200.0, 0.4);
        self.mic_b_tone[1].set_params(FilterType::HighShelf(-8.0 * axis_b), 2500.0, 0.7);
        self.mic_b_tone[2].set_params(FilterType::LowPass, 9000.0 - (dist_b * 4000.0), 0.8);

        // 5. マスタリング補正
        // Tight: スピーカー数が多いほど低域をタイトに
        self.tight_filter
            .set_params(FilterType::HighPass, 60.0 + (count * 8.0), 0.8);
        self.thump_filter
            .set_params(FilterType::Peaking(3.0), 90.0, 1.0);
        self.air_shelf
            .set_params(FilterType::HighShelf(pres_mod * 3.0), 11000.0, 0.7);
        self.smoothing_lowpass
            .set_params(FilterType::LowPass, 13000.0, 0.7);

        self.last_params = current_params;
    }

    pub fn process_truce(&mut self, buffer: &mut AudioBuffer) {
        self.update_coefficients_if_needed();

        let num_samples = buffer.num_samples();
        let room_mix = self.params.room_mix.value();
        let reverb_mix = self.params.reverb_mix.value();
        let room_size = self.params.room_size.value();
        let master_gain = self.params.master_gain.value();
        let master_factor = 10.0f32.powf(master_gain / 20.0);

        let mic_a_dist = self.params.mic_a_distance.value();
        let mic_b_dist = self.params.mic_b_distance.value();

        // マイクロフォン間の距離による物理的な遅延 (2ms以内)
        let dist_diff = (mic_b_dist - mic_a_dist).abs();
        let delay_samples =
            (dist_diff * 0.002 * self.sample_rate).clamp(0.0, PHASE_DELAY_SIZE as f32 - 1.0);
        let delay_int = delay_samples as usize;
        let frac = delay_samples - (delay_int as f32);

        // Room 関連の計算をループ外へ
        let total_mix = (room_mix + reverb_mix).min(1.0);
        let reflect_time = 0.015 + (room_size * 0.05);
        let dr = (reflect_time * self.sample_rate) as usize;
        let buf_len = self.room_reflection.len();
        let reverb_feedback = 0.7 + reverb_mix * 0.3;

        for i in 0..num_samples {
            let mut sig = buffer.output(0)[i];

            // --- 1. スピーカーの物理的飽和 (Dynamic Compression & Tenacity) ---
            let input_abs = sig.abs();
            // 入力が大きいほど圧縮を強め、粘り(Sustain感)を出す
            let comp_target = if input_abs > 0.3 {
                input_abs * 0.4
            } else {
                0.0
            };
            self.speaker_compression += (comp_target - self.speaker_compression) * 0.002;

            // パワーアンプのサチュレーション (粘りの核)
            // 少しだけバイアスをかけて2次倍音を増やす
            let bias = 0.02;
            sig = (sig + bias).tanh() - bias.tanh();

            // 圧縮の適用
            sig *= 1.0 - (self.speaker_compression.min(0.8) * 0.5);

            // --- 2. キャビネット筐体とスピーカーユニットの共鳴 ---
            for res in &mut self.body_resonators {
                sig = res.process(sig);
            }
            sig = self.unit_character.process(sig);

            // --- 3. マイクロフォン・パラレル・パス ---
            let mut sig_a = sig;
            for f in &mut self.mic_a_tone {
                sig_a = f.process(sig_a);
            }

            let mut sig_b = sig;
            for f in &mut self.mic_b_tone {
                sig_b = f.process(sig_b);
            }

            // Mic B に微細なディレイを適用 (位相による空間形成)
            self.phase_alignment_delay[self.write_idx_phase] = sig_b;
            let r1 = (self.write_idx_phase + PHASE_DELAY_SIZE - delay_int) & PHASE_DELAY_MASK;
            let r2 = (r1 + PHASE_DELAY_SIZE - 1) & PHASE_DELAY_MASK;
            sig_b = self.phase_alignment_delay[r1] * (1.0 - frac)
                + self.phase_alignment_delay[r2] * frac;
            self.write_idx_phase = (self.write_idx_phase + 1) & PHASE_DELAY_MASK;

            // --- 4. ミキシング & ステレオイメージング ---
            let mut out_l = sig_a * 0.7 + sig_b * 0.4;
            let mut out_r = sig_a * 0.7 + sig_b * -0.3;

            // --- 5. 最終補正 & Room ---
            out_l = self.thump_filter.process(out_l);
            out_r = self.thump_filter.process(out_r);
            out_l = self.air_shelf.process(out_l);
            out_r = self.air_shelf.process(out_r);
            out_l = self.tight_filter.process(out_l);
            out_r = self.tight_filter.process(out_r);
            out_l = self.smoothing_lowpass.process(out_l);
            out_r = self.smoothing_lowpass.process(out_r);

            // Master Gain の適用 (ここで行うことで最終的な音量を決定)
            out_l *= master_factor;
            out_r *= master_factor;

            // Room Reflection & Reverb (Tenacity & Completeness)
            if total_mix > 0.0 {
                let idx = (self.write_idx_room + buf_len - dr) % buf_len;

                let reflection = (self.room_reflection[idx] * reverb_feedback).tanh() * 0.4;

                out_l += reflection * total_mix;
                out_r -= reflection * total_mix;

                // フィードバックを戻す
                let fb_val = (out_l + out_r) * 0.5;
                // デノーマル対策
                self.room_reflection[self.write_idx_room] =
                    if fb_val.abs() < 1e-18 { 0.0 } else { fb_val };
                self.write_idx_room = (self.write_idx_room + 1) % buf_len;
            }

            // 出力
            if buffer.num_output_channels() >= 2 {
                buffer.output(0)[i] = out_l;
                buffer.output(1)[i] = out_r;
            } else {
                buffer.output(0)[i] = (out_l + out_r) * 0.5;
            }
        }
    }
}
