use egui::{Align2, Color32, FontId, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Widget, vec2};
use std::f32::consts::PI;
use crate::utils::FloatParamNormalizedExt;

pub struct Knob<'a> {
    param: &'a truce::params::FloatParam,
    base_color: Color32,
}

impl<'a> Knob<'a> {
    pub fn new(param: &'a truce::params::FloatParam, color: Color32) -> Self {
        Self {
            param,
            base_color: color,
        }
    }

    /// 値に応じて色を動的に変化させる（高い値ほど鮮やかに）
    fn get_dynamic_color(&self, visual_val: f32) -> Color32 {
        let r = (self.base_color.r() as f32 * (0.5 + visual_val * 0.5)) as u8;
        let g = (self.base_color.g() as f32 * (0.5 + visual_val * 0.5)) as u8;
        let b = (self.base_color.b() as f32 * (0.5 + visual_val * 0.5)) as u8;

        if visual_val > 0.8 {
            let boost = ((visual_val - 0.8) * 5.0 * 50.0) as u8;
            Color32::from_rgb(
                r.saturating_add(boost),
                g.saturating_add(boost),
                b.saturating_add(boost),
            )
        } else {
            Color32::from_rgb(r, g, b)
        }
    }
}

impl<'a> Widget for Knob<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        // --- 1. レイアウト計算 ---
        let desired_size = vec2(80.0, 110.0);
        let (rect, response) = ui.allocate_at_least(desired_size, Sense::click_and_drag());

        let id = response.id;
        let text_edit_id = id.with("text_edit");
        let edit_string_id = id.with("edit_string");

        // インタラクション
        if response.double_clicked() {
            self.param.set_value(self.param.info.default_plain);
            ui.memory_mut(|mem| mem.data.insert_temp(text_edit_id, false));
        }

        if response.dragged() {
            let delta = -response.drag_delta().y * 0.005;
            let current_norm = self.param.value_normalized();
            let new_norm = (current_norm + delta as f64).clamp(0.0, 1.0);
            self.param.set_value_normalized(new_norm);
        }

        let visual_val = self.param.value_normalized() as f32;

        // --- 2. 描画 ---
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let active_color = self.get_dynamic_color(visual_val);

            // 各セクションの矩形
            let title_rect = Rect::from_min_max(rect.min, rect.min + vec2(rect.width(), 20.0));
            let value_rect = Rect::from_min_max(rect.max - vec2(rect.width(), 25.0), rect.max);

            // 中央の正方形ノブ領域
            let knob_area_width = rect.width().min(rect.height() - 45.0);
            let knob_rect = Rect::from_center_size(
                rect.center() + vec2(0.0, -2.0),
                egui::Vec2::splat(knob_area_width),
            );

            // A. タイトル表示
            painter.text(
                title_rect.center(),
                Align2::CENTER_CENTER,
                &self.param.info.name,
                FontId::proportional(13.0),
                Color32::from_gray(200),
            );

            // B. ノブ本体
            let center = knob_rect.center();
            let radius = knob_rect.width() * 0.4;
            let start_angle = PI * 0.75;
            let end_angle = PI * 2.25;
            let current_angle = start_angle + (visual_val * (end_angle - start_angle));

            // 背景（溝）
            painter.circle_filled(center, radius + 2.0, Color32::BLACK);
            painter.circle_stroke(
                center,
                radius + 4.0,
                Stroke::new(2.0, Color32::from_gray(40)),
            );

            // 円弧インジケーター
            let n_points = 32;
            let current_n = (n_points as f32 * visual_val).ceil() as usize;
            let arc_points: Vec<Pos2> = (0..=current_n)
                .map(|i| {
                    let a = start_angle + (i as f32 / n_points as f32) * (end_angle - start_angle);
                    center + vec2(a.cos(), a.sin()) * (radius + 6.0)
                })
                .collect();

            if arc_points.len() > 1 {
                painter.add(Shape::line(arc_points, Stroke::new(3.0, active_color)));
            }

            // ノブ本体と指針
            painter.circle_filled(center, radius, Color32::from_gray(25));
            let tip = center + vec2(current_angle.cos(), current_angle.sin()) * radius;
            let base = center + vec2(current_angle.cos(), current_angle.sin()) * (radius * 0.4);
            painter.line_segment([base, tip], Stroke::new(3.0, active_color));

            // C. 数値表示エリア
            let is_editing =
                ui.memory(|mem| mem.data.get_temp::<bool>(text_edit_id).unwrap_or(false));

            if is_editing {
                let mut value_text = ui.memory(|mem| {
                    mem.data
                        .get_temp::<String>(edit_string_id)
                        .unwrap_or_else(|| format!("{:.1}", self.param.value()))
                });

                let res = ui.put(
                    value_rect,
                    egui::TextEdit::singleline(&mut value_text)
                        .font(FontId::proportional(12.0))
                        .horizontal_align(egui::Align::Center)
                        .frame(true),
                );

                if res.changed() {
                    ui.memory_mut(|mem| mem.data.insert_temp(edit_string_id, value_text.clone()));
                }
                if res.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if let Ok(parsed) = value_text.parse::<f64>() {
                        self.param.set_value(parsed);
                    }
                    ui.memory_mut(|mem| mem.data.insert_temp(text_edit_id, false));
                } else {
                    res.request_focus();
                }
            } else {
                let val_res = ui.interact(value_rect, id.with("val_interact"), Sense::click());
                painter.rect_filled(value_rect.shrink(2.0), 4.0, Color32::from_black_alpha(100));

                let display_text = format!("{:.1}", self.param.value());

                painter.text(
                    value_rect.center(),
                    Align2::CENTER_CENTER,
                    display_text,
                    FontId::monospace(12.0),
                    active_color,
                );
                if val_res.clicked() {
                    ui.memory_mut(|mem| mem.data.insert_temp(text_edit_id, true));
                }
            }
        }

        if response.dragged() {
            ui.ctx().request_repaint();
        }

        response
    }
}
