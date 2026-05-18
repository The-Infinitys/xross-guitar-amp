use truce::prelude::*;
mod amp;
mod editor;
mod modules;
mod params;
mod plugin;
mod utils;

use amp::XrossGuitarAmp;
use params::XrossGuitarAmpParams;

truce::plugin! {
    logic: XrossGuitarAmp,
    params: XrossGuitarAmpParams,
}
