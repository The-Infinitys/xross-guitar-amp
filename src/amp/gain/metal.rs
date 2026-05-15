use std::f32::consts::PI;

/// 2次フィルタ（Biquad）の状態保持用
#[derive(Default, Clone, Copy)]
pub struct Biquad {
    z1: f32,
    z2: f32,
}

impl Biquad {
    #[inline(always)]
    fn process(&mut self, input: f32, a1: f32, a2: f32, b0: f32, b1: f32, b2: f32) -> f32 {
        let out = b0 * input + self.z1;
        self.z1 = b1 * input - a1 * out + self.z2;
        self.z2 = b2 * input - a2 * out;
        out
    }
}

pub struct MetalDistortion {
    // 内部フィルタ・状態変数
    pre_hp: f32,
    slew_state: f32,
    dc_block: f32,
    envelope: f32,
    prev_input: f32,
    low_resonance: f32,
    post_tight: f32,     // 歪み後の動的な高域引き締め
    feedback_state: f32, // サチュレーションのフィードバック
    os_lpf_biquad: Biquad,
    sample_rate: f32,

    // キャッシュ
    last_s_high: f32,
    cached_coeffs: (f32, f32, f32, f32, f32),
}

#[derive(Clone, Copy)]
pub struct MetalParams {
    pub drive: f32,
    pub dist: f32,
    pub sag: f32,
    pub tight: f32,
    pub s_low: f32,
    pub s_mid: f32,
    pub s_high: f32,
}

impl MetalDistortion {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            pre_hp: 0.0,
            slew_state: 0.0,
            dc_block: 0.0,
            envelope: 0.0,
            prev_input: 0.0,
            low_resonance: 0.0,
            post_tight: 0.0,
            feedback_state: 0.0,
            os_lpf_biquad: Biquad::default(),
            sample_rate,
            last_s_high: -1.0,
            cached_coeffs: (0.0, 0.0, 1.0, 0.0, 0.0),
        }
    }

    pub fn initialize(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.last_s_high = -1.0;
    }

    /// 信号処理のコアロジック
    #[inline(always)]
    fn drive_core(&mut self, input: f32, p: MetalParams) -> f32 {
        let drive = p.drive;
        let dist = p.dist;
        let sag = p.sag;
        let tight = p.tight;
        let s_low = p.s_low;
        let s_mid = p.s_mid;
        let s_high = p.s_high;

        if drive <= 0.0 {
            return input;
        }

        // 1. PRE-FILTERING (Input Tightness)
        let tight_norm = (tight * 2.0 * PI / self.sample_rate).clamp(0.001, 0.5);
        self.pre_hp += tight_norm * (input - self.pre_hp);
        let mut x = input - self.pre_hp;

        // 2. GAIN STAGING & SAG
        let sag_val = 1.0 - (self.envelope * sag * 0.6);
        let drive_gain = if drive < 0.15 {
            drive * 6.0
        } else {
            1.0 + (drive - 0.15).powf(1.6) * 85.0
        };
        x *= drive_gain * sag_val;

        // 3. MULTI-STAGE SATURATION WITH FEEDBACK & TENACITY
        let fb_amount = 0.15 + dist * 0.3;
        let mut sig = x + (self.feedback_state * fb_amount);

        let asymmetry = 0.08 * dist + (sag * 0.15);
        let drive_factor = 1.3 + dist * 2.5;

        let soft_clip = |v: f32, g: f32| {
            let vg = v * g;
            if vg.abs() < 1.0 {
                vg - (vg.powi(3) / 3.0)
            } else {
                vg.signum() * (2.0 / 3.0)
            }
        };

        if sig > 0.0 {
            sig = soft_clip(sig, drive_factor).tanh();
        } else {
            let neg_factor = drive_factor * (1.0 - asymmetry);
            sig = soft_clip(sig, neg_factor).tanh() * (1.0 - asymmetry);
        }

        self.feedback_state = sig * 0.8 + self.feedback_state * 0.1;

        // 4. EQUALIZATION & CHARACTER
        let mid_scoop = (0.5 - s_mid).max(0.0) * 0.85;
        if mid_scoop > 0.0 {
            sig -= (sig - sig.powi(3)) * mid_scoop;
        }

        let low_boost = s_low * 0.8;
        self.low_resonance += 0.2 * (sig - self.low_resonance);
        sig += self.low_resonance * low_boost;

        // 5. POST-PROCESSING (Post Tight & Slew)
        let post_cutoff = 0.1 + (s_high * 0.4) + (1.0 - drive * 0.5) * 0.3;
        self.post_tight += post_cutoff * (sig - self.post_tight);
        sig = self.post_tight;

        let max_step = 0.02 + (dist * 0.4) + (s_high * 0.5) + (1.0 - drive);
        let diff = sig - self.slew_state;
        self.slew_state += diff.clamp(-max_step, max_step);

        if drive < 0.1 {
            let mix = drive * 10.0;
            return input * (1.0 - mix) + self.slew_state * mix;
        }

        self.slew_state
    }

    pub fn process_sample(&mut self, input: f32, p: MetalParams) -> f32 {
        if p.drive <= 0.0 {
            return input;
        }

        if (p.s_high - self.last_s_high).abs() > 0.001 {
            let lpf_hz = 17500.0 - (1.0 - p.s_high) * 6000.0;
            self.cached_coeffs = Self::calculate_biquad_lpf(self.sample_rate, lpf_hz);
            self.last_s_high = p.s_high;
        }

        let os_factor = if p.drive < 0.2 {
            1
        } else if p.drive < 0.5 {
            2
        } else {
            4
        };
        let inv_os = 1.0 / os_factor as f32;

        let env_target = input.abs();
        let env_rate = if env_target > self.envelope {
            0.1
        } else {
            0.005
        };
        self.envelope += (env_target - self.envelope) * env_rate;

        let mut output_sum = 0.0;
        for i in 0..os_factor {
            let fraction = i as f32 * inv_os;
            let sub_sample = self.prev_input + (input - self.prev_input) * fraction;
            output_sum += self.drive_core(sub_sample, p);
        }
        self.prev_input = input;

        let raw_out = output_sum * inv_os;

        let (a1, a2, b0, b1, b2) = self.cached_coeffs;
        let filtered_out = self.os_lpf_biquad.process(raw_out, a1, a2, b0, b1, b2);

        let dc_fix = filtered_out - self.dc_block;
        self.dc_block = filtered_out + 0.995 * (self.dc_block - filtered_out);
        dc_fix * 0.75
    }

    pub fn process_slice(&mut self, slice: &mut [f32], p: MetalParams) {
        for sample in slice.iter_mut() {
            *sample = self.process_sample(*sample, p);
        }
    }

    fn calculate_biquad_lpf(sample_rate: f32, cutoff: f32) -> (f32, f32, f32, f32, f32) {
        let ff = (cutoff / sample_rate).min(0.45);
        let omega = 2.0 * PI * ff;
        let sn = omega.sin();
        let cs = omega.cos();
        let alpha = sn / (2.0f32).sqrt(); // Q = 0.707 (Butterworth)

        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cs / a0;
        let a2 = (1.0 - alpha) / a0;
        let b1 = (1.0 - cs) / a0;
        let b0 = b1 * 0.5;
        let b2 = b0;

        (a1, a2, b0, b1, b2)
    }
}
