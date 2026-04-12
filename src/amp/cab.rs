use crate::modules::filter::{Biquad, FilterType};
use crate::params::XrossGuitarAmpParams;
use std::sync::Arc;

// ビット演算用：2のべき乗 (2048)
const PHASE_DELAY_SIZE: usize = 2048;
const PHASE_DELAY_MASK: usize = PHASE_DELAY_SIZE - 1;

// 最大サンプリングレート(192kHz)想定
const MAX_BUFFER_SIZE: usize = 192000;

pub struct CabProcessor {
    pub params: Arc<XrossGuitarAmpParams>,

    // Path A/B フィルタ群（[0]:低域共振, [1]:LPF, [2]:Presence補正）
    mic_a_filters: [Biquad; 3],
    mic_b_filters: [Biquad; 3],

    // キャビネット全体の箱鳴りとタイトさの制御
    body_resonance: Biquad,
    tight_filter: Biquad,

    // 遅延バッファ
    phase_delay_buffer_b: Vec<f32>,
    room_delay_buffer: Vec<f32>,
    reverb_buffer: Vec<f32>,

    write_idx_phase: usize,
    write_idx_room: usize,
    write_idx_rev: usize,

    sample_rate: f32,

    // パラメータ変更検知
    last_speaker_size: f32,
    last_mic_params: [f32; 4],
}

impl CabProcessor {
    pub fn new(params: Arc<XrossGuitarAmpParams>) -> Self {
        let sr = 44100.0;
        Self {
            params,
            mic_a_filters: [Biquad::new(sr), Biquad::new(sr), Biquad::new(sr)],
            mic_b_filters: [Biquad::new(sr), Biquad::new(sr), Biquad::new(sr)],
            body_resonance: Biquad::new(sr),
            tight_filter: Biquad::new(sr),
            phase_delay_buffer_b: vec![0.0; PHASE_DELAY_SIZE],
            room_delay_buffer: vec![0.0; MAX_BUFFER_SIZE],
            reverb_buffer: vec![0.0; MAX_BUFFER_SIZE],
            write_idx_phase: 0,
            write_idx_room: 0,
            write_idx_rev: 0,
            sample_rate: sr,
            last_speaker_size: -1.0,
            last_mic_params: [-1.0; 4],
        }
    }

    pub fn initialize(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.room_delay_buffer.resize(sample_rate as usize, 0.0);
        self.reverb_buffer.resize(sample_rate as usize, 0.0);

        // 各フィルタの内部サンプリングレートを更新（自作Biquadに合わせる必要があれば設定）
        // ※ 提供されたBiquadにset_sample_rateがない場合は、newし直すかフィールドを直接更新してください。
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
        self.tight_filter.reset();
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

        if (s_size - self.last_speaker_size).abs() > 0.001
            || (d_a - self.last_mic_params[0]).abs() > 0.001
            || (a_a - self.last_mic_params[1]).abs() > 0.001
            || (d_b - self.last_mic_params[2]).abs() > 0.001
            || (a_b - self.last_mic_params[3]).abs() > 0.001
        {
            // --- 基礎共振設定 ---
            let base_res_freq = 110.0 * (12.0 / s_size);

            // --- Path A (明瞭度・エッジ重視) ---
            // LPFを12kHzまで引き上げ、Qを0.5にすることで高域を「生かす」
            let hc_a = 12000.0 * (1.0 - d_a * 0.35) * (1.0 - a_a * 0.25);
            self.mic_a_filters[0].set_params(FilterType::Peaking(3.0), base_res_freq, 1.2);
            self.mic_a_filters[1].set_params(FilterType::LowPass, hc_a.max(4500.0), 0.5);
            // 4.8kHz付近をブーストして「ジャリッ」とした抜けを作る
            let presence_a = 5.0 * (1.0 - a_a);
            self.mic_a_filters[2].set_params(FilterType::Peaking(presence_a), 4800.0, 0.7);

            // --- Path B (空気感・中低域重視) ---
            let hc_b = 10000.0 * (1.0 - d_b * 0.4) * (1.0 - a_b * 0.3);
            self.mic_b_filters[0].set_params(FilterType::Peaking(2.5), base_res_freq * 1.05, 1.0);
            self.mic_b_filters[1].set_params(FilterType::LowPass, hc_b.max(3500.0), 0.5);
            // 中音域のパンチ
            self.mic_b_filters[2].set_params(FilterType::Peaking(2.0), 2200.0, 0.8);

            // --- 全体補正 ---
            // 低域がボヤけないように150HzのQを高くし、85Hz以下をHPFでカット
            self.body_resonance
                .set_params(FilterType::Peaking(2.0), 150.0 * (12.0 / s_size), 2.0);
            self.tight_filter
                .set_params(FilterType::HighPass, 85.0, 0.707);

            self.last_speaker_size = s_size;
            self.last_mic_params = [d_a, a_a, d_b, a_b];
        }
    }

