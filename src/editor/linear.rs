use crate::utils::FloatParamNormalizedExt;
use egui::{Align2, Color32, FontId, Rect, Response, Sense, Stroke, Ui, Widget, vec2};

pub struct LinearSlider<'a> {
    param: &'a truce::params::FloatParam,
    color: Color32,
}

impl<'a> LinearSlider<'a> {
    pub fn new(param: &'a truce::params::FloatParam, color: Color32) -> Self {
        Self { param, color }
    }

    /// Knob.rs / StackedKnob.rs と共通の動的カラー計算
    fn get_dynamic_color(base_color: Color32, visual_val: f32) -> Color32 {
        let r = (base_color.r() as f32 * (0.6 + visual_val * 0.4)) as u8;
        let g = (base_color.g() as f32 * (0.6 + visual_val * 0.4)) as u8;
        let b = (base_color.b() as f32 * (0.6 + visual_val * 0.4)) as u8;

        if visual_val > 0.85 {
            let boost = ((visual_val - 0.85) * 6.6 * 40.0) as u8;
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

impl<'a> Widget for LinearSlider<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let desired_size = vec2(120.0, 24.0);
        let (rect, response) = ui.allocate_at_least(desired_size, Sense::click_and_drag());

        let id = response.id;
        let text_edit_id = id.with("text_edit");
        let edit_string_id = id.with("edit_string");

        // 状態取得
        let mut is_editing = ui.memory(|m| m.data.get_temp::<bool>(text_edit_id).unwrap_or(false));
        let visual_val = self.param.value_normalized() as f32;
        let unit = self.param.info.unit.as_str();

        // --- 1. インタラクション ---

        // 右クリックでリセット
        if response.secondary_clicked() {
            self.param.set_value(self.param.info.default_plain);
            ui.memory_mut(|m| m.data.insert_temp(text_edit_id, false));
            is_editing = false;
        }

        // クリックでテキスト入力モードへ
        if response.clicked() && !is_editing {
            is_editing = true;
            ui.memory_mut(|m| {
                m.data.insert_temp(text_edit_id, true);
                m.data
                    .insert_temp(edit_string_id, format!("{:.1}", self.param.value()));
            });
        }

        // ドラッグ操作
        if response.dragged() && !is_editing {
            let delta = (response.drag_delta().x / rect.width()) as f64;
            if delta != 0.0 {
                let new_val = (self.param.value_normalized() + delta).clamp(0.0, 1.0);
                self.param.set_value_normalized(new_val);
            }
        }

        // --- 2. 描画 ---
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let bar_color = Self::get_dynamic_color(self.color, visual_val);

            // 背景
            painter.rect_filled(rect, 2.0, Color32::from_rgb(15, 15, 15));

            // フィットバー（左から現在の値まで）
            let fill_rect = Rect::from_min_max(
                rect.left_top(),
                egui::pos2(rect.left() + (visual_val * rect.width()), rect.bottom()),
            );
            painter.rect_filled(fill_rect, 2.0, bar_color.linear_multiply(0.8));

            // 外枠
            painter.rect_stroke(
                rect,
                2.0,
                Stroke::new(1.0, Color32::from_gray(60)),
                egui::StrokeKind::Middle,
            );

            // ハンドル（細い白い線）
            let handle_x = (rect.left() + visual_val * rect.width())
                .clamp(rect.left() + 1.0, rect.right() - 1.0);
            painter.line_segment(
                [
                    egui::pos2(handle_x, rect.top()),
                    egui::pos2(handle_x, rect.bottom()),
                ],
                Stroke::new(1.5, Color32::WHITE),
            );

            // テキスト表示
            if is_editing {
                let mut val_str = ui.memory(|m| {
                    m.data
                        .get_temp::<String>(edit_string_id)
                        .unwrap_or_else(|| format!("{:.1}", self.param.value()))
                });

                let res = ui.put(
                    rect.shrink(2.0),
                    egui::TextEdit::singleline(&mut val_str)
                        .font(FontId::monospace(11.0))
                        .horizontal_align(egui::Align::Center)
                        .frame(false),
                );

                if res.changed() {
                    ui.memory_mut(|m| m.data.insert_temp(edit_string_id, val_str.clone()));
                }
                if res.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if let Ok(v) = val_str.parse::<f64>() {
                        self.param.set_value(v);
                    }
                    ui.memory_mut(|m| m.data.insert_temp(text_edit_id, false));
                } else {
                    res.request_focus();
                }
            } else {
                // 表示用テキスト: 「名前: 値単位」
                let display_text = format!(
                    "{}: {:.1}{}",
                    self.param.info.name,
                    self.param.value(),
                    unit
                );
                let font_id = FontId::proportional(11.0);
                let text_pos = rect.center();

                // 1. シャドウ（視認性向上）
                painter.text(
                    text_pos + vec2(1.0, 1.0),
                    Align2::CENTER_CENTER,
                    &display_text,
                    font_id.clone(),
                    Color32::from_black_alpha(180),
                );

                // 2. 基本の暗いテキスト
                painter.text(
                    text_pos,
                    Align2::CENTER_CENTER,
                    &display_text,
                    font_id.clone(),
                    Color32::from_gray(160),
                );

                // 3. バーに重なっている部分だけを白抜きにするクリッピング描画
                painter.with_clip_rect(fill_rect).text(
                    text_pos,
                    Align2::CENTER_CENTER,
                    &display_text,
                    font_id,
                    Color32::WHITE,
                );
            }
        }

        if response.dragged() || is_editing {
            ui.ctx().request_repaint();
        }

        response
    }
}
