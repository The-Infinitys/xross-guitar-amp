use crate::modules::filter::{Biquad, FilterType};
use crate::params::XrossGuitarAmpParams;
use std::sync::Arc;

const PHASE_DELAY_SIZE: usize = 2048;
const PHASE_DELAY_MASK: usize = PHASE_DELAY_SIZE - 1;
const MAX_BUFFER_SIZE: usize = 192000;

pub struct CabProcessor {
    pub params: Arc<XrossGuitarAmpParams>,

    // Path A/B フィルタ群
    // [0]: 低域共振, [1]: 中低域の箱鳴り, [2]: Cone Breakup A, [3]: Cone Breakup B, [4]: LPF
    mic_a_filters: [Biquad; 5],
    mic_b_filters: [Biquad; 5],

    // 全体特性 (Power Amp / Cab Interaction)
    impedance_resonance: Biquad, // スピーカーのインピーダンスピーク
    presence_shelf: Biquad,      // 高域の伸び
    cabinet_thump: Biquad,       // 4x12特有の底鳴り
    tight_filter: Biquad,        // 不要な低域のカット

    // スピーカー固有の不規則な特性
    cone_character: [Biquad; 3],

    phase_delay_buffer_b: Vec<f32>,
    room_delay_buffer: Vec<f32>,
    reverb_buffer: Vec<f32>,

    write_idx_phase: usize,
    write_idx_room: usize,
    write_idx_rev: usize,

    sample_rate: f32,

    last_speaker_size: f32,
    last_speaker_count: i32,
    last_mic_params: [f32; 4],
    last_eq_extras: [f32; 2],
}

impl CabProcessor {
    pub fn new(params: Arc<XrossGuitarAmpParams>) -> Self {
        let sr = 44100.0;
        Self {
            params,
            mic_a_filters: [
                Biquad::new(sr),
                Biquad::new(sr),
                Biquad::new(sr),
                Biquad::new(sr),
                Biquad::new(sr),
            ],
            mic_b_filters: [
                Biquad::new(sr),
                Biquad::new(sr),
                Biquad::new(sr),
                Biquad::new(sr),
                Biquad::new(sr),
            ],
            impedance_resonance: Biquad::new(sr),
            presence_shelf: Biquad::new(sr),
            cabinet_thump: Biquad::new(sr),
            tight_filter: Biquad::new(sr),
            cone_character: [Biquad::new(sr), Biquad::new(sr), Biquad::new(sr)],
            phase_delay_buffer_b: vec![0.0; PHASE_DELAY_SIZE],
            room_delay_buffer: vec![0.0; MAX_BUFFER_SIZE],
            reverb_buffer: vec![0.0; MAX_BUFFER_SIZE],
            write_idx_phase: 0,
            write_idx_room: 0,
            write_idx_rev: 0,
            sample_rate: sr,
            last_speaker_size: -1.0,
            last_speaker_count: -1,
            last_mic_params: [-1.0; 4],
            last_eq_extras: [-1.0; 2],
        }
    }

    pub fn initialize(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        // フィルタ群のサンプリングレートをすべて更新
        for f in &mut self.mic_a_filters {
            f.set_sample_rate(sample_rate);
        }
        for f in &mut self.mic_b_filters {
            f.set_sample_rate(sample_rate);
        }
        for f in &mut self.cone_character {
            f.set_sample_rate(sample_rate);
        }
        self.impedance_resonance.set_sample_rate(sample_rate);
        self.presence_shelf.set_sample_rate(sample_rate);
        self.cabinet_thump.set_sample_rate(sample_rate);
        self.tight_filter.set_sample_rate(sample_rate);

        self.room_delay_buffer.resize(sample_rate as usize, 0.0);
        self.reverb_buffer.resize(sample_rate as usize, 0.0);
        self.reset();
    }

    pub fn reset(&mut self) {
        for f in &mut self.mic_a_filters {
            f.reset();
        }
        for f in &mut self.mic_b_filters {
            f.reset();
        }
        for f in &mut self.cone_character {
            f.reset();
        }
        self.impedance_resonance.reset();
        self.presence_shelf.reset();
        self.cabinet_thump.reset();
        self.tight_filter.reset();
        self.phase_delay_buffer_b.fill(0.0);
        self.room_delay_buffer.fill(0.0);
        self.reverb_buffer.fill(0.0);
        // 次回のprocessで確実に係数を再計算させる
        self.last_speaker_size = -1.0;
    }

