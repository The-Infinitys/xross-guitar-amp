use std::sync::Arc;

use nih_plug::editor::Editor;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Color32, Frame},
};

use crate::params::XrossGuitarAmpParams;

pub fn create_editor(params: Arc<XrossGuitarAmpParams>) -> Option<Box<dyn Editor>> {
    create_egui_editor(
        params.editor_state.clone(),
        (),
        |_cx, _state| {},
        move |egui_ctx, setter, _state| {
            egui::CentralPanel::default()
                .frame(Frame::NONE.fill(Color32::BLACK))
                .show(egui_ctx, |ui| {});
        },
    )
}
