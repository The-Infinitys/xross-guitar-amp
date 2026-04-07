use nih_plug::prelude::*;
use nih_plug_egui::{
    EguiState, create_egui_editor,
    egui::{self, Align, Color32, Frame, Layout, UiBuilder, Vec2, ecolor::Hsva},
};
use std::sync::Arc;

use crate::params::XrossGuitarAmpParams;
mod background;
mod knob;

use background::Background;
use knob::Knob;

/// 彩度を1.0(最大)に引き上げたネオン虹色
fn get_vibrant_rainbow_color(index: usize, total: usize) -> Color32 {
    let h = (index as f32 / total as f32) * 0.85;
    // S(彩度)を1.0, V(明度)を1.0に固定して最も鮮やかに
    Hsva::new(h, 1.0, 1.0, 1.0).into()
}

pub fn create_editor(params: Arc<XrossGuitarAmpParams>) -> Option<Box<dyn Editor>> {
    let width = 1000;
    let height = 450;
    let bg = Background::new();

    create_egui_editor(
        EguiState::from_size(width, height),
        (),
        |_cx, _state| {},
        move |egui_ctx, setter, _state| {
            egui::CentralPanel::default()
                .frame(Frame::NONE)
                .show(egui_ctx, |ui| {
                    bg.draw(ui);

                    let margin = 30.0;
                    let spacing = 18.0;
                    let total_knobs = 9;

                    ui.allocate_new_ui(
                        UiBuilder::new().max_rect(ui.max_rect().shrink(margin)),
                        |ui| {
                            ui.vertical_centered(|ui| {
                                ui.add_space(10.0);
                                ui.heading(
                                    egui::RichText::new("XROSS DIGITAL AMP")
                                        .strong()
                                        .size(28.0)
                                        .color(Color32::WHITE)
                                        .extra_letter_spacing(2.0),
                                );
                                ui.add_space(35.0);

                                let avail_width = ui.available_width() - (spacing * 2.0);
                                let unit_width = avail_width / total_knobs as f32;

                                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                    ui.spacing_mut().item_spacing.x = spacing;

                                    // --- PREAMP (2 knobs) ---
                                    draw_section(ui, "PREAMP", unit_width * 2.0, |ui| {
                                        ui.add(Knob::new(
                                            &params.gain,
                                            setter,
                                            get_vibrant_rainbow_color(0, total_knobs),
                                        ));
                                        ui.add(Knob::new(
                                            &params.tight,
                                            setter,
                                            get_vibrant_rainbow_color(1, total_knobs),
                                        ));
                                    });

                                    // --- ACTIVE EQ (3 knobs) ---
                                    draw_section(ui, "ACTIVE EQ", unit_width * 3.0, |ui| {
                                        ui.add(Knob::new(
                                            &params.bass,
                                            setter,
                                            get_vibrant_rainbow_color(2, total_knobs),
                                        ));
                                        ui.add(Knob::new(
                                            &params.middle,
                                            setter,
                                            get_vibrant_rainbow_color(3, total_knobs),
                                        ));
                                        ui.add(Knob::new(
                                            &params.treble,
                                            setter,
                                            get_vibrant_rainbow_color(4, total_knobs),
                                        ));
                                    });

                                    // --- MASTER (4 knobs) ---
                                    draw_section(ui, "MASTER", unit_width * 4.0, |ui| {
                                        ui.add(Knob::new(
                                            &params.presence,
                                            setter,
                                            get_vibrant_rainbow_color(5, total_knobs),
                                        ));
                                        ui.add(Knob::new(
                                            &params.resonance,
                                            setter,
                                            get_vibrant_rainbow_color(6, total_knobs),
                                        ));
                                        ui.add(Knob::new(
                                            &params.sag,
                                            setter,
                                            get_vibrant_rainbow_color(7, total_knobs),
                                        ));
                                        ui.add(Knob::new(
                                            &params.master_gain,
                                            setter,
                                            Color32::WHITE,
                                        ));
                                    });
                                });
                            });
                        },
                    );
                });
        },
    )
}

fn draw_section(
    ui: &mut egui::Ui,
    title: &str,
    width: f32,
    mut add_contents: impl FnMut(&mut egui::Ui),
) {
    let height = 260.0;

    Frame::NONE
        .fill(Color32::from_black_alpha(180)) // コントラストを高めるため少し暗く
        .stroke(egui::Stroke::new(1.0, Color32::from_gray(80)))
        .corner_radius(10.0)
        .show(ui, |ui| {
            // ボックスサイズを強制固定
            ui.set_min_size(Vec2::new(width, height));
            ui.set_max_size(Vec2::new(width, height));

            ui.vertical(|ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new(title)
                            .strong()
                            .color(Color32::from_gray(200))
                            .size(13.0),
                    );
                });

                // コンテンツエリアを中央配置
                ui.centered_and_justified(|ui| {
                    // 内側に水平中央寄せのレイアウトを配置
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 20.0; // ノブ間の余白
                        add_contents(ui);
                    });
                });
            });
        });
}
