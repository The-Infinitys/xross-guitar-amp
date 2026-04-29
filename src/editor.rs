use std::sync::Arc;
use egui::{self, Color32, Frame, UiBuilder, Vec2, ecolor::Hsva};
use truce::core::Editor;
use truce_egui::EguiEditor;

use crate::params::XrossGuitarAmpParams;
mod background;
mod knob;
mod linear;
mod logo;
mod speaker;

use background::Background;
use knob::Knob;
use linear::LinearSlider;
use logo::Logo;
use speaker::SpeakerVisualizer;

fn get_vibrant_rainbow_color(index: usize, total: usize) -> Color32 {
    let h = (index as f32 / total as f32) * 0.85;
    Hsva::new(h, 1.0, 1.0, 1.0).into()
}

pub fn create_editor(params: Arc<XrossGuitarAmpParams>) -> Box<dyn Editor> {
    let width = 1280;
    let height = 900;
    let bg = Background::new();

    let editor = EguiEditor::new((width, height), move |egui_ctx, _state| {
        egui::CentralPanel::default()
            .frame(Frame::NONE)
            .show(egui_ctx, |ui| {
                bg.draw(ui);

                let mut color_idx = 0;
                let total_knobs = 12;

                let container_rect = ui.max_rect().shrink2(Vec2::new(40.0, 30.0));
                ui.allocate_new_ui(UiBuilder::new().max_rect(container_rect), |ui| {
                    ui.vertical(|ui| {
                        ui.vertical_centered(|ui| {
                            Logo::draw(ui, 50.0);
                        });
                        ui.add_space(15.0);

                        // --- 上段: アンプヘッド ---
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 15.0;

                            draw_section_weighted(ui, "GAIN", 4.0, |ui| {
                                ui.horizontal(|ui| {
                                    for k in
                                        [&params.input_gain, &params.drive, &params.distortion, &params.master_gain]
                                    {
                                        ui.add(Knob::new(
                                            k,
                                            get_vibrant_rainbow_color(color_idx, total_knobs),
                                        ));
                                        color_idx += 1;
                                    }
                                });
                            });

                            draw_section_weighted(ui, "EQUALIZER", 5.0, |ui| {
                                ui.horizontal(|ui| {
                                    for k in
                                        [&params.low, &params.mid, &params.high, &params.presence, &params.resonance]
                                    {
                                        ui.add(Knob::new(
                                            k,
                                            get_vibrant_rainbow_color(color_idx, total_knobs),
                                        ));
                                        color_idx += 1;
                                    }
                                });
                            });

                            draw_section_weighted(ui, "EFFECT", 3.0, |ui| {
                                ui.horizontal(|ui| {
                                    for k in [&params.sag, &params.tight, &params.reverb_mix] {
                                        ui.add(Knob::new(
                                            k,
                                            get_vibrant_rainbow_color(color_idx, total_knobs),
                                        ));
                                        color_idx += 1;
                                    }
                                });
                            });
                        });

                        ui.add_space(20.0);

                        // --- 下段: キャビネットセクション ---
                        let cab_height = ui.available_height() - 20.0;
                        draw_section_with_height(
                            ui,
                            "CABINET & DUAL MICROPHONES",
                            cab_height,
                            |ui| {
                                ui.vertical(|ui| {
                                    // 上部コントロール類
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 40.0;

                                        let mic_colors = [
                                            Color32::from_rgb(0, 180, 255),
                                            Color32::from_rgb(255, 100, 0),
                                        ];
                                        let mic_data = [
                                            (
                                                "MIC A (Left)",
                                                &params.mic_a_axis,
                                                &params.mic_a_distance,
                                            ),
                                            (
                                                "MIC B (Right)",
                                                &params.mic_b_axis,
                                                &params.mic_b_distance,
                                            ),
                                        ];

                                        for (i, (label, axis, dist)) in
                                            mic_data.iter().enumerate()
                                        {
                                            ui.vertical(|ui| {
                                                ui.label(
                                                    egui::RichText::new(*label)
                                                        .color(mic_colors[i])
                                                        .strong(),
                                                );
                                                ui.add(LinearSlider::new(
                                                    axis,
                                                    mic_colors[i],
                                                ));
                                                ui.add(LinearSlider::new(
                                                    dist,
                                                    mic_colors[i],
                                                ));
                                            });
                                        }

                                        ui.vertical(|ui| {
                                            ui.label(
                                                egui::RichText::new("Cabinet / Room").strong(),
                                            );
                                            ui.add(LinearSlider::new(
                                                &params.speaker_size,
                                                Color32::GOLD,
                                            ));
                                            ui.add(LinearSlider::new(
                                                &params.room_mix,
                                                Color32::WHITE,
                                            ));
                                        });

                                        ui.vertical(|ui| {
                                            ui.label(
                                                egui::RichText::new("Speaker Count").strong(),
                                            );
                                            ui.add_space(5.0);
                                            ui.horizontal(|ui| {
                                                for &count in &[1, 2, 4, 6, 8] {
                                                    let is_selected = params
                                                        .speaker_count
                                                        .value()
                                                        == count;
                                                    let btn =
                                                        egui::Button::new(count.to_string())
                                                            .fill(if is_selected {
                                                                Color32::from_rgb(0, 128, 0)
                                                            } else {
                                                                Color32::from_gray(60)
                                                            })
                                                            .min_size(Vec2::new(35.0, 25.0));
                                                    if ui.add(btn).clicked() {
                                                        params.speaker_count.set_value(count);
                                                    }
                                                }
                                            });
                                        });
                                    });

                                    ui.add_space(15.0);
                                    ui.separator();

                                    // --- ビジュアライザー ---
                                    let visualizer_area = ui.available_height() - 10.0;
                                    ui.vertical_centered(|ui| {
                                        SpeakerVisualizer::new(&params)
                                            .draw(ui, visualizer_area.min(450.0));
                                    });
                                });
                            },
                        );
                    });
                });
            });
    });
    Box::new(editor)
}

/// 重み付けされた幅でセクションを描画する
fn draw_section_weighted(
    ui: &mut egui::Ui,
    title: &str,
    weight: f32,
    add_contents: impl FnMut(&mut egui::Ui),
) {
    let total_weight = 12.0;
    let spacing = ui.spacing().item_spacing.x;
    let available_width = ui.available_width() - (spacing * 2.0);
    let width = (available_width * (weight / total_weight)).floor();

    ui.allocate_ui(Vec2::new(width, 0.0), |ui| {
        draw_section_with_height(ui, title, 0.0, add_contents);
    });
}

fn draw_section_with_height(
    ui: &mut egui::Ui,
    title: &str,
    height: f32,
    mut add_contents: impl FnMut(&mut egui::Ui),
) {
    Frame::NONE
        .fill(Color32::from_black_alpha(150))
        .stroke(egui::Stroke::new(1.0, Color32::from_gray(70)))
        .corner_radius(10.0)
        .inner_margin(15.0)
        .show(ui, |ui| {
            ui.set_min_height(height);
            ui.set_width(ui.available_width());
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(title)
                            .strong()
                            .color(Color32::from_gray(180))
                            .size(13.0),
                    );
                });

                ui.add_space(13.0);
                add_contents(ui);
            });
        });
}
