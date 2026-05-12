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
    last_params_hash: f32,
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
            last_params_hash: -1.0,
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
        }
        for f in &mut self.body_resonators {
            f.set_sample_rate(sample_rate);
        }
        for f in &mut self.mic_a_tone {
            f.set_sample_rate(sample_rate);
        }
        for f in &mut self.mic_b_tone {
            f.set_sample_rate(sample_rate);
        }
        self.reset();
    }

    pub fn reset(&mut self) {
        self.phase_alignment_delay.fill(0.0);
        self.room_reflection.fill(0.0);
        self.speaker_compression = 0.0;
    }

    fn update_coefficients_if_needed(&mut self) {
        // 簡易的なハッシュチェックで計算負荷を軽減
        let current_hash =
            self.params.speaker_size.value() + self.params.mic_a_distance.value() * 10.0;
        if (current_hash - self.last_params_hash).abs() < 0.0001 {
            return;
        }

        let size = self.params.speaker_size.value(); // 8, 10, 12, 15 inch
        let count = self.params.speaker_count.value() as f32; // 1, 2, 4
        let res_mod = self.params.resonance.value();
        let pres_mod = self.params.presence.value();

        // 1. キャビネット共鳴の設計 (物理的サイズに基づく)
        // 底鳴り (Thump)
        self.body_resonators[0].set_params(
            FilterType::Peaking(4.0 * res_mod),
            80.0 * (12.0 / size),
            1.5,
        );
        // 内部定在波 (Boxinessの排除と微かな強調)
        self.body_resonators[1].set_params(
            FilterType::Peaking(2.0 * res_mod),
            400.0 * (12.0 / size),
            2.5,
        );
        // ウッドパネルの振動
        self.body_resonators[2].set_params(FilterType::Peaking(1.5 * res_mod), 1200.0, 1.0);

        // 2. スピーカーユニットの「コーンの硬さ」
        let unit_freq = 3000.0 + (size * 100.0);
        self.unit_character
            .set_params(FilterType::Peaking(pres_mod * 3.0), unit_freq, 0.7);

        // 3. Mic A (Dynamic: SM57 style) - 中心部のエネルギー
        let dist_a = self.params.mic_a_distance.value();
        let axis_a = self.params.mic_a_axis.value();
        let prox_a = (1.0 - dist_a).max(0.0) * 5.0; // 近接効果
        self.mic_a_tone[0].set_params(FilterType::Peaking(prox_a), 120.0, 0.7);
        self.mic_a_tone[1].set_params(FilterType::Peaking((1.0 - axis_a) * 5.0), 4500.0, 1.0); // 芯
        self.mic_a_tone[2].set_params(FilterType::LowPass, 14000.0 - (axis_a * 6000.0), 0.7);

        // 4. Mic B (Ribbon: R-121 style) - 豊かな中低域
        let dist_b = self.params.mic_b_distance.value();
        let axis_b = self.params.mic_b_axis.value();
        let prox_b = (1.0 - dist_b).max(0.0) * 8.0;
        self.mic_b_tone[0].set_params(FilterType::Peaking(prox_b), 250.0, 0.5);
        self.mic_b_tone[1].set_params(FilterType::HighShelf(-6.0 * axis_b), 3000.0, 0.7);
        self.mic_b_tone[2].set_params(FilterType::LowPass, 8000.0 - (dist_b * 3000.0), 0.9);

        // 5. マスタリング補正
        self.tight_filter
            .set_params(FilterType::HighPass, 70.0 + (count * 5.0), 0.7);
        self.thump_filter
            .set_params(FilterType::Peaking(2.0), 90.0, 1.2);
        self.air_shelf
            .set_params(FilterType::HighShelf(pres_mod * 4.0), 10000.0, 0.7);
        self.smoothing_lowpass
            .set_params(FilterType::LowPass, 12000.0, 0.7);

        self.last_params_hash = current_hash;
    }

    pub fn process_truce(&mut self, buffer: &mut AudioBuffer) {
        self.update_coefficients_if_needed();

        let num_samples = buffer.num_samples();
        let room_mix = self.params.room_mix.value();
        let room_size = self.params.room_size.value();

        // マイクロフォン間の距離による物理的な遅延 (2ms以内)
        let dist_diff =
            (self.params.mic_b_distance.value() - self.params.mic_a_distance.value()).abs();
        let delay_samples =
            (dist_diff * 0.002 * self.sample_rate).clamp(0.0, PHASE_DELAY_SIZE as f32 - 1.0);
        let delay_int = delay_samples as usize;
        let frac = delay_samples - (delay_int as f32);

        for i in 0..num_samples {
            let mut sig = buffer.output(0)[i];

            // --- 1. スピーカーの物理的飽和 (Dynamic Compression) ---
            // 大入力時に低域のアーティキュレーションが潰れる挙動を模倣
            let input_abs = sig.abs();
            let comp_target = if input_abs > 0.5 { 0.1 } else { 0.0 };
            self.speaker_compression += (comp_target - self.speaker_compression) * 0.001; // 遅いアタック

            // 非対称なサチュレーション (真空管パワーアンプとの相互作用)
            sig = if sig > 0.0 {
                (sig * 1.1).tanh()
            } else {
                (sig * 0.98).tanh() * 1.02
            };
            sig *= 1.0 - (self.speaker_compression * 0.15);

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
            // Mic A: Center (70%), Mic B: Wide (30%)
            // 完全に左右に振るのではなく、センターの芯を残しながら広げる
            let mut out_l = sig_a * 0.6 + sig_b * 0.5;
            let mut out_r = sig_a * 0.6 + sig_b * -0.2; // わずかな逆相成分で広がりを演出

            // --- 5. 最終補正 & Room ---
            out_l = self.thump_filter.process(out_l);
            out_r = self.thump_filter.process(out_r);
            out_l = self.air_shelf.process(out_l);
            out_r = self.air_shelf.process(out_r);
            out_l = self.tight_filter.process(out_l);
            out_r = self.tight_filter.process(out_r);
            out_l = self.smoothing_lowpass.process(out_l);
            out_r = self.smoothing_lowpass.process(out_r);

            // Room Reflection (アーリーリフレクションのみ)
            if room_mix > 0.0 {
                let reflect_time = 0.015 + (room_size * 0.04); // 15ms - 55ms
                let dr = (reflect_time * self.sample_rate) as usize;
                let buf_len = self.room_reflection.len();
                let idx = (self.write_idx_room + buf_len - dr) % buf_len;

                // 反射音は高域を落とし、わずかに歪ませることで「壁の質感」を出す
                let reflection = (self.room_reflection[idx] * 0.8).tanh() * 0.5;
                out_l += reflection * room_mix;
                out_r -= reflection * room_mix; // ステレオ幅の拡張

                self.room_reflection[self.write_idx_room] = (out_l + out_r) * 0.5;
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
