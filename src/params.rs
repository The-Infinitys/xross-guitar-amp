use truce::{Params, params::FloatParam, params::IntParam};

#[derive(Params)]
pub struct XrossGuitarAmpParams {
    // --- 1. Gain Section ---
    #[param(
        id = 1,
        name = "Input Gain",
        range = "linear(-20.0, 20.0)",
        default = 0.0,
        unit = "dB",
        smooth = "exp(50)"
    )]
    pub input_gain: FloatParam,

    #[param(
        id = 2,
        name = "Drive",
        range = "linear(0.0, 1.0)",
        default = 0.5,
        smooth = "exp(50)"
    )]
    pub drive: FloatParam,

    #[param(
        id = 3,
        name = "Distortion",
        range = "linear(0.0, 1.0)",
        default = 0.5,
        smooth = "exp(50)"
    )]
    pub distortion: FloatParam,

    #[param(
        id = 4,
        name = "Master",
        range = "linear(-60.0, 0.0)",
        default = -6.0,
        unit = "dB",
        smooth = "exp(50)"
    )]
    pub master_gain: FloatParam,

    #[param(
        id = 5,
        name = "Style Low",
        range = "linear(0.0, 1.0)",
        default = 0.5,
        smooth = "exp(50)"
    )]
    pub style_low: FloatParam,

    #[param(
        id = 6,
        name = "Style Mid",
        range = "linear(0.0, 1.0)",
        default = 0.5,
        smooth = "exp(50)"
    )]
    pub style_mid: FloatParam,

    #[param(
        id = 7,
        name = "Style High",
        range = "linear(0.0, 1.0)",
        default = 0.5,
        smooth = "exp(50)"
    )]
    pub style_high: FloatParam,

    // --- 2. EQ Section ---
    #[param(
        id = 8,
        name = "Eq Low",
        range = "linear(-18.0, 18.0)",
        default = 0.0,
        unit = "dB",
        smooth = "exp(50)"
    )]
    pub eq_low: FloatParam,

    #[param(
        id = 9,
        name = "Eq Mid",
        range = "linear(-18.0, 18.0)",
        default = 0.0,
        unit = "dB",
        smooth = "exp(50)"
    )]
    pub eq_mid: FloatParam,

    #[param(
        id = 10,
        name = "Eq High",
        range = "linear(-18.0, 18.0)",
        default = 0.0,
        unit = "dB",
        smooth = "exp(50)"
    )]
    pub eq_high: FloatParam,

    #[param(
        id = 11,
        name = "Presence",
        range = "linear(0.0, 18.0)",
        default = 0.0,
        unit = "dB",
        smooth = "exp(50)"
    )]
    pub presence: FloatParam,

    #[param(
        id = 12,
        name = "Resonance",
        range = "linear(0.0, 18.0)",
        default = 0.0,
        unit = "dB",
        smooth = "exp(50)"
    )]
    pub resonance: FloatParam,

    // --- 3. Cab Section ---
    #[param(
        id = 13,
        name = "Speaker Size",
        range = "linear(8.0, 15.0)",
        default = 12.0,
        smooth = "exp(50)"
    )]
    pub speaker_size: FloatParam,

    #[param(id = 14, name = "Speaker Count", range = "linear(1, 8)", default = 4)]
    pub speaker_count: IntParam,

    #[param(
        id = 15,
        name = "Mic A Distance",
        range = "linear(0.0, 1.0)",
        default = 0.5,
        smooth = "exp(50)"
    )]
    pub mic_a_distance: FloatParam,

    #[param(
        id = 16,
        name = "Mic A Axis",
        range = "linear(0.0, 1.0)",
        default = 0.5,
        smooth = "exp(50)"
    )]
    pub mic_a_axis: FloatParam,

    #[param(
        id = 17,
        name = "Mic B Distance",
        range = "linear(0.0, 1.0)",
        default = 0.5,
        smooth = "exp(50)"
    )]
    pub mic_b_distance: FloatParam,

    #[param(
        id = 18,
        name = "Mic B Axis",
        range = "linear(0.0, 1.0)",
        default = 0.5,
        smooth = "exp(50)"
    )]
    pub mic_b_axis: FloatParam,

    #[param(
        id = 19,
        name = "Room Size",
        range = "linear(0.0, 1.0)",
        default = 0.3,
        smooth = "exp(50)"
    )]
    pub room_size: FloatParam,

    #[param(
        id = 20,
        name = "Room Mix",
        range = "linear(0.0, 1.0)",
        default = 0.1,
        smooth = "exp(50)"
    )]
    pub room_mix: FloatParam,

    // --- 4. Effects Section ---
    #[param(
        id = 21,
        name = "Sag",
        range = "linear(0.0, 1.0)",
        default = 0.2,
        smooth = "exp(50)"
    )]
    pub sag: FloatParam,

    #[param(
        id = 22,
        name = "Tight",
        range = "linear(20.0, 500.0)",
        default = 80.0,
        unit = "Hz",
        smooth = "exp(50)"
    )]
    pub tight: FloatParam,

    #[param(
        id = 23,
        name = "Reverb Mix",
        range = "linear(0.0, 1.0)",
        default = 0.1,
        smooth = "exp(50)"
    )]
    pub reverb_mix: FloatParam,
}