    fn update_coefficients(&mut self) {
        let cab = &self.params.cab_section;
        let eq = &self.params.eq_section;

        let s_size = cab.speaker_size.value();
        let s_count = cab.speaker_count.value();
        let d_a = cab.mic_a_distance.value();
        let a_a = cab.mic_a_axis.value();
        let d_b = cab.mic_b_distance.value();
        let a_b = cab.mic_b_axis.value();

        let res_val = eq.resonance.value();
        let pres_val = eq.presence.value();

        if (s_size - self.last_speaker_size).abs() > 0.001
            || s_count != self.last_speaker_count
            || (d_a - self.last_mic_params[0]).abs() > 0.001
            || (a_a - self.last_mic_params[1]).abs() > 0.001
            || (d_b - self.last_mic_params[2]).abs() > 0.001
            || (a_b - self.last_mic_params[3]).abs() > 0.001
            || (res_val - self.last_eq_extras[0]).abs() > 0.001
            || (pres_val - self.last_eq_extras[1]).abs() > 0.001
        {
            // --- 物理ベースの基礎設定 ---
            let speaker_res_freq = 82.0 * (12.0 / s_size);
            let count_scale = (s_count as f32).sqrt();

            // 1. パワーアンプの相互作用 (Impedance Resonance)
            self.impedance_resonance.set_params(
                FilterType::Peaking(res_val * 1.2),
                speaker_res_freq,
                1.8,
            );
            self.presence_shelf
                .set_params(FilterType::HighShelf(pres_val * 0.7), 4500.0, 0.7);

            // 2. スピーカー固有の癖 (Cone resonances)
            // 800Hzのディップ、2.5kHzのピーク、3.8kHzの鋭いピーク
            self.cone_character[0].set_params(FilterType::Peaking(-3.5), 850.0, 1.5);
            self.cone_character[1].set_params(FilterType::Peaking(2.8), 2400.0, 1.2);
            self.cone_character[2].set_params(FilterType::Peaking(4.2), 3800.0, 2.5);

            // 3. Path A (SM57風: 芯のあるサウンド)
            // 強力な近接効果
            let prox_a = (1.0 - d_a).powi(3) * 12.0;
            self.mic_a_filters[0].set_params(
                FilterType::Peaking(prox_a + count_scale * 1.5),
                speaker_res_freq,
                1.4,
            );
            // 中域の押し出し
            self.mic_a_filters[1].set_params(FilterType::Peaking(2.5), 550.0, 0.8);
            // エッジの強調 (Axis依存)
            let edge_a = (1.0 - a_a) * 5.5;
            self.mic_a_filters[2].set_params(FilterType::Peaking(edge_a), 3500.0, 1.2);
            self.mic_a_filters[3].set_params(FilterType::Peaking(edge_a * 0.5), 5200.0, 2.0);
            // LPF (AxisとDistanceによる減衰)
            let hc_a = 14000.0 * (1.0 - a_a * 0.6) * (1.0 - d_a * 0.2);
            self.mic_a_filters[4].set_params(FilterType::LowPass, hc_a.max(3200.0), 0.707);

            // 4. Path B (Ribbon風: 太く滑らか)
            let prox_b = (1.0 - d_b).powi(2) * 15.0;
            self.mic_b_filters[0].set_params(
                FilterType::Peaking(prox_b + count_scale * 2.5),
                speaker_res_freq * 0.95,
                1.0,
            );
            self.mic_b_filters[1].set_params(FilterType::Peaking(4.0), 350.0, 0.6);
            self.mic_b_filters[2].set_params(FilterType::Peaking(1.5), 2200.0, 0.7);
            self.mic_b_filters[3].set_params(FilterType::Peaking(-2.0), 4500.0, 1.5);
            let hc_b = 10000.0 * (1.0 - a_b * 0.7) * (1.0 - d_b * 0.4);
            self.mic_b_filters[4].set_params(FilterType::LowPass, hc_b.max(2200.0), 0.707);

            // 5. キャビネット全体
            self.cabinet_thump.set_params(
                FilterType::Peaking(2.5 * count_scale),
                140.0 * (12.0 / s_size),
                2.5,
            );
            self.tight_filter
                .set_params(FilterType::HighPass, 60.0, 0.6);

            self.last_speaker_size = s_size;
            self.last_speaker_count = s_count;
            self.last_mic_params = [d_a, a_a, d_b, a_b];
            self.last_eq_extras = [res_val, pres_val];
        }
    }

