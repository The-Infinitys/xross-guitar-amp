use crate::params::XrossGuitarAmpParams;
use nih_plug::params::Param;
use std::f32::consts::PI;
use std::sync::Arc;

// 2次フィルタ（Biquad）の状態保持用
struct Biquad {
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn new() -> Self {
        Self { z1: 0.0, z2: 0.0 }
    }

    #[inline]
    fn process(&mut self, input: f32, a1: f32, a2: f32, b0: f32, b1: f32, b2: f32) -> f32 {
        let out = b0 * input + self.z1;
        self.z1 = b1 * input - a1 * out + self.z2;
        self.z2 = b2 * input - a2 * out;
        out
    }
}

pub struct GainProcessor {
    pub params: Arc<XrossGuitarAmpParams>,

    // 内部状態
    pre_hp: f32,
    pre_res: f32,
    slew_state: f32,
    dc_block: f32,
    envelope: f32,
    prev_input: f32,
    low_resonance: f32,
    input_lpf: f32,
    feedback_state: f32,
    post_tight: f32,

    // 2次LPF用の状態
    os_lpf_biquad: Biquad,
    sample_rate: f32,
}

impl GainProcessor {
    pub fn new(params: Arc<XrossGuitarAmpParams>) -> Self {
        Self {
            params,
            pre_hp: 0.0,
            pre_res: 0.0,
            slew_state: 0.0,
            dc_block: 0.0,
            envelope: 0.0,
            prev_input: 0.0,
            low_resonance: 0.0,
            input_lpf: 0.0,
            feedback_state: 0.0,
            post_tight: 0.0,
            os_lpf_biquad: Biquad::new(),
            sample_rate: 44100.0, // 初期値
        }
    }
    pub fn initialize(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.reset();
    }
    pub fn reset(&mut self) {
        self.pre_hp = 0.0;
        self.pre_res = 0.0;
        self.slew_state = 0.0;
        self.dc_block = 0.0;
        self.envelope = 0.0;
        self.prev_input = 0.0;
        self.low_resonance = 0.0;
        self.input_lpf = 0.0;
        self.feedback_state = 0.0;
        self.post_tight = 0.0;
        self.os_lpf_biquad = Biquad::new();
    }

    // Butterworth 2次低域通過フィルタの係数計算
    fn calculate_biquad_lpf(&self, cutoff: f32) -> (f32, f32, f32, f32, f32) {
        let ff = (cutoff / self.sample_rate).min(0.45);
        let omega = 2.0 * PI * ff;
        let sn = omega.sin();
        let cs = omega.cos();
        let alpha = sn / (2.0f32).sqrt(); // Q = 0.707

        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cs / a0;
        let a2 = (1.0 - alpha) / a0;
        let b1 = (1.0 - cs) / a0;
        let b0 = b1 * 0.5;
        let b2 = b0;

        (a1, a2, b0, b1, b2)
    }

