use crate::modules::filter::{Biquad, FilterType};
use crate::params::XrossGuitarAmpParams;
use crate::utils::ParamChangeDetector;
use std::sync::Arc;
use truce::core::AudioBuffer;
use truce::params::FloatParamReadF32;

const PHASE_DELAY_SIZE: usize = 2048;
const PHASE_DELAY_MASK: usize = PHASE_DELAY_SIZE - 1;
const MAX_ROOM_DELAY: usize = 192000;

/// 精密なキャビネット物理モデリング・プロセッサ
pub struct CabProcessor {
    pub params: Arc<XrossGuitarAmpParams>,

    // --- 筐体物理モデリング ---
    // 筐体の低域共鳴、箱鳴り、内部反射を多段階でシミュレート
    body_resonators: [Biquad; 4],
    // スピーカーコーンの硬さと分割振動 (Breakup) のエミュレーション
    cone_character: [Biquad; 2],

    // --- マイク・モデリング ---
    mic_a_tone: [Biquad; 3],
    mic_b_tone: [Biquad; 3],

    // --- 動的補正 (Dynamic Response) ---
    // 入力信号のエンベロープに基づき、物理的な「しなり」を再現
    thump_dynamic: Biquad,
    air_exciter: Biquad,
    tight_shaper: Biquad,
    fizzy_cut: Biquad,

    // 遅延・空間
    phase_delay_line: Vec<f32>,
    room_buffer: Vec<f32>,
    write_idx_phase: usize,
    write_idx_room: usize,

    // 動的パラメータの状態
    envelope: f32,
    sample_rate: f32,
    detector: ParamChangeDetector<12>,
}

impl CabProcessor {
    pub fn new(params: Arc<XrossGuitarAmpParams>) -> Self {
        let sr = 44100.0;
        Self {
            params,
            body_resonators: std::array::from_fn(|_| Biquad::new(sr)),
            cone_character: std::array::from_fn(|_| Biquad::new(sr)),
            mic_a_tone: std::array::from_fn(|_| Biquad::new(sr)),
            mic_b_tone: std::array::from_fn(|_| Biquad::new(sr)),
            thump_dynamic: Biquad::new(sr),
            air_exciter: Biquad::new(sr),
            tight_shaper: Biquad::new(sr),
            fizzy_cut: Biquad::new(sr),
            phase_delay_line: vec![0.0; PHASE_DELAY_SIZE],
            room_buffer: vec![0.0; MAX_ROOM_DELAY],
            write_idx_phase: 0,
            write_idx_room: 0,
            envelope: 0.0,
            sample_rate: sr,
            detector: ParamChangeDetector::new(0.0001),
        }
    }

    pub fn initialize(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        let filters: &mut [&mut Biquad] = &mut [
            &mut self.thump_dynamic,
            &mut self.air_exciter,
            &mut self.tight_shaper,
            &mut self.fizzy_cut,
        ];
        for f in filters {
            f.set_sample_rate(sample_rate);
            f.reset();
        }
        for f in &mut self.body_resonators {
            f.set_sample_rate(sample_rate);
            f.reset();
        }
        for f in &mut self.cone_character {
            f.set_sample_rate(sample_rate);
            f.reset();
        }
        for f in &mut self.mic_a_tone {
            f.set_sample_rate(sample_rate);
            f.reset();
        }
        for f in &mut self.mic_b_tone {
            f.set_sample_rate(sample_rate);
            f.reset();
        }
        self.reset();
    }

    pub fn reset(&mut self) {
        self.phase_delay_line.fill(0.0);
        self.room_buffer.fill(0.0);
        self.envelope = 0.0;
        self.detector = ParamChangeDetector::new(0.0001);
    }

