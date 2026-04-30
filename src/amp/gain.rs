use crate::params::XrossGuitarAmpParams;
use std::f32::consts::PI;
use std::sync::Arc;

/// 2次フィルタ（Biquad）の状態保持用
#[derive(Default, Clone)]
struct Biquad {
    z1: f32,
    z2: f32,
}

impl Biquad {
    /// ダイレクトフォームII転置形式
    #[inline(always)]
    fn process(&mut self, input: f32, a1: f32, a2: f32, b0: f32, b1: f32, b2: f32) -> f32 {
        let out = b0 * input + self.z1;
        self.z1 = b1 * input - a1 * out + self.z2;
        self.z2 = b2 * input - a2 * out;
        // 非正規化数対策
        if out.abs() < 1e-15 { 0.0 } else { out }
    }
}

pub struct GainProcessor {
    pub params: Arc<XrossGuitarAmpParams>,
    // 信号処理状態
    pre_hp: f32,
    slew_state: f32,
    dc_block: f32,
    input_dc_block: f32,
    envelope: f32,
    gate_visual: f32,
    low_resonance: f32,
    post_tight: f32,
    feedback_state: f32,
    sag_voltage: f32,
    os_lpf_biquad: Biquad,
    prev_input: f32,
    sample_rate: f32,
}

impl GainProcessor {
    pub fn new(params: Arc<XrossGuitarAmpParams>) -> Self {
        Self {
            params,
            pre_hp: 0.0,
            slew_state: 0.0,
            dc_block: 0.0,
            input_dc_block: 0.0,
            envelope: 0.0,
            gate_visual: 1.0,
            low_resonance: 0.0,
            post_tight: 0.0,
            feedback_state: 0.0,
            sag_voltage: 1.0,
            os_lpf_biquad: Biquad::default(),
            prev_input: 0.0,
            sample_rate: 44100.0,
        }
    }

    pub fn initialize(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.reset();
    }

    pub fn reset(&mut self) {
        let p = self.params.clone();
        let sr = self.sample_rate;
        // 状態を初期化しつつ、パラメータとサンプリングレートを維持
        *self = Self::new(p);
        self.sample_rate = sr;
    }

    pub fn process(&mut self, input: f32) -> f32 {
        if !input.is_finite() {
            return 0.0;
        }

        let p = &self.params;
        let g = p.drive.value();
        let dist = p.distortion.value();
        let sag_knob = p.sag.value();
        let sl = (p.low.value() + 18.0) / 36.0;
        let sm = (p.mid.value() + 18.0) / 36.0;
        let sh = (p.high.value() + 18.0) / 36.0;
        let master = 10.0f32.powf(p.master_gain.value() / 20.0);

        // --- 1. Envelope & Gate Analysis ---
        // DCオフセットを除去した成分でエンベロープを取る
        let in_dc = input - self.input_dc_block;
        self.input_dc_block = input + 0.999 * (self.input_dc_block - input);

        let abs_in = in_dc.abs();
        let env_speed = if abs_in > self.envelope { 0.5 } else { 0.01 };
        self.envelope += (abs_in - self.envelope) * env_speed;

        // ゲート：音が小さくなると gate_visual が 0 に向かい、フェードアウトさせる
        let gate_target = if self.envelope > 0.003 { 1.0 } else { 0.0 };
        self.gate_visual += (gate_target - self.gate_visual) * 0.05;

        // --- 2. Dynamic Sag ---
        let sag_target = 1.0 - (self.envelope * sag_knob * 0.4).min(0.5);
        self.sag_voltage += (sag_target - self.sag_voltage) * 0.005;

        // --- 3. Variable Oversampling ---
        let os_factor = if g < 0.3 {
            1
        } else if g < 0.6 {
            2
        } else {
            4
        };
        let inv_os = 1.0 / os_factor as f32;

        let mut output_sum = 0.0;
        for i in 0..os_factor {
            // 線形補間
            let fraction = (i as f32 + 1.0) * inv_os;
            let sub_sample = self.prev_input + (in_dc - self.prev_input) * fraction;
            output_sum += self.drive_core(sub_sample, g, dist, sl, sm, sh);
        }
        self.prev_input = in_dc;

        let raw_out = output_sum * inv_os;

        // --- 4. Dynamic Post Filtering ---
        // 終わり際の矩形波的なザラつきを消すため、音が小さい時はLPFを閉じる
        let noise_filter_freq = 18000.0 * (0.1 + self.gate_visual * 0.9);
        let (a1, a2, b0, b1, b2) =
            self.calculate_biquad_lpf(noise_filter_freq.clamp(500.0, 18000.0));
        let filtered_out = self.os_lpf_biquad.process(raw_out, a1, a2, b0, b1, b2);

        // 出力段の DC Block
        let out_final = filtered_out - self.dc_block;
        self.dc_block = filtered_out + 0.998 * (self.dc_block - filtered_out);

        // マスターゲインとゲート（フェーダー）の適用
        (out_final * 0.8 * master * self.gate_visual).clamp(-1.0, 1.0)
    }

