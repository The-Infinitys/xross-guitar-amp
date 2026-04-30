use crate::modules::filter::{Biquad, FilterType};
use crate::params::XrossGuitarAmpParams;
use std::sync::Arc;

const PHASE_DELAY_SIZE: usize = 2048;
const PHASE_DELAY_MASK: usize = PHASE_DELAY_SIZE - 1;
const MAX_BUFFER_SIZE: usize = 192000;

pub struct CabProcessor {
    pub params: Arc<XrossGuitarAmpParams>,

    // --- フィルタ群 ---
    mic_a_filters: [Biquad; 5],
    mic_b_filters: [Biquad; 5],

    // 物理特性
    impedance_resonance: Biquad,
    presence_shelf: Biquad,
    cabinet_thump: Biquad, // 低域の押し出し
    box_resonance: Biquad, // 箱鳴り (200-400Hz付近)
    tight_filter: Biquad,
    floor_reflection: Biquad, // 床からの反射によるノッチ

    // 位相処理
    phase_smearer: [Biquad; 3], // 2枚から3枚に増やし、より拡散を複雑化

    // スピーカーの特性
    cone_character: [Biquad; 4],
    internal_standing_wave: Biquad,

    // --- バッファ群 ---
    phase_delay_buffer_b: Vec<f32>,
    room_delay_buffer: Vec<f32>,
    write_idx_phase: usize,
    write_idx_room: usize,

    sample_rate: f32,

    // --- 内部状態 (非線形処理用) ---
    prev_out: f32,

    // --- キャッシュ ---
    last_speaker_size: f32,
    last_speaker_count: i64,
    last_mic_params: [f32; 4], // [dist_a, axis_a, dist_b, axis_b]
    last_eq_extras: [f32; 2],  // [res, pres]
}

impl CabProcessor {
    pub fn new(params: Arc<XrossGuitarAmpParams>) -> Self {
        let sr = 44100.0;
        Self {
            params,
            mic_a_filters: std::array::from_fn(|_| Biquad::new(sr)),
            mic_b_filters: std::array::from_fn(|_| Biquad::new(sr)),
            impedance_resonance: Biquad::new(sr),
            presence_shelf: Biquad::new(sr),
            cabinet_thump: Biquad::new(sr),
            box_resonance: Biquad::new(sr),
            tight_filter: Biquad::new(sr),
            floor_reflection: Biquad::new(sr),
            phase_smearer: std::array::from_fn(|_| Biquad::new(sr)),
            cone_character: std::array::from_fn(|_| Biquad::new(sr)),
            internal_standing_wave: Biquad::new(sr),
            phase_delay_buffer_b: vec![0.0; PHASE_DELAY_SIZE],
            room_delay_buffer: vec![0.0; MAX_BUFFER_SIZE],
            write_idx_phase: 0,
            write_idx_room: 0,
            sample_rate: sr,
            prev_out: 0.0,
            last_speaker_size: -1.0,
            last_speaker_count: -1,
            last_mic_params: [-1.0; 4],
            last_eq_extras: [-1.0; 2],
        }
    }

    pub fn initialize(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.update_all_filter_rates(sample_rate);
        self.room_delay_buffer.resize(sample_rate as usize, 0.0);
        self.reset();
    }

    fn update_all_filter_rates(&mut self, sr: f32) {
        let filters: &mut [&mut Biquad] = &mut [
            &mut self.impedance_resonance,
            &mut self.presence_shelf,
            &mut self.cabinet_thump,
            &mut self.box_resonance,
            &mut self.tight_filter,
            &mut self.internal_standing_wave,
            &mut self.floor_reflection,
        ];
        for f in filters {
            f.set_sample_rate(sr);
        }
        for f in &mut self.mic_a_filters {
            f.set_sample_rate(sr);
        }
        for f in &mut self.mic_b_filters {
            f.set_sample_rate(sr);
        }
        for f in &mut self.cone_character {
            f.set_sample_rate(sr);
        }
        for f in &mut self.phase_smearer {
            f.set_sample_rate(sr);
        }
    }

    pub fn reset(&mut self) {
        self.phase_delay_buffer_b.fill(0.0);
        self.room_delay_buffer.fill(0.0);
        self.write_idx_phase = 0;
        self.write_idx_room = 0;
        self.prev_out = 0.0;
    }

