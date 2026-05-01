#[derive(Default, Clone)]
struct GateState {
    gate_gain: f32,
    hold_timer: i32,
    envelope: f32,
    noise_floor: f32,
    prev_envelope: f32,
    decay_velocity: f32,
    post_lpf_state: f32,

    adaptive_sensitivity: f32,
}

pub struct AutoNoiseGate {
    sample_rate: f32,
    state: GateState,
    // バッファごとの解析結果（開閉状態）を一時保存
    analysis_buffer: Vec<bool>,
}

impl AutoNoiseGate {
    pub fn new(sample_rate: f32) -> Self {
        let mut state = GateState::default();
        state.gate_gain = 1.0;
        state.noise_floor = 0.0001;
        state.adaptive_sensitivity = 5.0;

        Self {
            sample_rate,
            state,
            analysis_buffer: Vec::with_capacity(512),
        }
    }

    /// 1. Pre-Process: バッファを解析し、各サンプルのゲート開閉フラグを生成
    /// ※ 音量は一切変更しません。
    pub fn pre_process(&mut self, buffer: &[f32]) {
        // 解析結果保存用バッファのサイズ調整
        if self.analysis_buffer.len() != buffer.len() {
            self.analysis_buffer.resize(buffer.len(), false);
        }

        let state = &mut self.state;

        for (i, &sample) in buffer.iter().enumerate() {
            let abs_in = sample.abs();

            // --- A. 基本分析 ---
            // エンベロープ
            state.envelope += 0.1 * (abs_in - state.envelope);

            // 適応型ノイズフロア推定
            if abs_in < state.noise_floor {
                state.noise_floor += 0.0001 * (abs_in - state.noise_floor);
            } else {
                state.noise_floor += 0.0000001 * (abs_in - state.noise_floor);
            }

            // 減衰速度 (リリース時の自然さのため)
            let current_decay = (state.prev_envelope - state.envelope).max(0.0);
            state.decay_velocity += 0.05 * (current_decay - state.decay_velocity);
            state.prev_envelope = state.envelope;

            // --- B. 自動感度調整ロジック ---
            let snr_estimate = (state.envelope / (state.noise_floor + 1e-9)).max(1.0);
            let target_sens = if snr_estimate < 2.0 {
                12.0
            } else if snr_estimate > 10.0 {
                4.0
            } else {
                8.0
            };
            state.adaptive_sensitivity += 0.001 * (target_sens - state.adaptive_sensitivity);

            // --- C. ゲート判定 ---
            let threshold =
                (state.noise_floor * state.adaptive_sensitivity) + (state.decay_velocity * 4.0);

            let is_open = if state.gate_gain < 0.1 {
                state.envelope > threshold
            } else {
                state.envelope > threshold * 0.5 // ヒステリシス
            };

            self.analysis_buffer[i] = is_open;
        }
    }

    /// 2. Post-Process: Preの結果に基づき、バッファにゲインとフィルタを適用
    /// ※ ここで実際に音量を絞ります。
    pub fn post_process(&mut self, buffer: &mut [f32]) {
        let state = &mut self.state;
        let release_ms = 80.0; // ギターに最適な固定リリース

        let atk_coef = (-1.0 / (0.0005 * self.sample_rate)).exp();
        let rel_coef = (-1.0 / (release_ms * self.sample_rate / 1000.0)).exp();

        for (i, sample) in buffer.iter_mut().enumerate() {
            let is_open = self.analysis_buffer[i];

            // ターゲットゲインとホールド処理
            let target_gain = if is_open {
                state.hold_timer = (0.02 * self.sample_rate) as i32;
                1.0
            } else if state.hold_timer > 0 {
                state.hold_timer -= 1;
                1.0
            } else {
                0.0
            };

            // ゲインスムージング
            let coef = if target_gain > state.gate_gain {
                atk_coef
            } else {
                rel_coef
            };
            state.gate_gain = target_gain + coef * (state.gate_gain - target_gain);

            // ゲイン適用
            let input_val = *sample;
            let gated_sample = input_val * state.gate_gain;

            // ポスト LPF (閉じる際のスムージング)
            let lpf_alpha = (1.0 - state.gate_gain).powi(2) * 0.9;
            state.post_lpf_state += lpf_alpha * (gated_sample - state.post_lpf_state);

            *sample = if state.gate_gain > 0.999 {
                gated_sample
            } else {
                state.post_lpf_state
            };
        }
    }
}