    pub fn process(&mut self, input: f32) -> (f32, f32) {
        self.update_coefficients();

        // 1. 全体特性：不要な超低域を除去し、箱鳴りを加える
        let cab_signal = self
            .tight_filter
            .process(self.body_resonance.process(input));

        // 2. Path A 処理
        let mut sig_a = cab_signal;
        for f in &mut self.mic_a_filters {
            sig_a = f.process(sig_a);
        }

        // 3. Path B 処理
        let mut sig_b = cab_signal;
        for f in &mut self.mic_b_filters {
            sig_b = f.process(sig_b);
        }

        // --- マイク位相差シミュレーション (線形補間ディレイ) ---
        // 最大3ms。距離を離すと適度なコンボフィルタが発生する
        let delay_float =
            (self.params.cab_section.mic_b_distance.value() * 0.003 * self.sample_rate).max(0.1);
        let delay_int = delay_float as usize;
        let frac = delay_float - (delay_int as f32);

        self.phase_delay_buffer_b[self.write_idx_phase] = sig_b;

        let r_idx1 = (self.write_idx_phase + PHASE_DELAY_SIZE - delay_int) & PHASE_DELAY_MASK;
        let r_idx2 = (r_idx1 + PHASE_DELAY_SIZE - 1) & PHASE_DELAY_MASK;

        // 滑らかな読み出し
        sig_b = self.phase_delay_buffer_b[r_idx1] * (1.0 - frac)
            + self.phase_delay_buffer_b[r_idx2] * frac;
        self.write_idx_phase = (self.write_idx_phase + 1) & PHASE_DELAY_MASK;

        // 4. 定出力パンニングを意識したミックス (Aが左寄り、Bが右寄り)
        let mut out_l = sig_a * 0.8 + sig_b * 0.2;
        let mut out_r = sig_a * 0.2 + sig_b * 0.8;

        // 5. ルーム (Early Reflection)
        let room_mix = self.params.cab_section.room_mix.value();
        if room_mix > 0.0 {
            let room_size = self.params.cab_section.room_size.value();
            let delay_samples = ((0.01 + room_size * 0.03) * self.sample_rate) as usize;
            let read_idx = (self.write_idx_room + self.room_delay_buffer.len() - delay_samples)
                % self.room_delay_buffer.len();

            let reflection = self.room_delay_buffer[read_idx];

            // こもり防止のため反射音を控えめに加算
            out_l += reflection * room_mix * 0.25;
            out_r += reflection * room_mix * 0.25;

            self.room_delay_buffer[self.write_idx_room] = (sig_a + sig_b) * 0.5;
            self.write_idx_room = (self.write_idx_room + 1) % self.room_delay_buffer.len();
        }

        // 6. リバーブ (Long Tail) - 低域の飽和を防ぐためフィードバックを0.5に
        let reverb_mix = self.params.fx_section.reverb_mix.value();
        if reverb_mix > 0.0 {
            let rev_delay = (0.07 * self.sample_rate) as usize;
            let read_idx = (self.write_idx_rev + self.reverb_buffer.len() - rev_delay)
                % self.reverb_buffer.len();
            let rev_sig = self.reverb_buffer[read_idx];

            out_l += rev_sig * reverb_mix * 0.15;
            out_r += rev_sig * reverb_mix * 0.15;

            self.reverb_buffer[self.write_idx_rev] = (sig_a + sig_b) * 0.5 + rev_sig * 0.5;
            self.write_idx_rev = (self.write_idx_rev + 1) % self.reverb_buffer.len();
        }

        (out_l, out_r)
    }
}
