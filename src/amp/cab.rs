use crate::modules::filter::{Biquad, FilterType};
use crate::params::XrossGuitarAmpParams;
use std::sync::Arc;

// 最大サンプリングレート(192kHz)での1秒分。アロケーション防止のため固定。
const MAX_BUFFER_SIZE: usize = 192000;
// 位相干渉用の微小ディレイ（最大10msもあれば十分）
const PHASE_DELAY_SIZE: usize = 2048;

pub struct CabProcessor {
    pub params: Arc<XrossGuitarAmpParams>,

    // Path A/B フィルタ群
    mic_a_filters: [Biquad; 3],
    mic_b_filters: [Biquad; 3],

    // キャビネット全体の共鳴
    body_resonance: Biquad,
    low_shelf_beef: Biquad, // 低域の押し出し感用

    // 遅延バッファ
    phase_delay_buffer_b: Vec<f32>, // Path Bのみを遅らせて位相干渉を作る
    room_delay_buffer: Vec<f32>,
    reverb_buffer: Vec<f32>,

    write_idx_phase: usize,
    write_idx_room: usize,
    write_idx_rev: usize,

    sample_rate: f32,

    // パラメータ変更検知（CPU負荷軽減）
    last_speaker_size: f32,
    last_mic_params: [f32; 4],
}

impl CabProcessor {
    pub fn new(params: Arc<XrossGuitarAmpParams>) -> Self {
        Self {
            params,
            mic_a_filters: [
                Biquad::new(44100.0),
                Biquad::new(44100.0),
                Biquad::new(44100.0),
            ],
            mic_b_filters: [
                Biquad::new(44100.0),
                Biquad::new(44100.0),
                Biquad::new(44100.0),
            ],
            body_resonance: Biquad::new(44100.0),
            low_shelf_beef: Biquad::new(44100.0),
            phase_delay_buffer_b: vec![0.0; PHASE_DELAY_SIZE],
            room_delay_buffer: vec![0.0; MAX_BUFFER_SIZE],
            reverb_buffer: vec![0.0; MAX_BUFFER_SIZE],
            write_idx_phase: 0,
            write_idx_room: 0,
            write_idx_rev: 0,
            sample_rate: 44100.0,
            last_speaker_size: -1.0,
            last_mic_params: [-1.0; 4],
        }
    }

    pub fn initialize(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        // ベクタの長さ調整（capacityは維持されるため再アロケーションは最小限）
        self.room_delay_buffer.resize(sample_rate as usize, 0.0);
        self.reverb_buffer.resize(sample_rate as usize, 0.0);
        self.reset();
    }

    pub fn reset(&mut self) {
        for f in &mut self.mic_a_filters {
            f.reset();
        }
        for f in &mut self.mic_b_filters {
            f.reset();
        }
        self.body_resonance.reset();
        self.low_shelf_beef.reset();
        self.phase_delay_buffer_b.fill(0.0);
        self.room_delay_buffer.fill(0.0);
        self.reverb_buffer.fill(0.0);
    }

    fn update_coefficients(&mut self) {
        let cab = &self.params.cab_section;
        let s_size = cab.speaker_size.value();
        let d_a = cab.mic_a_distance.value();
        let a_a = cab.mic_a_axis.value();
        let d_b = cab.mic_b_distance.value();
        let a_b = cab.mic_b_axis.value();

        // 変更がある場合のみ重い計算を実行
        if (s_size - self.last_speaker_size).abs() > 0.0001
            || (d_a - self.last_mic_params[0]).abs() > 0.0001
            || (a_a - self.last_mic_params[1]).abs() > 0.0001
            || (d_b - self.last_mic_params[2]).abs() > 0.0001
            || (a_b - self.last_mic_params[3]).abs() > 0.0001
        {
            let base_res_freq = 110.0 * (12.0 / s_size);

            // Path A フィルタ設定
            let hc_a = 8500.0 * (1.0 - d_a * 0.4) * (1.0 - a_a * 0.3);
            self.mic_a_filters[0].set_params(FilterType::Peaking(4.0), base_res_freq, 1.2);
            self.mic_a_filters[1].set_params(FilterType::LowPass, hc_a, 0.707);
            self.mic_a_filters[2].set_params(FilterType::Peaking(-5.0 * a_a), 3000.0, 0.5);

            // Path B フィルタ設定
            let hc_b = 8500.0 * (1.0 - d_b * 0.4) * (1.0 - a_b * 0.3);
            self.mic_b_filters[0].set_params(FilterType::Peaking(4.0), base_res_freq * 1.02, 1.2);
            self.mic_b_filters[1].set_params(FilterType::LowPass, hc_b, 0.707);
            self.mic_b_filters[2].set_params(FilterType::Peaking(-5.0 * a_b), 3000.0, 0.5);

            // 共通：箱鳴りシミュレーション
            // 150Hz付近のレゾナンスと、400Hz付近のキャビネット内のこもり
            self.body_resonance
                .set_params(FilterType::Peaking(3.0), 150.0 * (12.0 / s_size), 0.4);
            self.low_shelf_beef
                .set_params(FilterType::LowShelf(2.0), 100.0, 0.707);

            self.last_speaker_size = s_size;
            self.last_mic_params = [d_a, a_a, d_b, a_b];
        }
    }

