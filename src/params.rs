use std::sync::Arc;

use nih_plug::params::Params;
use nih_plug_egui::EguiState;

#[derive(Params)]
pub struct XrossGuitarAmpParams {
    #[persist = "editor_state"]
    pub editor_state: Arc<EguiState>,
}
impl Default for XrossGuitarAmpParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(1200, 720),
        }
    }
}
