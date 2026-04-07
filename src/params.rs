use nih_plug::params::{FloatParam, Params};
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Params)]
pub struct XrossGuitarAmpParams {
    // --- 1. Preamp Section ---
    #[id = "gain"]
    pub gain: FloatParam,

    #[id = "tight"]
    pub tight: FloatParam,

    // --- 2. Active EQ Section ---
    #[id = "bass"]
    pub bass: FloatParam,

    #[id = "middle"]
    pub middle: FloatParam,

    #[id = "treble"]
    pub treble: FloatParam,

    // --- 3. Power Amp Section ---
    #[id = "presence"]
    pub presence: FloatParam,

    #[id = "resonance"]
    pub resonance: FloatParam,

    #[id = "sag"]
    pub sag: FloatParam,

    // --- 4. Master Section ---
    #[id = "master_gain"]
    pub master_gain: FloatParam,
}

impl Default for XrossGuitarAmpParams {
    fn default() -> Self {
        Self {
            // Gain: 小数点なし (例: 20 dB)
            gain: FloatParam::new(
                "Gain",
                20.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 60.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(Arc::new(|x| format!("{:.0}", x))),

            // Tight: 小数点なし (例: 80 Hz)
            tight: FloatParam::new(
                "Tight",
                80.0,
                FloatRange::Skewed {
                    min: 20.0,
                    max: 500.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(Arc::new(|x| format!("{:.0}", x))),

            // Active EQ: 小数点なし (例: +12 dB / -5 dB)
            bass: FloatParam::new(
                "Bass",
                0.0,
                FloatRange::Linear {
                    min: -12.0,
                    max: 12.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(Arc::new(|x| format!("{:.0}", x))),

            middle: FloatParam::new(
                "Middle",
                0.0,
                FloatRange::Linear {
                    min: -12.0,
                    max: 12.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(Arc::new(|x| format!("{:.0}", x))),

            treble: FloatParam::new(
                "Treble",
                0.0,
                FloatRange::Linear {
                    min: -12.0,
                    max: 12.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(Arc::new(|x| format!("{:.0}", x))),

            // Presence / Resonance: 小数点なし
            presence: FloatParam::new(
                "Presence",
                0.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 12.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(Arc::new(|x| format!("{:.0}", x))),

            resonance: FloatParam::new(
                "Resonance",
                0.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 12.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(Arc::new(|x| format!("{:.0}", x))),

            // Sag: 小数点1位まで (例: 0.2 / 1.0)
            sag: FloatParam::new("Sag", 0.2, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|x| format!("{:.1}", x))),

            // Master: 小数点1位まで (例: -6.0 dB / -10.5 dB)
            master_gain: FloatParam::new(
                "Master",
                -6.0,
                FloatRange::Linear {
                    min: -60.0,
                    max: 0.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(Arc::new(|x| format!("{:.1}", x))),
        }
    }
}