    fn update_coefficients(&mut self) {
        let s_size = self.params.speaker_size.value();
        let s_count = self.params.speaker_count.value() as f32;
        let d_a = self.params.mic_a_distance.value();
        let a_a = self.params.mic_a_axis.value();
        let d_b = self.params.mic_b_distance.value();
        let a_b = self.params.mic_b_axis.value();
        let res_val = self.params.resonance.value();
        let pres_val = self.params.presence.value();

        let changed = (s_size - self.last_speaker_size).abs() > 0.001
            || (s_count - self.last_speaker_count as f32).abs() > 0.1
            || (d_a - self.last_mic_params[0]).abs() > 0.001
            || (a_a - self.last_mic_params[1]).abs() > 0.001
            || (d_b - self.last_mic_params[2]).abs() > 0.001
            || (a_b - self.last_mic_params[3]).abs() > 0.001
            || (res_val - self.last_eq_extras[0]).abs() > 0.001
            || (pres_val - self.last_eq_extras[1]).abs() > 0.001;

        if !changed {
            return;
        }

        let speaker_res_freq = 82.0 * (12.0 / s_size);
        let count_scale = s_count.sqrt();

        // 1. パワーアンプ・インピーダンス特性
        self.impedance_resonance.set_params(
            FilterType::Peaking(res_val * 3.0),
            speaker_res_freq,
            1.0,
        );
        self.presence_shelf
            .set_params(FilterType::HighShelf(pres_val * 2.0), 3500.0, 0.7);

        // 2. キャビネット物理特性
        let box_res_freq = 220.0 * (12.0 / s_size);
        self.box_resonance.set_params(
            FilterType::Peaking(2.5 * count_scale.min(2.0)),
            box_res_freq,
            1.8,
        );

        // 内部定在波 (サイズ依存)
        let internal_res = 1100.0 * (12.0 / s_size);
        self.internal_standing_wave
            .set_params(FilterType::Peaking(-6.0), internal_res, 4.0);

        // 床反射 (Comb Filter代わりのNotch) - 距離に応じて周波数が変化
        let floor_notch_freq = 400.0 / (d_a + 0.1);
        self.floor_reflection.set_params(
            FilterType::Notch,
            floor_notch_freq.clamp(100.0, 800.0),
            1.5,
        );

        // 3. 位相拡散
        self.phase_smearer[0].set_params(FilterType::AllPass, 1400.0, 0.4);
        self.phase_smearer[1].set_params(FilterType::AllPass, 3800.0, 0.5);
        self.phase_smearer[2].set_params(FilterType::AllPass, 8500.0, 0.3);

        // 4. コーンの分割振動
        self.cone_character[0].set_params(FilterType::Peaking(-4.0), 800.0, 1.2);
        self.cone_character[1].set_params(FilterType::Peaking(4.0), 2400.0, 1.5);
        self.cone_character[2].set_params(FilterType::Peaking(3.0), 4200.0, 2.0);
        self.cone_character[3].set_params(FilterType::Peaking(-8.0), 7000.0, 2.5);

        // 5. Mic A (SM57風: プレゼンス強調)
        let prox_a = (1.0 - d_a).powi(3) * 14.0;
        self.mic_a_filters[0].set_params(FilterType::Peaking(prox_a), 120.0, 0.7);
        let air_loss_a = 20000.0 - (d_a * 8000.0);
        self.mic_a_filters[4].set_params(FilterType::LowPass, air_loss_a.max(4000.0), 0.707);

        // 6. Mic B (121 Ribbon風: ウォーム)
        let prox_b = (1.0 - d_b).powi(2) * 20.0;
        self.mic_b_filters[0].set_params(FilterType::Peaking(prox_b), 90.0, 0.6);
        let dark_b = (1.0 - d_b) * -6.0 - (a_b * 10.0);
        self.mic_b_filters[3].set_params(FilterType::HighShelf(dark_b), 3000.0, 0.7);
        self.mic_b_filters[4].set_params(FilterType::LowPass, 12000.0 * (1.0 - a_b * 0.5), 0.707);

        // 7. 最終的なタイトさ
        self.cabinet_thump
            .set_params(FilterType::Peaking(4.0), 100.0, 2.5);
        self.tight_filter
            .set_params(FilterType::HighPass, 65.0, 0.707);

        self.last_speaker_size = s_size;
        self.last_speaker_count = s_count as i64;
        self.last_mic_params = [d_a, a_a, d_b, a_b];
        self.last_eq_extras = [res_val, pres_val];
    }

