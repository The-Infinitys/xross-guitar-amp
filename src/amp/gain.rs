use std::sync::Arc;

use crate::params::XrossGuitarAmpParams;

pub struct GainProcessor {
    pub params: Arc<XrossGuitarAmpParams>,

    // 内部状態 (モノラル処理用)
    pre_hp: f32,
    pre_res: f32,
    slew_state: f32,
    dc_block: f32,
    envelope: f32,
    prev_input: f32,
    os_lpf: f32,
    os_lpf_2: f32,
    low_resonance: f32,
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
            os_lpf: 0.0,
            os_lpf_2: 0.0,
            low_resonance: 0.0,
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
        self.os_lpf = 0.0;
        self.os_lpf_2 = 0.0;
        self.low_resonance = 0.0;
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
        // 1. Pre-Filtering (Tightening)
        // Combine low param with explicit Tight param
        let hp_freq = (0.10 + (1.0 - s_low) * 0.10 + tight_norm * 0.20) * os_inv;
        self.pre_hp += hp_freq * (input - self.pre_hp);
        let mut x = input - self.pre_hp;

        // 2. Pre-Resonance (Mids Character)
        let res_freq = 0.265 * os_inv;
        let res_q = 0.52 + (s_mid * 0.48);
        self.pre_res += res_freq * (x - self.pre_res);
        x = x + (x - self.pre_res) * (2.8 * res_q);
        self.pre_res *= 0.98;

        // 3. Main Distortion Stage
        let drive = 5.0 + (drive_val * 40.0) * (0.5 + distortion_val * 0.5);
        x *= drive;

        // 非対称クリッピング (Distortion value affects the blend/intensity)
        x = if x > 0.0 {
            (x * (2.5 + distortion_val * 2.0)).atan() * 0.9
        } else {
            (x * (2.0 + distortion_val * 1.5)).tanh() * 1.1
        };

        // ハード・クリッピング (Warp)
        let hard_warp = 2.0 + (drive_val * 30.0) + (distortion_val * 20.0);
        x = (x * hard_warp).clamp(-0.90, 0.90);

        // 4. Metal Scoop Filter
        let scoop_depth = (0.7 - s_mid).max(0.0) * 1.6;
        let scoop_filter = x.powi(3) - x * 0.3;
        x -= scoop_filter * scoop_depth;

        // 5. Dynamic Bite (Slew rate limit based on high-end and envelope)
        let bite_base = 0.12 + (s_high * 0.55);
        let bite = bite_base * (env * 0.85 + 0.15).min(1.0);
        let diff = x - self.slew_state;
        self.slew_state += diff.clamp(-bite, bite);
        x = self.slew_state;

        // 6. Punch (Low frequency resonance)
        let punch_amount = s_low * 1.35;
        let punch_freq = 0.08 * os_inv;
        self.low_resonance += punch_freq * (x - self.low_resonance);
        x += self.low_resonance * punch_amount;

        x.clamp(-1.0, 1.0)
    }

    pub fn process(&mut self, input: f32) -> f32 {
        // Extract needed values to avoid borrowing self.params during the loop
        let input_gain_db = self.params.gain_section.input_gain.value();
        let master_gain_db = self.params.gain_section.master_gain.value();
        let distortion_val = self.params.gain_section.distortion.value();

        let drive_val = self.params.gain_section.drive.value() / 60.0;

        let sag_val = self.params.fx_section.sag.value();
        let tight_freq_val = self.params.fx_section.tight.value();

        // 入力ゲイン適用
        let input_gain = 10.0f32.powf(input_gain_db / 20.0);
        let in_signal = input * input_gain;

        // 4x オーバーサンプリング処理
        let os_factor = 4;
        let inv_os = 1.0 / os_factor as f32;

        // エンベロープ・フォロワー (Bite & Sagの制御用)
        let target = in_signal.abs();
        let env_step = if target > self.envelope { 0.3 } else { 0.01 };
        self.envelope += env_step * (target - self.envelope);

        // Sag (Power supply sag simulation)
        let dynamic_gain = 1.0 - (self.envelope * sag_val * 0.5).min(0.5);

        let mut output_sum = 0.0;
        let current_env = self.envelope;

        // Tightening (Pre-HPF frequency integration)
        // tight_freq_val (20-500Hz) を正規化して drive_core に渡す
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

        // オーバーサンプリング後のLPF (ダウンサンプリング・フィルタ)
        let lpf_cutoff = 0.48;
        self.os_lpf += lpf_cutoff * (raw_out - self.os_lpf);
        self.os_lpf_2 += lpf_cutoff * (self.os_lpf - self.os_lpf_2);

        let out = self.os_lpf_2;

        // DCオフセット除去
        let dc_fix = out - self.dc_block;
        self.dc_block = out + 0.995 * (self.dc_block - out);

        // マスターゲインと最終補正
        let master_gain = 10.0f32.powf(master_gain_db / 20.0);
        dc_fix * 0.82 * master_gain
    }
}