    pub fn process(&mut self, input: f32) -> (f32, f32) {
        self.update_coefficients();

        // 1. 全体物理特性
        let mut sig = self.impedance_resonance.process(input);
        sig = self.presence_shelf.process(sig);
        sig = self.cabinet_thump.process(sig);
        sig = self.tight_filter.process(sig);

        for f in &mut self.cone_character {
            sig = f.process(sig);
        }

        // 2. Path A
        let mut sig_a = sig;
        for f in &mut self.mic_a_filters {
            sig_a = f.process(sig_a);
        }

        // 3. Path B
        let mut sig_b = sig;
        for f in &mut self.mic_b_filters {
            sig_b = f.process(sig_b);
        }

        // 4. マイク距離による位相差 (より忠実な遅延量)
        let delay_a = self.params.cab_section.mic_a_distance.value() * 0.0015 * self.sample_rate;
        let delay_b = self.params.cab_section.mic_b_distance.value() * 0.0045 * self.sample_rate;

        let delay_float = (delay_b - delay_a).max(0.1);
        let delay_int = delay_float as usize;
        let frac = delay_float - (delay_int as f32);

        self.phase_delay_buffer_b[self.write_idx_phase] = sig_b;
        let r_idx1 = (self.write_idx_phase + PHASE_DELAY_SIZE - delay_int) & PHASE_DELAY_MASK;
        let r_idx2 = (r_idx1 + PHASE_DELAY_SIZE - 1) & PHASE_DELAY_MASK;
        sig_b = self.phase_delay_buffer_b[r_idx1] * (1.0 - frac)
            + self.phase_delay_buffer_b[r_idx2] * frac;
        self.write_idx_phase = (self.write_idx_phase + 1) & PHASE_DELAY_MASK;

        // 5. ステレオミックス
        let mut out_l = sig_a * 0.85 + sig_b * 0.35;
        let mut out_r = sig_a * 0.35 + sig_b * 0.85;

        // 6. ルーム / リバーブ (より密度の高い反射)
        let room_mix = self.params.cab_section.room_mix.value();
        if room_mix > 0.0 {
            let room_size = self.params.cab_section.room_size.value();
            let delay_samples = ((0.018 + room_size * 0.042) * self.sample_rate) as usize;
            let read_idx = (self.write_idx_room + self.room_delay_buffer.len() - delay_samples)
                % self.room_delay_buffer.len();
            let reflection = self.room_delay_buffer[read_idx];

            out_l += reflection * room_mix * 0.4;
            out_r += reflection * room_mix * 0.4;

            self.room_delay_buffer[self.write_idx_room] = (sig_a + sig_b) * 0.5;
            self.write_idx_room = (self.write_idx_room + 1) % self.room_delay_buffer.len();
        }

        let reverb_mix = self.params.fx_section.reverb_mix.value();
        if reverb_mix > 0.0 {
            let rev_delay = (0.082 * self.sample_rate) as usize;
            let read_idx = (self.write_idx_rev + self.reverb_buffer.len() - rev_delay)
                % self.reverb_buffer.len();
            let rev_sig = self.reverb_buffer[read_idx];

            out_l += rev_sig * reverb_mix * 0.3;
            out_r += rev_sig * reverb_mix * 0.3;

            self.reverb_buffer[self.write_idx_rev] = (sig_a + sig_b) * 0.5 + rev_sig * 0.6;
            self.write_idx_rev = (self.write_idx_rev + 1) % self.reverb_buffer.len();
        }

        (out_l * 1.25, out_r * 1.25)
    }
}