    fn apply_speaker_physics(&mut self, input: f32) -> f32 {
        // ヒステリシスを模した簡易的な非対称サチュレーション
        let drive = 1.05;
        let x = input * drive;

        // 前回の出力を用いたソフトクリップ（磁気回路の慣性）
        let target = if x > 0.0 {
            x.tanh()
        } else {
            (x * 0.98).tanh() * 1.02
        };

        // スルーレート制限的な動きを微量に混ぜて「重い」コーンの動きを出す
        let out = self.prev_out + 0.9 * (target - self.prev_out);
        self.prev_out = out;
        out
    }

    pub fn process(&mut self, input: f32) -> (f32, f32) {
        self.update_coefficients();

        // 1. スピーカーダイナミクス
        let mut sig = self.apply_speaker_physics(input);

        // 2. 共通フィルタリング
        sig = self.impedance_resonance.process(sig);
        sig = self.presence_shelf.process(sig);
        sig = self.box_resonance.process(sig);
        sig = self.cabinet_thump.process(sig);
        sig = self.internal_standing_wave.process(sig);
        sig = self.floor_reflection.process(sig);
        sig = self.tight_filter.process(sig);

        for ap in &mut self.phase_smearer {
            sig = ap.process(sig);
        }
        for f in &mut self.cone_character {
            sig = f.process(sig);
        }

        // 3. マイクパラレル
        let mut sig_a = sig;
        for f in &mut self.mic_a_filters {
            sig_a = f.process(sig_a);
        }

        let mut sig_b = sig;
        for f in &mut self.mic_b_filters {
            sig_b = f.process(sig_b);
        }

        // 4. Time Alignment (位相干渉)
        let delay_a = self.params.mic_a_distance.value() * 3.0; // 3ms max
        let delay_b = self.params.mic_b_distance.value() * 6.0; // 6ms max
        let diff_samples = (delay_b - delay_a).abs() * 0.001 * self.sample_rate;

        let delay_int = diff_samples as usize;
        let frac = diff_samples - (delay_int as f32);

        self.phase_delay_buffer_b[self.write_idx_phase] = sig_b;
        let r1 = (self.write_idx_phase + PHASE_DELAY_SIZE - delay_int) & PHASE_DELAY_MASK;
        let r2 = (r1 + PHASE_DELAY_SIZE - 1) & PHASE_DELAY_MASK;

        // 線形補間
        sig_b = self.phase_delay_buffer_b[r1] * (1.0 - frac) + self.phase_delay_buffer_b[r2] * frac;
        self.write_idx_phase = (self.write_idx_phase + 1) & PHASE_DELAY_MASK;

        // 5. Stereo Placement
        // Aを主軸、Bをテクスチャとして広げる
        let mut out_l = sig_a * 0.8 + sig_b * 0.4;
        let mut out_r = sig_a * 0.8 - sig_b * 0.2;

        // 6. Early Reflections (Room)
        let room_mix = self.params.room_mix.value();
        if room_mix > 0.0 {
            let room_size = self.params.room_size.value();
            let taps_l = [0.012, 0.025, 0.045];
            let taps_r = [0.015, 0.028, 0.052];

            let mut reflections_l = 0.0;
            let mut reflections_r = 0.0;
            let buf_len = self.room_delay_buffer.len();

            for i in 0..3 {
                let dl = ((taps_l[i] + room_size * 0.04) * self.sample_rate) as usize;
                let dr = ((taps_r[i] + room_size * 0.04) * self.sample_rate) as usize;
                reflections_l += self.room_delay_buffer
                    [(self.write_idx_room + buf_len - dl) % buf_len]
                    * (1.0 / (i + 1) as f32);
                reflections_r += self.room_delay_buffer
                    [(self.write_idx_room + buf_len - dr) % buf_len]
                    * (1.0 / (i + 1) as f32);
            }

            out_l += reflections_l * room_mix * 0.35;
            out_r += reflections_r * room_mix * 0.35;

            self.room_delay_buffer[self.write_idx_room] = (sig_a + sig_b) * 0.5;
            self.write_idx_room = (self.write_idx_room + 1) % buf_len;
        }

        // 最終ゲイン：デジタル的な「冷たさ」を避けるために微調整
        (out_l * 1.1, out_r * 1.1)
    }
}