    #[inline]
    fn drive_core(
        &mut self,
        input: f32,
        env: f32,
        os_inv: f32,
        drive_val: f32,
        tight_norm: f32,
        distortion_val: f32,
    ) -> f32 {
        let s_low = (self.params.eq_section.low.value() + 12.0) / 24.0;
        let s_mid = (self.params.eq_section.mid.value() + 12.0) / 24.0;
        let s_high = (self.params.eq_section.high.value() + 12.0) / 24.0;

        // 0. Input Conditioning
        self.input_lpf += 0.85 * (input - self.input_lpf);
        let conditioned = input * 0.92 + self.input_lpf * 0.08;

        // 1. Dynamic Pre-Filtering (ピッキングへの食いつき)
        let hp_freq = (0.10 + (1.0 - s_low) * 0.15 + tight_norm * 0.35 + (env * 0.15)) * os_inv;
        self.pre_hp += hp_freq * (conditioned - self.pre_hp);
        let mut x = conditioned - self.pre_hp;

        // 2. Pre-Resonance (Mids Character)
        let res_freq = 0.25 * os_inv;
        let res_q = 0.5 + (s_mid * 1.5);
        self.pre_res += res_freq * (x - self.pre_res);
        x = x + (x - self.pre_res) * (2.8 * res_q);

        // 3. Distortion Stage
        let noise_gate_scale = (env * 15.0).min(1.0).powf(1.5);
        let drive = (4.0 + (drive_val * 60.0) * (0.2 + distortion_val * 0.8)) * noise_gate_scale;
        x *= drive;

        // 粘りのフィードバック (メタル譲りの高揚感)
        x += self.feedback_state * 0.22;

        // 非対称サチュレーション
        let soft_out = if x > 0.0 {
            (x * 1.1).tanh()
        } else {
            (x * 1.05).tanh() * 0.97
        };

        // 矩形波エッジのブレンド
        let hard_limit = 0.85 - (distortion_val * 0.20);
        let hard_out = x.clamp(-hard_limit, hard_limit);

        let square_mix = distortion_val * 0.45;
        x = (soft_out * (1.0 - square_mix)) + (hard_out * square_mix);
        self.feedback_state = x;

        // 4. Metal Scoop
        let scoop_depth = (0.6 - s_mid).max(0.0) * 1.6;
        x -= (x - x.powi(3)) * scoop_depth;

        // 5. Post Filtering (ゴミ掃除)
        let post_cutoff = (0.30 + (s_high * 0.50)) * os_inv;
        self.post_tight += post_cutoff * (x - self.post_tight);
        x = self.post_tight;

        // 6. Slew Rate (Bite感)
        let bite_base = 0.03 + (s_high * 0.90);
        let bite = bite_base * (env * 0.7 + 0.3).min(1.0);
        let diff = x - self.slew_state;
        self.slew_state += diff.clamp(-bite, bite);
        x = self.slew_state;

        // 7. Punch (低域共振)
        let punch_amount = s_low * 1.3 * (1.0 - env.min(0.75));
        let punch_freq = 0.10 * os_inv;
        self.low_resonance += punch_freq * (x - self.low_resonance);
        x += self.low_resonance * punch_amount;

        x.clamp(-1.2, 1.2)
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let input_gain_db = self.params.gain_section.input_gain.value();
        let master_gain_db = self.params.gain_section.master_gain.value();
        let distortion_val = self.params.gain_section.distortion.value();
        let drive_val = self.params.gain_section.drive.modulated_normalized_value();
        let sag_val = self.params.fx_section.sag.value();
        let tight_freq_val = self.params.fx_section.tight.value();

        let input_gain = 10.0f32.powf(input_gain_db / 20.0);
        let in_signal = input * input_gain;

        // 固定4倍オーバーサンプリング
        let os_factor = 4;
        let inv_os = 1.0 / os_factor as f32;

        let target = in_signal.abs();
        let env_step = if target > self.envelope { 0.25 } else { 0.02 };
        self.envelope += env_step * (target - self.envelope);

        let dynamic_gain = 1.0 - (self.envelope * sag_val * 0.45).min(0.5);

        let mut output_sum = 0.0;
        let current_env = self.envelope;
        let tight_norm = (tight_freq_val - 20.0) / 480.0;

        for i in 0..os_factor {
            let fraction = i as f32 * inv_os;
            let sub_sample =
                (self.prev_input + (in_signal - self.prev_input) * fraction) * dynamic_gain;
            output_sum += self.drive_core(
                sub_sample,
                current_env,
                inv_os,
                drive_val,
                tight_norm,
                distortion_val,
            );
        }
        self.prev_input = in_signal;

        let raw_out = output_sum * inv_os;

        // 2次Biquadフィルタによるダウンサンプリング（エイリアシング除去）
        let (a1, a2, b0, b1, b2) = self.calculate_biquad_lpf(18000.0);
        let filtered_out = self.os_lpf_biquad.process(raw_out, a1, a2, b0, b1, b2);

        // DC Block
        let out = filtered_out;
        let dc_fix = out - self.dc_block;
        self.dc_block = out + 0.996 * (self.dc_block - out);

        let master_gain = 10.0f32.powf(master_gain_db / 20.0);
        dc_fix * 0.82 * master_gain
    }
}
