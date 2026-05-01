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

/// 内部状態（モノラル専用に1つのみ保持）
pub struct MetalDistortion {
    // フィルタやエンベロープの状態
    pre_hp: f32,
    slew_state: f32,
    dc_block: f32,
    envelope: f32,
    prev_input: f32,
    low_resonance: f32,
    post_tight: f32,
    feedback_state: f32,
    os_lpf_biquad: Biquad,
    // システム定数
    sample_rate: f32,
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
        }
    }

    /// 信号処理のコアロジック
    #[inline(always)]
    fn drive_core(
        &mut self,
        input: f32,
        drive: f32,
        dist: f32,
        s_low: f32,
        s_mid: f32,
        s_high: f32,
    ) -> f32 {
        // 1. DYNAMIC PRE-HP
        let hp_freq = 0.04 + (1.0 - s_low) * 0.15 + (self.envelope * 0.2);
        self.pre_hp += hp_freq * (input - self.pre_hp);
        let mut x = input - self.pre_hp;

        // 2. GAIN STAGING
        let drive_gain = (drive * 6.5).exp() * 12.0;
        let noise_gate_scale = (self.envelope * 30.0).min(1.0).powf(1.2);
        x *= drive_gain * noise_gate_scale;

        // 3. MULTI-STAGE SATURATION
        x += self.feedback_state * (0.2 + dist * 0.15);

        let asymmetry = 0.1 * dist;
        x = if x > 0.0 {
            (x * (1.2 + dist * 0.3)).tanh()
        } else {
            (x * (1.1 + dist * 0.2)).tanh() * (0.96 - asymmetry)
        };

        // Mid Scoop
        let mid_scoop = (0.5 - s_mid).max(0.0) * 0.85;
        if mid_scoop > 0.0 {
            x -= (x - x.powi(3)) * mid_scoop;
        }

        // Hardness
        let soft_out = (x * 1.1).atan() * 0.9;
        let hard_limit = 0.85 - (dist * 0.2) - (s_high * 0.2);
        let hard_out = x.clamp(-hard_limit, hard_limit);

        let hardness_mix = (dist * 0.5 + s_high * 0.5).clamp(0.0, 1.0);
        x = (soft_out * (1.0 - hardness_mix)) + (hard_out * hardness_mix);

        self.feedback_state = x;

        // 4. POST-PROCESSING
        let lpf_cutoff = 0.2 + (s_high * 0.5) + (dist * 0.1);
        self.post_tight += lpf_cutoff * (x - self.post_tight);
        x = self.post_tight;

        let low_boost = s_low * 0.7 * (1.0 - self.envelope.min(0.7));
        self.low_resonance += 0.12 * (x - self.low_resonance);
        x += self.low_resonance * low_boost;

        // 5. SLEW RATE
        let max_step = 0.03 + (dist * 0.4) + (s_high * 0.5);
        let diff = x - self.slew_state;
        self.slew_state += diff.clamp(-max_step, max_step);

        self.slew_state
    }

    /// 1サンプル処理
    pub fn process_sample(
        &mut self,
        input: f32,
        drive: f32,
        dist: f32,
        s_low: f32,
        s_mid: f32,
        s_high: f32,
    ) -> f32 {
        let os_factor = if drive < 0.3 {
            1
        } else if drive < 0.6 {
            2
        } else {
            4
        };
        let inv_os = 1.0 / os_factor as f32;

        self.envelope += (input.abs() - self.envelope) * 0.25;

        let mut output_sum = 0.0;
        for i in 0..os_factor {
            let fraction = i as f32 * inv_os;
            let sub_sample = self.prev_input + (input - self.prev_input) * fraction;
            output_sum += self.drive_core(sub_sample, drive, dist, s_low, s_mid, s_high);
        }
        self.prev_input = input;

        let raw_out = output_sum * inv_os;

        // 2次 Butterworth LPF (約16.5kHz)
        let (a1, a2, b0, b1, b2) = Self::calculate_biquad_lpf(self.sample_rate, 16500.0);
        let filtered_out = self.os_lpf_biquad.process(raw_out, a1, a2, b0, b1, b2);

        // DC Block
        let dc_fix = filtered_out - self.dc_block;
        self.dc_block = filtered_out + 0.995 * (self.dc_block - filtered_out);

        dc_fix * 0.75
    }
    pub fn initialize(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
    }

    /// スライス処理
    pub fn process_slice(
        &mut self,
        slice: &mut [f32],
        drive: f32,
        dist: f32,
        s_low: f32,
        s_mid: f32,
        s_high: f32,
    ) {
        for sample in slice.iter_mut() {
            *sample = self.process_sample(*sample, drive, dist, s_low, s_mid, s_high);
        }
    }

    fn calculate_biquad_lpf(sample_rate: f32, cutoff: f32) -> (f32, f32, f32, f32, f32) {
        let ff = (cutoff / sample_rate).min(0.45);
        let omega = 2.0 * PI * ff;
        let sn = omega.sin();
        let cs = omega.cos();
        let alpha = sn / (2.0f32).sqrt();

        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cs / a0;
        let a2 = (1.0 - alpha) / a0;
        let b1 = (1.0 - cs) / a0;
        let b0 = b1 * 0.5;
        let b2 = b0;

        (a1, a2, b0, b1, b2)
    }
}
