use crate::params::XrossGuitarAmpParams;
use nih_plug::params::Param;
use std::sync::Arc;

pub struct GainProcessor {
    pub params: Arc<XrossGuitarAmpParams>,

    // 内部状態
    pre_hp: f32,
    pre_res: f32,
    slew_state: f32,
    dc_block: f32,
    envelope: f32,
    prev_input: f32,
    os_lpf: [f32; 2],
    low_resonance: f32,
    input_lpf: f32,
    // 追加：波形の粘りとノイズ除去用
    feedback_state: f32,
    post_tight: f32,
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
            os_lpf: [0.0; 2],
            low_resonance: 0.0,
            input_lpf: 0.0,
            feedback_state: 0.0,
            post_tight: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.pre_hp = 0.0;
        self.pre_res = 0.0;
        self.slew_state = 0.0;
        self.dc_block = 0.0;
        self.envelope = 0.0;
        self.prev_input = 0.0;
        self.os_lpf = [0.0; 2];
        self.low_resonance = 0.0;
        self.input_lpf = 0.0;
        self.feedback_state = 0.0;
        self.post_tight = 0.0;
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

        // 1. Dynamic Pre-Filtering
        let hp_freq = (0.10 + (1.0 - s_low) * 0.12 + tight_norm * 0.30 + (env * 0.1)) * os_inv;
        self.pre_hp += hp_freq * (conditioned - self.pre_hp);
        let mut x = conditioned - self.pre_hp;

        // 2. Pre-Resonance (Mids Character)
        let res_freq = 0.25 * os_inv;
        let res_q = 0.5 + (s_mid * 1.5);
        self.pre_res += res_freq * (x - self.pre_res);
        x = x + (x - self.pre_res) * (2.8 * res_q);

        // 3. Main Distortion Stage (Multi-stage & Noise Controlled)
        // 入力が小さい時のゲインを絞り、「サー」音を抑制
        let noise_gate_scale = (env * 12.0).min(1.0).powf(1.8);
        let drive = (4.0 + (drive_val * 50.0) * (0.3 + distortion_val * 0.7)) * noise_gate_scale;
        x *= drive;

        // 粘りのフィードバック
        x += self.feedback_state * 0.18;

        // --- Hybrid Saturation ---
        // 1段目: 滑らかな飽和
        let soft_out = (x * 1.1).tanh();

        // 2段目: 矩形波エッジのブレンド (distortion_valが高いほど硬くなる)
        let hard_limit = 0.88 - (distortion_val * 0.15);
        let hard_out = x.clamp(-hard_limit, hard_limit);

        let square_mix = distortion_val * 0.35;
        x = (soft_out * (1.0 - square_mix)) + (hard_out * square_mix);
        self.feedback_state = x;

        // 4. Metal Scoop (Mid連動)
        let scoop_depth = (0.6 - s_mid).max(0.0) * 1.5;
        x -= (x - x.powi(3)) * scoop_depth;

        // 5. Post Filtering (歪み後のゴミ掃除)
        let post_cutoff = (0.35 + (s_high * 0.45)) * os_inv;
        self.post_tight += post_cutoff * (x - self.post_tight);
        x = self.post_tight;

        // 6. Dynamic Bite (Slew Rate)
        let bite_base = 0.04 + (s_high * 0.85);
        let bite = bite_base * (env * 0.7 + 0.3).min(1.0);
        let diff = x - self.slew_state;
        self.slew_state += diff.clamp(-bite, bite);
        x = self.slew_state;

        // 7. Punch (低域の共振)
        let punch_amount = s_low * 1.2 * (1.0 - env.min(0.7));
        let punch_freq = 0.08 * os_inv;
        self.low_resonance += punch_freq * (x - self.low_resonance);
        x += self.low_resonance * punch_amount;

        x.clamp(-1.1, 1.1)
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

        let os_factor = 4;
        let inv_os = 1.0 / os_factor as f32;

        // エンベロープ追従
        let target = in_signal.abs();
        let env_step = if target > self.envelope { 0.25 } else { 0.02 };
        self.envelope += env_step * (target - self.envelope);

        // Sag (コンプレッション感)
        let dynamic_gain = 1.0 - (self.envelope * sag_val * 0.40).min(0.5);

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

        // ダウンサンプリング・フィルタ (位相のズレを抑えつつノイズ除去)
        let ds_freq = 0.48;
        self.os_lpf[0] += ds_freq * (raw_out - self.os_lpf[0]);
        self.os_lpf[1] += ds_freq * (self.os_lpf[0] - self.os_lpf[1]);
        let out = self.os_lpf[1];

        let dc_fix = out - self.dc_block;
        self.dc_block = out + 0.995 * (self.dc_block - out);

        let master_gain = 10.0f32.powf(master_gain_db / 20.0);
        dc_fix * 0.85 * master_gain
    }
}