    fn update_coefficients_if_needed(&mut self) {
        let p = &self.params;
        let values = [
            p.speaker_size.value(),
            p.speaker_count.value_f32(),
            p.resonance.value(),
            p.presence.value(),
            p.mic_a_distance.value(),
            p.mic_a_axis.value(),
            p.mic_b_distance.value(),
            p.mic_b_axis.value(),
            p.cab_open_back.value(),
            p.speaker_thump.value(),
            p.speaker_sparkle.value(),
            p.master_gain.value(),
        ];

        if !self.detector.is_changed(values) {
            return;
        }

        let [
            size,
            count,
            res,
            pres,
            m_a_d,
            m_a_x,
            m_b_d,
            m_b_x,
            open,
            thump,
            sparkle,
            _,
        ] = values;
        let res_scale = 1.0 + res * 0.1;

        // 1. 筐体の多段共鳴 (物理的体積に基づく)
        let base_f = 12.0 / size;
        // 基本振動 (110Hz付近)
        self.body_resonators[0].set_params(
            FilterType::Peaking((6.0 + thump * 4.0) * (1.0 - open * 0.6)),
            95.0 * base_f,
            1.5,
        );
        // キャビネット内部の定在波 (450Hz付近)
        self.body_resonators[1].set_params(
            FilterType::Peaking(3.0 * res_scale),
            440.0 * base_f,
            3.0,
        );
        // パネル共振 (中高域の箱鳴り)
        self.body_resonators[2].set_params(FilterType::Peaking(2.0), 1100.0 / count.sqrt(), 1.0);
        // 位相干渉によるディップのシミュレート
        self.body_resonators[3].set_params(
            FilterType::Notch,
            3200.0 * (1.0 + (1.0 - m_a_x) * 0.2),
            2.0,
        );

        // 2. コーン・レスポンス (Cone Breakup)
        // 2k-5kHzのピークが「ギターアンプらしさ」を作る
        let cone_freq = 2800.0 + (sparkle * 1200.0);
        self.cone_character[0].set_params(FilterType::Peaking(4.0 * res_scale), cone_freq, 0.7);
        self.cone_character[1].set_params(FilterType::HighShelf(pres * 3.0), 5000.0, 0.7);

        // 3. Mic A (Dynamic / SM57) - 芯のある中域
        self.mic_a_tone[0].set_params(FilterType::Peaking((1.0 - m_a_d) * 10.0), 120.0, 0.6); // 近接効果
        self.mic_a_tone[1].set_params(FilterType::Peaking(5.0 * (1.0 - m_a_x)), 3500.0, 1.2); // 軸上ピーク
        self.mic_a_tone[2].set_params(FilterType::LowPass, 14000.0 - (m_a_x * 7000.0), 0.7);

        // 4. Mic B (Ribbon / R-121) - 太い低域と滑らかな高域
        self.mic_b_tone[0].set_params(FilterType::Peaking((1.0 - m_b_d) * 14.0), 180.0, 0.4);
        self.mic_b_tone[1].set_params(FilterType::HighShelf(-6.0 * m_b_x), 3000.0, 0.7);
        self.mic_b_tone[2].set_params(FilterType::LowPass, 10000.0 - (m_b_d * 4000.0), 0.8);

        // 5. スタジオ・マスタリング・ロジック
        self.tight_shaper
            .set_params(FilterType::HighPass, 70.0 + (open * 50.0), 0.7);
        self.thump_dynamic
            .set_params(FilterType::Peaking(2.0 + thump * 3.0), 85.0, 1.2);
        self.fiz_cut_update(sparkle);
    }

    fn fiz_cut_update(&mut self, sparkle: f32) {
        self.fizzy_cut
            .set_params(FilterType::LowPass, 12000.0 - (sparkle * 2000.0), 0.8);
    }

