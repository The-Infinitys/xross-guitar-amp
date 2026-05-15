use std::f32::consts::PI;

#[derive(Default, Clone)]
struct GateState {
    gate_gain: f32,
    hold_timer: i32,

    // エンベロープ（帯域別）
    env_full: f32, // 全帯域の音量
    env_high: f32, // 高域（1kHz以上）の音量

    // フィルタ状態
    hp_state: f32,       // 解析用高域抽出フィルタの状態
    post_lpf_state: f32, // 閉じる際のスムージングフィルタ
}

pub struct NoiseGate {
    sample_rate: f32,
    state: GateState,
    analysis_buffer: Vec<bool>,
}

impl NoiseGate {
    pub fn new(sample_rate: f32) -> Self {
        let state = GateState {
            gate_gain: 1.0,
            ..GateState::default()
        };

        Self {
            sample_rate,
            state,
            analysis_buffer: Vec::with_capacity(512),
        }
    }

    /// バッファを解析し、各サンプルのゲート開閉フラグを生成
    pub fn pre_process(&mut self, buffer: &[f32], threshold_db: f32) {
        if self.analysis_buffer.len() != buffer.len() {
            self.analysis_buffer.resize(buffer.len(), false);
        }

        let state = &mut self.state;
        let threshold = 10.0f32.powf(threshold_db / 20.0);

        // 簡易HPF係数 (約1.5kHzで高域抽出)
        let hp_coef = -(2.0 * PI * 1500.0 / self.sample_rate).exp() + 1.0;

        for (i, &sample) in buffer.iter().enumerate() {
            let abs_in = sample.abs();

            // --- 1. 帯域別エンベロープ抽出 ---
            state.env_full += 0.1 * (abs_in - state.env_full);

            // 高域 (1次HPF経由)
            let hp_out = abs_in - state.hp_state;
            state.hp_state += hp_coef * hp_out;
            state.env_high += 0.1 * (hp_out.abs() - state.env_high);

            // --- 2. ゲート判定ロジック ---
            // 手動しきい値を使用しつつ、高域の感度も考慮
            let high_th = threshold * 0.7;

            // ヒステリシス：一度開いたら、しきい値の半分まで閉じるのを待つ
            let is_open = if state.gate_gain < 0.1 {
                state.env_full > threshold || state.env_high > high_th
            } else {
                state.env_full > threshold * 0.5 || state.env_high > high_th * 0.5
            };

            self.analysis_buffer[i] = is_open;
        }
    }

    /// Preの結果に基づき、ゲインと動的フィルタを適用
    pub fn post_process(
        &mut self,
        buffer: &mut [f32],
        attack_ms: f32,
        hold_ms: f32,
        release_ms: f32,
        range_db: f32,
    ) {
        let state = &mut self.state;

        let hold_samples = (hold_ms * self.sample_rate / 1000.0) as i32;
        let range_gain = 10.0f32.powf(range_db / 20.0);

        let atk_coef = (-1.0 / (attack_ms * self.sample_rate / 1000.0)).exp();
        let rel_coef = (-1.0 / (release_ms * self.sample_rate / 1000.0)).exp();

        for (i, sample) in buffer.iter_mut().enumerate() {
            let is_detected = self.analysis_buffer[i];

            // ターゲットゲインの決定とホールド処理
            let target_gain = if is_detected {
                state.hold_timer = hold_samples;
                1.0
            } else if state.hold_timer > 0 {
                state.hold_timer -= 1;
                1.0
            } else {
                range_gain
            };

            // ゲインスムージング
            let coef = if target_gain > state.gate_gain {
                atk_coef
            } else {
                rel_coef
            };
            state.gate_gain = target_gain + coef * (state.gate_gain - target_gain);

            let input_val = *sample;
            let mut gated_sample = input_val * state.gate_gain;

            // --- 動的ポストフィルタ (閉じる際、高域から先に削る) ---
            // range_gainが0に近いほど強くフィルターをかける
            let lpf_alpha = state.gate_gain.powi(3).clamp(0.02, 1.0);
            state.post_lpf_state += lpf_alpha * (gated_sample - state.post_lpf_state);

            if state.gate_gain < 0.999 {
                gated_sample = state.post_lpf_state;
            }

            *sample = gated_sample;
        }
    }
}