    #[inline(always)]
    fn drive_core(&mut self, input: f32, g: f32, dist: f32, sl: f32, sm: f32, sh: f32) -> f32 {
        // A. Pre-Processing: Dynamic Tightness
        let dynamic_tight = (self.envelope * 0.5).min(0.4);
        let hp_freq = 0.05 + (1.0 - sl) * 0.12 + dynamic_tight;
        self.pre_hp += hp_freq * (input - self.pre_hp);
        let mut x = input - self.pre_hp;

        // B. Adaptive Gain Staging
        // 終わり際のブチブチを防ぐため、極小入力時はゲインを抑制
        let input_sensitivity = (self.envelope * 120.0).clamp(0.1, 1.0);
        let drive_amt =
            (g * 5.8).exp() * 18.0 * input_sensitivity * (0.75 + self.sag_voltage * 0.25);
        x *= drive_amt;

        // C. Multi-Stage Saturation
        x += self.feedback_state * 0.2;

        // Stage 1: Asymmetric Tube Saturation (倍音密度の強化)
        let bias = 0.25 * (1.0 - dist);
        let x_s1 = if x > 0.0 {
            (x + bias).tanh() - bias.tanh()
        } else {
            ((x + bias) * 1.2).tanh() - bias.tanh()
        };

        // Stage 2: Hard Clipping Blend (Style Highに連動するエッジ)
        let h_limit = 0.72 - (sh * 0.1);
        let x_s2 = x_s1.clamp(-h_limit, h_limit);

        // ブレンド比率の調整
        let distorted = x_s1 * (1.0 - dist * 0.7) + x_s2 * (dist * 0.7);

        // 減衰時の自然な消え際（Clean Blend Logic）
        let clean_mix = (1.0 - (self.envelope * 60.0)).clamp(0.0, 1.0);
        x = (distorted * (1.0 - clean_mix)) + (x.tanh() * clean_mix);

        self.feedback_state = x;
        x *= self.sag_voltage;

        // D. Tone Shaping
        // Mid Scoop
        let scoop = (0.55 - sm).max(0.0) * 0.8;
        x -= (x * x.abs() * x) * scoop;

        // Post LPF (Drive量に連動してエッジを残す)
        let lpf = 0.28 + (sh * 0.45) + (g * 0.15);
        self.post_tight += lpf * (x - self.post_tight);
        x = self.post_tight;

        // Low Resonance
        let res = sl * 0.7 * (1.0 - self.envelope.min(0.8));
        self.low_resonance += 0.15 * (x - self.low_resonance);
        x += self.low_resonance * res;

        // E. Slew Rate (アタックのエッジ強調)
        let max_step = 0.09 + (sh * 0.95);
        let diff = x - self.slew_state;
        self.slew_state += diff.clamp(-max_step, max_step);

        self.slew_state
    }

    fn calculate_biquad_lpf(&self, cutoff: f32) -> (f32, f32, f32, f32, f32) {
        let f = (cutoff / self.sample_rate).clamp(0.001, 0.49);
        let w0 = 2.0 * PI * f;
        let cos_w0 = w0.cos();
        let alpha = w0.sin() / (2.0f32).sqrt(); // Q = 0.707
        let a0 = 1.0 + alpha;
        (
            -2.0 * cos_w0 / a0,
            (1.0 - alpha) / a0,
            ((1.0 - cos_w0) * 0.5) / a0,
            (1.0 - cos_w0) / a0,
            ((1.0 - cos_w0) * 0.5) / a0,
        )
    }
}