    pub fn process_truce(&mut self, buffer: &mut AudioBuffer) {
        self.update_coefficients_if_needed();

        let master_gain = 10.0f32.powf(self.params.master_gain.value() / 20.0);
        let room_mix = self.params.room_mix.value();
        let m_a_d = self.params.mic_a_distance.value();
        let m_b_d = self.params.mic_b_distance.value();

        // マイク間タイムアライメント (物理的な奥行き感)
        let delay_samples = ((m_b_d - m_a_d).abs() * 0.0015 * self.sample_rate)
            .clamp(0.0, PHASE_DELAY_SIZE as f32 - 1.0);
        let (d_int, d_frac) = (delay_samples as usize, delay_samples.fract());

        let num_samples = buffer.num_samples();
        let num_channels = buffer.num_output_channels();

        for i in 0..num_samples {
            let input_sig = buffer.output(0)[i];
            let (l, r) = self.process_sample(input_sig, d_int, d_frac, room_mix);

            let l_final = l * master_gain;
            let r_final = r * master_gain;

            buffer.output(0)[i] = if num_channels >= 2 {
                buffer.output(1)[i] = r_final;
                l_final
            } else {
                (l_final + r_final) * 0.5
            };
        }
    }

    #[inline]
    fn process_sample(&mut self, mut sig: f32, d_int: usize, d_frac: f32, room_mix: f32) -> (f32, f32) {
        // --- 物理的非線形エミュレーション ---
        // 1. エンベロープ・フォロワー (応答の速いスピーカーの挙動)
        let target_env = sig.abs();
        self.envelope += (target_env - self.envelope)
            * if target_env > self.envelope {
                0.1
            } else {
                0.001
            };

        // 2. スピーカーの物理的サチュレーション (Soft Clipping)
        // 強い入力に対して、低域の制動がかかる様子を再現
        let drive = 1.0 + self.envelope * 0.5;
        sig = (sig * drive).tanh() / drive;

        // --- フィルターチェーン ---
        for res in &mut self.body_resonators {
            sig = res.process(sig);
        }
        for cone in &mut self.cone_character {
            sig = cone.process(sig);
        }

        // --- マイク・パス ---
        let mut sig_a = sig;
        for f in &mut self.mic_a_tone {
            sig_a = f.process(sig_a);
        }

        let mut sig_b = sig;
        for f in &mut self.mic_b_tone {
            sig_b = f.process(sig_b);
        }

        // 分散位相 (Delay Line)
        self.phase_delay_line[self.write_idx_phase] = sig_b;
        let r1 = (self.write_idx_phase + PHASE_DELAY_SIZE - d_int) & PHASE_DELAY_MASK;
        let r2 = (r1 + PHASE_DELAY_SIZE - 1) & PHASE_DELAY_MASK;
        sig_b = self.phase_delay_line[r1] * (1.0 - d_frac) + self.phase_delay_line[r2] * d_frac;
        self.write_idx_phase = (self.write_idx_phase + 1) & PHASE_DELAY_MASK;

        // --- ステレオ・イメージング ---
        // マイクの位相差を利用して広がりを作る
        let mut out_l = sig_a * 0.6 + sig_b * 0.4;
        let mut out_r = sig_a * 0.6 - sig_b * 0.3;

        // --- 最終動的補正 ---
        // Thump Dynamic: 強いアタック時に低域を少し「押し出す」
        let dynamic_gain = 1.0 + (self.envelope * 0.2);
        out_l = self.thump_dynamic.process(out_l * dynamic_gain) / dynamic_gain;
        out_r = self.thump_dynamic.process(out_r * dynamic_gain) / dynamic_gain;

        out_l = self.tight_shaper.process(out_l);
        out_r = self.tight_shaper.process(out_r);
        out_l = self.fizzy_cut.process(out_l);
        out_r = self.fizzy_cut.process(out_r);

        // --- Room Reflection (簡易的な初期反射) ---
        if room_mix > 0.0 {
            let delay_time = (0.02 * self.sample_rate) as usize;
            let read_idx = (self.write_idx_room + MAX_ROOM_DELAY - delay_time) % MAX_ROOM_DELAY;
            let reflection = self.room_buffer[read_idx] * 0.5;

            out_l += reflection * room_mix;
            out_r -= reflection * room_mix; // 位相反転で広げる

            self.room_buffer[self.write_idx_room] = (out_l + out_r) * 0.5;
            self.write_idx_room = (self.write_idx_room + 1) % MAX_ROOM_DELAY;
        }

        (out_l, out_r)
    }
}
