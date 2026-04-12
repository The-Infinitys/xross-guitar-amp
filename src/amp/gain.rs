use nih_plug::params::Param;

use crate::params::XrossGuitarAmpParams;
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
    os_lpf: [f32; 2], // ダウンサンプリング用
    low_resonance: f32,
    input_lpf: f32, // 入力コンディショニング用
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
        }
    }

    pub fn initialize(&mut self, _sample_rate: f32) {
        self.reset();
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

        // 0. Input Conditioning: アタック成分の保持
        self.input_lpf += 0.90 * (input - self.input_lpf);
        let conditioned = input * 0.94 + self.input_lpf * 0.06;

        // 1. Pre-Filtering (Tightening)
        let hp_freq = (0.12 + (1.0 - s_low) * 0.10 + tight_norm * 0.25) * os_inv;
        self.pre_hp += hp_freq * (conditioned - self.pre_hp);
        let mut x = conditioned - self.pre_hp;

        // 2. Pre-Resonance (Mids Character)
        let res_freq = 0.27 * os_inv;
        let res_q = 0.6 + (s_mid * 1.2); // Midに応じてレゾナンスを強化
        self.pre_res += res_freq * (x - self.pre_res);
        x = x + (x - self.pre_res) * (3.5 * res_q);
        self.pre_res *= 0.97;

        // 3. Main Distortion Stage (Hybrid & Wave-Folding)
        let drive = 4.0 + (drive_val * 45.0) * (0.4 + distortion_val * 0.6);
        x *= drive;

        // Hybrid Clipping: 高入力時にわずかに波形を折り返すエッジ感
        x = if x.abs() > 0.7 {
            let over = x.abs() - 0.7;
            (0.7 + over.atan() * (0.3 + distortion_val * 0.1)) * x.signum()
        } else {
            x
        };

        // 非対称シェイピング
        x = if x > 0.0 {
            let z = x * (2.2 + distortion_val * 2.0);
            z.atan() * (1.0 / (1.0 + (z * 0.1).powi(2)).sqrt()) * 0.8
        } else {
            (x * (1.8 + distortion_val * 1.5)).tanh() * 1.05
        };

        // 3次倍音による厚み
        x = 1.6 * x - 0.6 * x.powi(3);

        // 4. Metal Scoop Filter (Mid Param連動)
        let scoop_depth = (0.7 - s_mid).max(0.0) * 1.8;
        let scoop_filter = x.powi(3) - x * 0.4;
        x -= scoop_filter * scoop_depth;

        // 5. Dynamic Bite (エンベロープ連動スルーレート)
        let bite_base = 0.25 + (s_high * 0.60);
        let bite = bite_base * (env * 0.80 + 0.20).min(1.0);
        let diff = x - self.slew_state;
        self.slew_state += diff.clamp(-bite, bite);
        x = self.slew_state;

        // 6. Punch (低域の共振)
        let punch_amount = s_low * 1.4;
        let punch_freq = 0.09 * os_inv;
        self.low_resonance += punch_freq * (x - self.low_resonance);
        x += self.low_resonance * punch_amount;

        x.clamp(-1.0, 1.0)
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

        let target = in_signal.abs();
        let env_step = if target > self.envelope { 0.35 } else { 0.015 };
        self.envelope += env_step * (target - self.envelope);

        let dynamic_gain = 1.0 - (self.envelope * sag_val * 0.45).min(0.4);

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

        // ダウンサンプリング・フィルタ (metal.rs のヌケ重視設定を反映)
        let ds_freq = 0.55;
        self.os_lpf[0] += ds_freq * (raw_out - self.os_lpf[0]);
        self.os_lpf[1] += ds_freq * (self.os_lpf[0] - self.os_lpf[1]);
        let out = self.os_lpf[1];

        let dc_fix = out - self.dc_block;
        self.dc_block = out + 0.995 * (self.dc_block - out);

        let master_gain = 10.0f32.powf(master_gain_db / 20.0);
        dc_fix * 0.88 * master_gain // 補正係数をわずかに上げ、音圧を確保
    }
}