    pub fn process(&mut self, input: f32) -> (f32, f32) {
        self.update_coefficients();

        // 1. キャビネット全体の基礎特性を適用
        let mut cab_signal = self.body_resonance.process(input);
        cab_signal = self.low_shelf_beef.process(cab_signal);

        // 2. Path A 処理
        let mut sig_a = cab_signal;
        for f in &mut self.mic_a_filters {
            sig_a = f.process(sig_a);
        }

        // 3. Path B 処理 + 位相オフセット (Comb Filter)
        // マイクBの距離(0.0-1.0)を最大5msの遅延に変換（物理的なマイク位置のズレを再現）
        let phase_delay_samples =
            (self.params.cab_section.mic_b_distance.value() * 0.005 * self.sample_rate) as usize;
        let read_idx_phase =
            (self.write_idx_phase + PHASE_DELAY_SIZE - phase_delay_samples) % PHASE_DELAY_SIZE;

        let mut sig_b = cab_signal;
        for f in &mut self.mic_b_filters {
            sig_b = f.process(sig_b);
        }

        // Path B をバッファに書き込み、遅延したものを読み出す
        self.phase_delay_buffer_b[self.write_idx_phase] = sig_b;
        sig_b = self.phase_delay_buffer_b[read_idx_phase];
        self.write_idx_phase = (self.write_idx_phase + 1) % PHASE_DELAY_SIZE;

        // 4. ステレオミックス（定出力パンニングを想定）
        let mut out_l = sig_a * 0.9 + sig_b * 0.1;
        let mut out_r = sig_a * 0.1 + sig_b * 0.9;

        // 5. ルームシミュレーション (Early Reflections)
        let room_mix = self.params.cab_section.room_mix.value();
        if room_mix > 0.0 {
            let room_size = self.params.cab_section.room_size.value();
            // 反射音の遅延 (20ms ~ 50ms)
            let delay_samples = ((0.02 + room_size * 0.03) * self.sample_rate) as usize;
            let read_idx = (self.write_idx_room + self.room_delay_buffer.len() - delay_samples)
                % self.room_delay_buffer.len();

            let reflection = self.room_delay_buffer[read_idx];

            // 反射音に少しハイカットを適用（簡易的）
            out_l = out_l * (1.0 - room_mix * 0.5) + reflection * room_mix * 0.5;
            out_r = out_r * (1.0 - room_mix * 0.5) + reflection * room_mix * 0.5;

            self.room_delay_buffer[self.write_idx_room] = (sig_a + sig_b) * 0.5;
            self.write_idx_room = (self.write_idx_room + 1) % self.room_delay_buffer.len();
        }

        // 6. リバーブ (Long Tail)
        let reverb_mix = self.params.fx_section.reverb_mix.value();
        if reverb_mix > 0.0 {
            let rev_delay = (0.12 * self.sample_rate) as usize;
            let read_idx = (self.write_idx_rev + self.reverb_buffer.len() - rev_delay)
                % self.reverb_buffer.len();
            let rev_sig = self.reverb_buffer[read_idx];

            out_l += rev_sig * reverb_mix * 0.2;
            out_r += rev_sig * reverb_mix * 0.2;

            // フィードバックと減衰
            self.reverb_buffer[self.write_idx_rev] = (sig_a + sig_b) * 0.5 + rev_sig * 0.7;
            self.write_idx_rev = (self.write_idx_rev + 1) % self.reverb_buffer.len();
        }

        (out_l, out_r)
    }
}
