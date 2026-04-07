use nih_plug::editor::Editor;
use nih_plug_egui::create_egui_editor;

use crate::params::XrossGuitarAmpParams;

pub fn create_editor(params: &XrossGuitarAmpParams) -> Option<Box<dyn Editor>> {
    create_egui_editor(
        params.editor_state.clone(),
        (),
        |_cx, _state| {},
        move |egui_ctx, setter, _state| {},
    )
}
