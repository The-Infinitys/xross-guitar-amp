use truce::prelude::*;

#[derive(Params)]
pub struct XrossGuitarAmpParams {
    // ==========================================
    // 1. Gain Section (ID: 100 ~)
    // ==========================================
    #[param(
        id = 100,
        name = "Input Gain",
        range = "linear(-20.0, 20.0)",
        default = 0.0,
        unit = "dB",
        smooth = "exp(10)"
    )]
    pub input_gain: FloatParam,

    #[param(
        id = 101,
        name = "Drive",
        range = "linear(0.0, 1.0)",
        default = 0.5,
        smooth = "exp(10)"
    )]
    pub drive: FloatParam,

    #[param(
        id = 102,
        name = "Distortion",
        range = "linear(0.0, 1.0)",
        default = 0.5,
        smooth = "exp(10)"
    )]
    pub distortion: FloatParam,

    #[param(
        id = 103,
        name = "Master",
        range = "linear(-60.0, 0.0)",
        default = -6.0,
        unit = "dB",
        smooth = "exp(10)"
    )]
    pub master_gain: FloatParam,

    #[param(
        id = 104,
        name = "Style Low",
        range = "linear(0.0, 1.0)",
        default = 0.5,
        smooth = "linear(20)"
    )]
    pub style_low: FloatParam,

    #[param(
        id = 105,
        name = "Style Mid",
        range = "linear(0.0, 1.0)",
        default = 0.5,
        smooth = "linear(20)"
    )]
    pub style_mid: FloatParam,

    #[param(
        id = 106,
        name = "Style High",
        range = "linear(0.0, 1.0)",
        default = 0.5,
        smooth = "linear(20)"
    )]
    pub style_high: FloatParam,

    // ==========================================
    // 2. EQ Section (ID: 200 ~)
    // ==========================================
    #[param(
        id = 200,
        name = "Eq Low",
        range = "linear(-18.0, 18.0)",
        default = 0.0,
        unit = "dB",
        smooth = "exp(15)"
    )]
    pub eq_low: FloatParam,

    #[param(
        id = 201,
        name = "Eq Mid",
        range = "linear(-18.0, 18.0)",
        default = 0.0,
        unit = "dB",
        smooth = "exp(15)"
    )]
    pub eq_mid: FloatParam,

    #[param(
        id = 202,
        name = "Eq High",
        range = "linear(-18.0, 18.0)",
        default = 0.0,
        unit = "dB",
        smooth = "exp(15)"
    )]
    pub eq_high: FloatParam,

    #[param(
        id = 203,
        name = "Presence",
        range = "linear(0.0, 18.0)",
        default = 0.0,
        unit = "dB",
        smooth = "exp(15)"
    )]
    pub presence: FloatParam,

    #[param(
        id = 204,
        name = "Resonance",
        range = "linear(0.0, 18.0)",
        default = 0.0,
        unit = "dB",
        smooth = "exp(15)"
    )]
    pub resonance: FloatParam,

    // ==========================================
    // 3. Cab Section (ID: 300 ~)
    // ==========================================
    #[param(
        id = 300,
        name = "Speaker Size",
        range = "linear(8.0, 15.0)",
        default = 12.0,
        smooth = "linear(30)"
    )]
    pub speaker_size: FloatParam,

    // IntParamの範囲指定は `discrete(min, max)`。smoothは利用不可
    #[param(
        id = 301,
        name = "Speaker Count",
        range = "discrete(1, 8)",
        default = 4
    )]
    pub speaker_count: IntParam,

    #[param(
        id = 302,
        name = "Mic A Distance",
        range = "linear(0.0, 1.0)",
        default = 0.5,
        smooth = "linear(25)"
    )]
    pub mic_a_distance: FloatParam,

    #[param(
        id = 303,
        name = "Mic A Axis",
        range = "linear(0.0, 1.0)",
        default = 0.5,
        smooth = "linear(25)"
    )]
    pub mic_a_axis: FloatParam,

    #[param(
        id = 304,
        name = "Mic B Distance",
        range = "linear(0.0, 1.0)",
        default = 0.5,
        smooth = "linear(25)"
    )]
    pub mic_b_distance: FloatParam,

    #[param(
        id = 305,
        name = "Mic B Axis",
        range = "linear(0.0, 1.0)",
        default = 0.5,
        smooth = "linear(25)"
    )]
    pub mic_b_axis: FloatParam,

    #[param(
        id = 306,
        name = "Room Size",
        range = "linear(0.0, 1.0)",
        default = 0.3,
        smooth = "linear(30)"
    )]
    pub room_size: FloatParam,

    #[param(
        id = 307,
        name = "Room Mix",
        range = "linear(0.0, 1.0)",
        default = 0.1,
        smooth = "linear(20)"
    )]
    pub room_mix: FloatParam,

    // ==========================================
    // 4. Effects Section (ID: 400 ~)
    // ==========================================
    #[param(
        id = 400,
        name = "Sag",
        range = "linear(0.0, 1.0)",
        default = 0.2,
        smooth = "exp(30)"
    )]
    pub sag: FloatParam,

    #[param(
        id = 401,
        name = "Tight",
        range = "log(20.0, 500.0)", // 周波数のため linear から log に変更して操作性を向上
        default = 80.0,
        unit = "Hz",
        smooth = "exp(20)"
    )]
    pub tight: FloatParam,

    #[param(
        id = 402,
        name = "Reverb Mix",
        range = "linear(0.0, 1.0)",
        default = 0.1,
        smooth = "linear(20)"
    )]
    pub reverb_mix: FloatParam,

    // ==========================================
    // 5. Noise Gate Section (ID: 500 ~)
    // ==========================================
    #[param(
        id = 500,
        name = "Gate Threshold",
        range = "linear(-100.0, 0.0)",
        default = -60.0,
        unit = "dB",
        smooth = "exp(10)"
    )]
    pub gate_threshold: FloatParam,

    #[param(
        id = 501,
        name = "Gate Release",
        range = "linear(10.0, 500.0)",
        default = 70.0,
        unit = "ms",
        smooth = "exp(15)"
    )]
    pub gate_release: FloatParam,

    #[param(
        id = 502,
        name = "Gate Attack",
        range = "linear(0.1, 10.0)",
        default = 1.0,
        unit = "ms",
        smooth = "exp(10)"
    )]
    pub gate_attack: FloatParam,

    #[param(
        id = 503,
        name = "Gate Hold",
        range = "linear(0.0, 100.0)",
        default = 10.0,
        unit = "ms",
        smooth = "exp(15)"
    )]
    pub gate_hold: FloatParam,

    #[param(
        id = 504,
        name = "Gate Range",
        range = "linear(-100.0, 0.0)",
        default = -100.0,
        unit = "dB",
        smooth = "exp(15)"
    )]
    pub gate_range: FloatParam,

    // ==========================================
    // 6. Detailed Cab Section (ID: 600 ~)
    // ==========================================
    #[param(
        id = 600,
        name = "Cab Open Back",
        range = "linear(0.0, 1.0)",
        default = 0.0,
        smooth = "linear(25)"
    )]
    pub cab_open_back: FloatParam,

    #[param(
        id = 601,
        name = "Speaker Thump",
        range = "linear(0.0, 1.0)",
        default = 0.5,
        smooth = "exp(20)"
    )]
    pub speaker_thump: FloatParam,

    #[param(
        id = 602,
        name = "Speaker Sparkle",
        range = "linear(0.0, 1.0)",
        default = 0.5,
        smooth = "exp(20)"
    )]
    pub speaker_sparkle: FloatParam,
}
