use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontFamily, FontId, Frame, Layout, Margin, Pos2,
    Rect, RichText, Sense, Stroke, StrokeKind, Vec2,
};

use crate::app::*;
use crate::model::{EditAction, EditItem, Tab};
use crate::util;

/// %100 yakınlaştırma: 1 PDF noktası = 96/72 mantıksal piksel (ekranda gerçek boyut).
const ZOOM_100: f32 = 96.0 / 72.0;
const ZOOM_MIN: f32 = 0.10;
const ZOOM_MAX: f32 = 5.0;
const ZOOM_MIN_PCT: f32 = 10.0;
const ZOOM_MAX_PCT: f32 = 500.0;
const ZOOM_STEPS: [f32; 13] = [
    10.0, 25.0, 33.0, 50.0, 67.0, 75.0, 100.0, 125.0, 150.0, 200.0, 300.0, 400.0, 500.0,
];

fn zoom_step_pct(current: f32, up: bool) -> f32 {
    if up {
        ZOOM_STEPS
            .iter()
            .copied()
            .find(|s| *s > current + 0.5)
            .unwrap_or(ZOOM_MAX_PCT)
    } else {
        ZOOM_STEPS
            .iter()
            .rev()
            .copied()
            .find(|s| *s < current - 0.5)
            .unwrap_or(ZOOM_MIN_PCT)
    }
}

impl PdfToolboxApp {
    pub fn ui_edit(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.page_title(ui, "PDF Düzenle", NAVY);
        ui.label(
            RichText::new(
                "Metin, şekil, resim ve filigran ekleyin; sayfaları döndürün veya silin. Mevcut metinleri yaklaşık olarak değiştirebilirsiniz.",
            )
            .size(12.0)
            .color(MUTED),
        );
        ui.add_space(10.0);

        card(ui, "Belge", |ui| {
            ui.horizontal(|ui| {
                if action_button(ui, "PDF Aç", ORANGE).clicked() {
                    if let Some(path) = pick_file("Düzenlenecek PDF'yi seç", "PDF", &["pdf"]) {
                        self.open_edit(ctx, path);
                    }
                }
                if let Some(path) = &self.edit.path {
                    ui.label(
                        RichText::new(util::file_name(path))
                            .size(12.5)
                            .strong()
                            .color(INK),
                    );
                    ui.label(
                        RichText::new(format!("{} sayfa", self.edit.total))
                            .size(11.5)
                            .color(MUTED),
                    );
                } else {
                    ui.label(RichText::new("PDF seçilmedi").size(12.0).color(MUTED));
                }
                if self.edit.total > 0 {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("Önizlemeyi Yenile").clicked() {
                            self.reload_edit_page(ctx);
                        }
                        let mut page = self.edit.page;
                        let total = self.edit.total;
                        let resp = ui.add(
                            egui::DragValue::new(&mut page)
                                .range(1..=total.max(1))
                                .speed(0.15),
                        );
                        ui.label(RichText::new("Sayfa").size(11.5).color(MUTED));
                        if resp.changed() && page != self.edit.page {
                            self.edit.page = page;
                            self.edit.selected = None;
                            self.edit.pending_pos = None;
                            self.reload_edit_page(ctx);
                        }
                    });
                }
            });
        });

        if self.edit.bytes.is_empty() {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("Düzenlemek için bir PDF açın")
                        .size(16.0)
                        .color(MUTED),
                );
                ui.label(
                    RichText::new("Dosyayı bu pencereye sürükleyebilirsiniz.")
                        .size(12.0)
                        .color(MUTED),
                );
            });
            return;
        }

        let canvas_height = (ctx.screen_rect().height() - 360.0).clamp(380.0, 760.0);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(126.0, canvas_height + 30.0),
                Layout::top_down(Align::Min),
                |ui| {
                    self.edit_tools(ui, ctx);
                },
            );
            ui.add_space(6.0);
            let remaining = ui.available_width();
            let canvas_w = (remaining - 320.0).max(240.0);
            ui.allocate_ui_with_layout(
                Vec2::new(canvas_w, canvas_height + 30.0),
                Layout::top_down(Align::Min),
                |ui| {
                    self.edit_canvas(ui, ctx, canvas_height);
                },
            );
            ui.add_space(6.0);
            ui.allocate_ui_with_layout(
                Vec2::new(300.0, canvas_height + 30.0),
                Layout::top_down(Align::Min),
                |ui| {
                    self.edit_props(ui);
                },
            );
        });

        ui.add_space(8.0);
        card(ui, "Değişiklikler", |ui| {
            ui.horizontal(|ui| {
                let can_undo = self.edit.cursor > 0;
                let can_redo = self.edit.cursor < self.edit.actions.len();
                ui.add_enabled_ui(can_undo, |ui| {
                    if ghost_button(ui, "Geri Al").clicked() {
                        self.edit.undo();
                    }
                });
                ui.add_enabled_ui(can_redo, |ui| {
                    if ghost_button(ui, "İleri Al").clicked() {
                        self.edit.redo();
                    }
                });
                if ghost_button(ui, "Temizle").clicked() && !self.edit.actions.is_empty() {
                    self.confirm = Some(ConfirmBox {
                        title: "Değişiklikleri Temizle".to_string(),
                        text: "Tüm bekleyen düzenlemeler silinsin mi?".to_string(),
                        action: ConfirmAction::ClearEditActions,
                    });
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} değişiklik", self.edit.current().len()))
                            .size(11.5)
                            .color(MUTED),
                    );
                });
            });
            ui.add_space(4.0);
            let items: Vec<String> = self
                .edit
                .current()
                .iter()
                .rev()
                .take(10)
                .map(|i| i.label())
                .collect();
            if items.is_empty() {
                ui.label(
                    RichText::new("Henüz değişiklik yok. Soldaki araçlarla sayfaya ekleme yapın.")
                        .size(11.5)
                        .color(MUTED),
                );
            } else {
                egui::ScrollArea::vertical()
                    .max_height(120.0)
                    .id_salt("edit_action_list")
                    .show(ui, |ui| {
                        for label in items {
                            ui.label(RichText::new(format!("• {label}")).size(11.0).color(INK));
                        }
                    });
            }
            ui.add_space(6.0);
            let running = self.is_running(Tab::Edit);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!running, |ui| {
                    if action_button(ui, "PDF Olarak Kaydet", GREEN).clicked() {
                        self.start_edit_save(ctx);
                    }
                });
                if self.fonts.is_none() {
                    ui.label(
                        RichText::new("Yazı tipi bulunamadı; metin ekleme çalışmayabilir.")
                            .size(10.5)
                            .color(RED),
                    );
                }
            });
            ui.add_space(4.0);
            self.progress_ui(ui, Tab::Edit);
            self.status_label(ui, Tab::Edit);
        });
    }

    fn edit_tools(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        Frame::new()
            .fill(CARD)
            .stroke(Stroke::new(1.0_f32, BORDER))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::same(8))
            .show(ui, |ui| {
                ui.set_width(108.0);
                ui.label(RichText::new("Araçlar").size(12.0).strong().color(INK));
                ui.add_space(4.0);
                for tool in [
                    EditTool::Text,
                    EditTool::Rectangle,
                    EditTool::Circle,
                    EditTool::Line,
                    EditTool::Image,
                    EditTool::TextSelect,
                ] {
                    let selected = self.edit.tool == tool;
                    let color = if selected {
                        ORANGE
                    } else {
                        Color32::from_rgb(241, 245, 249)
                    };
                    let text_color = if selected { Color32::WHITE } else { INK };
                    let button =
                        egui::Button::new(RichText::new(tool.title()).size(11.0).color(text_color))
                            .fill(color)
                            .stroke(Stroke::new(1.0_f32, if selected { ORANGE } else { BORDER }))
                            .min_size(Vec2::new(100.0, 27.0))
                            .corner_radius(6.0);
                    if ui.add(button).clicked() {
                        self.edit.tool = tool;
                    }
                }
                ui.add_space(8.0);
                ui.label(RichText::new("Sayfa").size(12.0).strong().color(INK));
                if ghost_button(ui, "◀ Önceki").clicked() && self.edit.page > 1 {
                    self.edit.page -= 1;
                    self.edit.selected = None;
                    self.reload_edit_page(ctx);
                }
                if ghost_button(ui, "Sonraki ▶").clicked() && self.edit.page < self.edit.total {
                    self.edit.page += 1;
                    self.edit.selected = None;
                    self.reload_edit_page(ctx);
                }
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!("{} / {}", self.edit.page, self.edit.total))
                        .size(11.0)
                        .color(MUTED),
                );
            });
    }

    fn edit_canvas(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, height: f32) {
        let width = ui.available_width();
        let pw = self.edit.page_w.max(1.0);
        let ph = self.edit.page_h.max(1.0);
        let pad = 16.0;

        // Yakınlaştırma araç çubuğu.
        let now = ui.input(|i| i.time);
        let ppp = ctx.pixels_per_point();
        let estimate_h = (height - 30.0).max(120.0);
        let estimate_fit = ((width - 2.0 * pad) / pw)
            .min((estimate_h - 2.0 * pad) / ph)
            .max(0.03);
        let estimate_scale = if self.edit.fit {
            estimate_fit
        } else {
            self.edit.zoom * ZOOM_100
        };
        let pct = estimate_scale / ZOOM_100 * 100.0;
        let mut zoom_action: Option<f32> = None;
        let mut do_fit = false;
        let mut do_one_to_one = false;

        ui.horizontal(|ui| {
            ui.label(RichText::new("Yakınlaştırma").size(11.0).color(MUTED));
            if ui
                .small_button("−")
                .on_hover_text("Uzaklaştır (Ctrl + tekerlek)")
                .clicked()
            {
                zoom_action = Some(zoom_step_pct(pct, false));
            }
            let pct_label = ui.label(
                RichText::new(format!("{pct:.0}%"))
                    .size(11.5)
                    .strong()
                    .color(INK),
            );
            pct_label.on_hover_text("Ctrl + tekerlek ile yakınlaştırın.");
            if ui
                .small_button("+")
                .on_hover_text("Yakınlaştır (Ctrl + tekerlek)")
                .clicked()
            {
                zoom_action = Some(zoom_step_pct(pct, true));
            }
            if ui
                .small_button("Sığdır")
                .on_hover_text("Sayfayı görünüm alanına sığdır")
                .clicked()
            {
                do_fit = true;
            }
            if ui
                .small_button("1:1")
                .on_hover_text("%100 gerçek boyut")
                .clicked()
            {
                do_one_to_one = true;
            }
            if self.edit.fit {
                ui.label(RichText::new("(sığdırıldı)").size(10.0).color(MUTED));
            }
            if self.edit.tool == EditTool::TextSelect {
                ui.separator();
                let count = self.edit.text_objects.len();
                ui.label(
                    RichText::new(format!("{count} düzenlenebilir metin"))
                        .size(11.0)
                        .color(if count == 0 { MUTED } else { BLUE }),
                );
            }
        });

        if let Some(p) = zoom_action {
            self.set_zoom_pct(p, ctx);
        }
        if do_fit {
            self.edit.fit = true;
            self.edit.center_view = true;
            self.edit.zoom_changed_at = now;
            ctx.request_repaint();
        }
        if do_one_to_one {
            self.set_zoom_pct(100.0, ctx);
        }

        // Kalan alanı kaydırılabilir tuval olarak kullan.
        let view = ui.available_size();
        let fit_scale = ((view.x - 2.0 * pad) / pw)
            .min((view.y - 2.0 * pad) / ph)
            .max(0.03);
        self.edit.view_scale = if self.edit.fit {
            fit_scale
        } else {
            (self.edit.zoom * ZOOM_100).max(0.03)
        };
        let scale = self.edit.view_scale;
        let page_size = Vec2::new(pw * scale, ph * scale);

        // Yüksek yakınlaştırmada bulanıklaşmayı önlemek için önizlemeyi
        // ekrandaki gerçek piksel yoğunluğunda yeniden oluştur.
        let target_preview = (scale * ppp).min(preview_scale_limit(pw, ph)).max(1.0);
        let needs_sharper = target_preview > self.edit.preview_scale * 1.08;
        if needs_sharper && self.edit.texture.is_some() {
            if now - self.edit.zoom_changed_at >= 0.18 {
                self.edit.preview_scale = target_preview;
                self.edit.generation += 1;
                let generation = self.edit.generation;
                let page = self.edit.page;
                let index = (page.saturating_sub(1)) as usize;
                self.bus.spawn_preview(
                    ctx.clone(),
                    self.edit.bytes.clone(),
                    index,
                    page,
                    generation,
                    target_preview,
                );
            } else {
                let wait = (0.18 - (now - self.edit.zoom_changed_at)).max(0.02);
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait));
            }
        }

        let content = Vec2::new(
            (page_size.x + 2.0 * pad).max(view.x),
            (page_size.y + 2.0 * pad).max(view.y),
        );
        if self.edit.center_view {
            self.edit.scroll = ((content - view) * 0.5).max(Vec2::ZERO);
            self.edit.center_view = false;
        }
        let scroll = self.edit.scroll;

        let out = egui::ScrollArea::both()
            .id_salt("edit_canvas_scroll")
            .auto_shrink([false, false])
            .drag_to_scroll(false)
            .scroll_offset(scroll)
            .show(ui, |ui| {
                let (content_rect, resp) = ui.allocate_exact_size(content, Sense::click_and_drag());
                let painter = ui.painter_at(content_rect);
                painter.rect_filled(
                    content_rect,
                    CornerRadius::same(8),
                    Color32::from_rgb(214, 221, 233),
                );

                let page_rect = Rect::from_center_size(content_rect.center(), page_size);
                painter.rect_filled(
                    page_rect.expand(2.0),
                    CornerRadius::same(2),
                    Color32::from_rgba_unmultiplied(15, 23, 42, 28),
                );
                painter.rect_filled(page_rect, CornerRadius::ZERO, Color32::WHITE);
                let tex = self.edit.texture.as_ref().map(|t| t.id());
                if let Some(id) = tex {
                    painter.image(
                        id,
                        page_rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                } else {
                    painter.text(
                        page_rect.center(),
                        Align2::CENTER_CENTER,
                        "Sayfa yükleniyor...",
                        FontId::proportional(14.0),
                        MUTED,
                    );
                }

                let actions = self.edit.current().to_vec();
                for item in &actions {
                    if item.page != 0 && item.page != self.edit.page {
                        continue;
                    }
                    draw_action(&painter, page_rect, scale, ph, item);
                }

                // "Metni Düzenle" aracı seçiliyken düzenlenebilir metin
                // alanlarını çerçevele: kullanıcı hangi metinlere
                // tıklayabileceğini doğrudan sayfada görür.
                let show_text_boxes =
                    self.edit.show_text_boxes && self.edit.tool == EditTool::TextSelect;
                let to_screen = |x: f32, y: f32| {
                    Pos2::new(
                        page_rect.left() + x * scale,
                        page_rect.top() + (ph - y) * scale,
                    )
                };
                let hovered_text = if show_text_boxes {
                    ctx.pointer_latest_pos()
                        .filter(|p| page_rect.contains(*p))
                        .and_then(|pos| {
                            let x = (pos.x - page_rect.left()) / scale;
                            let y = ph - (pos.y - page_rect.top()) / scale;
                            crate::textobj::find_text_at(&self.edit.text_objects, x, y)
                        })
                } else {
                    None
                };
                if show_text_boxes {
                    for obj in &self.edit.text_objects {
                        let (l, b, r, t) = obj.bounds;
                        let rect =
                            Rect::from_min_max(to_screen(l, t), to_screen(r, b)).expand(1.5);
                        let is_selected = self
                            .edit
                            .selected
                            .as_ref()
                            .map(|s| s.idx == obj.idx)
                            .unwrap_or(false);
                        let is_hovered = hovered_text
                            .as_ref()
                            .map(|h| h.idx == obj.idx)
                            .unwrap_or(false);
                        let (fill, stroke, width): (Color32, Color32, f32) = if is_selected {
                            (
                                Color32::from_rgba_unmultiplied(22, 163, 74, 48),
                                GREEN,
                                2.0,
                            )
                        } else if is_hovered {
                            (
                                Color32::from_rgba_unmultiplied(234, 88, 12, 48),
                                ORANGE,
                                2.0,
                            )
                        } else {
                            (
                                Color32::from_rgba_unmultiplied(37, 99, 235, 26),
                                BLUE,
                                1.0,
                            )
                        };
                        painter.rect_filled(rect, CornerRadius::same(2), fill);
                        painter.rect_stroke(
                            rect,
                            CornerRadius::same(2),
                            Stroke::new(width, stroke),
                            StrokeKind::Inside,
                        );
                    }
                }

                let to_page = |pos: Pos2| {
                    (
                        ((pos.x - page_rect.left()) / scale).clamp(-50.0, pw + 50.0),
                        (ph - (pos.y - page_rect.top()) / scale).clamp(-50.0, ph + 50.0),
                    )
                };

                let is_shape = matches!(
                    self.edit.tool,
                    EditTool::Rectangle | EditTool::Circle | EditTool::Line
                );

                // Ctrl + tekerlek / pinch ile yakınlaştırma.
                if resp.hovered() {
                    let zoom_delta = ui.input(|i| i.zoom_delta());
                    if (zoom_delta - 1.0).abs() > 0.0001 {
                        let new_pct = (scale * zoom_delta / ZOOM_100 * 100.0)
                            .clamp(ZOOM_MIN_PCT, ZOOM_MAX_PCT);
                        self.edit.fit = false;
                        self.edit.zoom = new_pct / 100.0;
                        self.edit.zoom_changed_at = ui.input(|i| i.time);
                        self.edit.center_view = true;
                        ui.ctx().request_repaint();
                    }
                }

                if is_shape {
                    if resp.drag_started() {
                        if let Some(pos) = resp.interact_pointer_pos() {
                            if page_rect.contains(pos) {
                                self.edit.drag_start = Some(to_page(pos));
                                self.edit.drag_current = self.edit.drag_start;
                            }
                        }
                    }
                    if resp.dragged() {
                        if let Some(pos) = resp.interact_pointer_pos() {
                            if self.edit.drag_start.is_some() {
                                self.edit.drag_current = Some(to_page(pos));
                            }
                        }
                    }
                    if resp.drag_stopped() {
                        let end = ctx
                            .pointer_latest_pos()
                            .map(to_page)
                            .or(self.edit.drag_current);
                        if let (Some(start), Some(end)) = (self.edit.drag_start.take(), end) {
                            self.finish_shape(start, end);
                        }
                        self.edit.drag_current = None;
                    }
                    if let (Some(a), Some(b)) = (self.edit.drag_start, self.edit.drag_current) {
                        let r = Rect::from_two_pos(
                            Pos2::new(
                                page_rect.left() + a.0 * scale,
                                page_rect.top() + (ph - a.1) * scale,
                            ),
                            Pos2::new(
                                page_rect.left() + b.0 * scale,
                                page_rect.top() + (ph - b.1) * scale,
                            ),
                        );
                        painter.rect_stroke(
                            r,
                            CornerRadius::ZERO,
                            Stroke::new(1.5_f32, ORANGE),
                            StrokeKind::Inside,
                        );
                    }
                } else if resp.clicked() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        if page_rect.contains(pos) {
                            let (x, y) = to_page(pos);
                            self.canvas_click(ctx, x, y);
                        }
                    }
                }

                if resp.hovered()
                    && page_rect.contains(ctx.pointer_latest_pos().unwrap_or_default())
                {
                    let icon = if self.edit.tool == EditTool::TextSelect && hovered_text.is_some() {
                        egui::CursorIcon::Text
                    } else {
                        egui::CursorIcon::Crosshair
                    };
                    ui.ctx().set_cursor_icon(icon);
                }
            });
        self.edit.scroll = out.state.offset;
    }

    fn set_zoom_pct(&mut self, pct: f32, ctx: &egui::Context) {
        self.edit.fit = false;
        self.edit.zoom = (pct / 100.0).clamp(ZOOM_MIN, ZOOM_MAX);
        self.edit.zoom_changed_at = ctx.input(|i| i.time);
        self.edit.center_view = true;
        ctx.request_repaint();
    }

    fn finish_shape(&mut self, start: (f32, f32), end: (f32, f32)) {
        let lw = parse_f32(&self.edit.shape_lw, 2.0).clamp(0.5, 20.0);
        let color = self.edit.shape_color.clone();
        let fill = self.edit.shape_fill;
        let action = match self.edit.tool {
            EditTool::Rectangle => {
                let x = start.0.min(end.0);
                let y = start.1.min(end.1);
                let w = (start.0 - end.0).abs();
                let h = (start.1 - end.1).abs();
                if w < 2.0 || h < 2.0 {
                    return;
                }
                EditAction::Rectangle {
                    x,
                    y,
                    w,
                    h,
                    fill,
                    color,
                    lw,
                }
            }
            EditTool::Circle => {
                let r = ((start.0 - end.0).powi(2) + (start.1 - end.1).powi(2)).sqrt();
                if r < 2.0 {
                    return;
                }
                EditAction::Circle {
                    cx: start.0,
                    cy: start.1,
                    r,
                    fill,
                    color,
                    lw,
                }
            }
            EditTool::Line => {
                if (start.0 - end.0).abs() < 1.0 && (start.1 - end.1).abs() < 1.0 {
                    return;
                }
                EditAction::Line {
                    x1: start.0,
                    y1: start.1,
                    x2: end.0,
                    y2: end.1,
                    color,
                    width: lw,
                }
            }
            _ => return,
        };
        self.edit.push(EditItem {
            page: self.edit.page,
            action,
        });
    }

    fn canvas_click(&mut self, ctx: &egui::Context, x: f32, y: f32) {
        match self.edit.tool {
            EditTool::Text => {
                self.edit.pending_pos = Some((x, y));
                self.edit.text.text.clear();
                self.edit.show_text_dialog = true;
            }
            EditTool::Image => {
                if let Some(path) = pick_file(
                    "Eklenecek resmi seç",
                    "Görseller",
                    &["png", "jpg", "jpeg", "bmp", "tif", "tiff", "webp"],
                ) {
                    let (nw, nh) = crate::export::image_dims(&path).unwrap_or((300, 200));
                    let max_w = (self.edit.page_w * 0.45).max(80.0);
                    let w = (nw as f32).min(max_w).max(30.0);
                    let h = w * (nh as f32 / nw.max(1) as f32);
                    self.edit.push(EditItem {
                        page: self.edit.page,
                        action: EditAction::Image {
                            path,
                            x: x - w / 2.0,
                            y: y - h / 2.0,
                            w,
                            h,
                        },
                    });
                }
            }
            EditTool::TextSelect => {
                let hit = crate::textobj::find_text_at(&self.edit.text_objects, x, y);
                match hit {
                    Some(obj) => {
                        self.edit.edit_new = obj.text.clone();
                        self.edit.edit_size = format!("{:.1}", obj.size);
                        self.edit.edit_color =
                            if crate::model::parse_hex_color(&obj.color).is_some() {
                                obj.color.clone()
                            } else {
                                "black".to_string()
                            };
                        self.edit.edit_serif = obj.serif;
                        self.edit.edit_bold = obj.bold;
                        if let Some(rendered) = &self.edit.rendered {
                            self.edit.edit_bg = crate::textobj::sample_background(
                                &rendered.rgba,
                                rendered.width,
                                rendered.height,
                                self.edit.page_w,
                                self.edit.page_h,
                                obj.bounds,
                            );
                        } else {
                            self.edit.edit_bg = "#FFFFFF".to_string();
                        }
                        self.edit.selected = Some(obj);
                        self.edit.show_edit_dialog = true;
                    }
                    None => {
                        self.note(
                            Tab::Edit,
                            "Bu noktada seçilebilir metin bulunamadı. Metin katmanı olmayan (taranmış) PDF'lerde OCR kullanın.",
                        );
                    }
                }
            }
            _ => {}
        }
        ctx.request_repaint();
    }

    fn edit_props(&mut self, ui: &mut egui::Ui) {
        Frame::new()
            .fill(CARD)
            .stroke(Stroke::new(1.0_f32, BORDER))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(280.0);
                ui.label(RichText::new("Araç Ayarları").size(13.0).strong().color(INK));
                ui.add_space(4.0);
                match self.edit.tool {
                    EditTool::Text => {
                        ui.label(
                            RichText::new(
                                "Sayfada metnin başlayacağı yere tıklayın; ardından metin penceresi açılır.",
                            )
                            .size(11.0)
                            .color(MUTED),
                        );
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Boyut").size(11.0).color(MUTED));
                            ui.add(
                                egui::TextEdit::singleline(&mut self.edit.text.size)
                                    .desired_width(50.0),
                            );
                            ui.checkbox(&mut self.edit.text.serif, "Serif");
                            ui.checkbox(&mut self.edit.text.bold, "Kalın");
                        });
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Renk").size(11.0).color(MUTED));
                            color_picker(ui, "edit_text_color", &mut self.edit.text.color);
                        });
                    }
                    EditTool::Rectangle | EditTool::Circle | EditTool::Line => {
                        ui.label(
                            RichText::new("Sayfada sürükleyerek şekli çizin.")
                                .size(11.0)
                                .color(MUTED),
                        );
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Renk").size(11.0).color(MUTED));
                            color_picker(ui, "edit_shape_color", &mut self.edit.shape_color);
                        });
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Kalınlık").size(11.0).color(MUTED));
                            ui.add(
                                egui::TextEdit::singleline(&mut self.edit.shape_lw)
                                    .desired_width(45.0),
                            );
                            if self.edit.tool != EditTool::Line {
                                ui.checkbox(&mut self.edit.shape_fill, "Dolgulu");
                            }
                        });
                    }
                    EditTool::Image => {
                        ui.label(
                            RichText::new(
                                "Sayfada resmin ortalanacağı yere tıklayın ve bir görsel seçin.",
                            )
                            .size(11.0)
                            .color(MUTED),
                        );
                    }
                    EditTool::TextSelect => {
                        ui.label(
                            RichText::new(
                                "Değiştirmek istediğiniz metne tıklayın. Özgün metin yaklaşık olarak kapatılıp yenisi yazılır.",
                            )
                            .size(11.0)
                            .color(MUTED),
                        );
                        ui.add_space(3.0);
                        ui.checkbox(
                            &mut self.edit.show_text_boxes,
                            "Düzenlenebilir metinleri göster",
                        );
                        let count = self.edit.text_objects.len();
                        if count == 0 {
                            ui.label(
                                RichText::new(
                                    "Bu sayfada seçilebilir metin yok. Taranmış PDF'ler için OCR sekmesini kullanın.",
                                )
                                .size(10.5)
                                .color(ORANGE),
                            );
                        } else {
                            ui.label(
                                RichText::new(format!(
                                    "Bu sayfada {count} düzenlenebilir metin var; mavi çerçeveli alanlara tıklayın."
                                ))
                                .size(10.5)
                                .color(BLUE),
                            );
                        }
                        if let Some(obj) = &self.edit.selected {
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(format!("Seçili: \"{}\"", truncate(&obj.text, 40)))
                                    .size(11.0)
                                    .color(INK),
                            );
                            if ui.button("Seçili Metni Düzenle").clicked() {
                                self.edit.show_edit_dialog = true;
                            }
                        }
                    }
                }

                ui.add_space(8.0);
                ui.separator();
                ui.label(
                    RichText::new("Hızlı İşlemler")
                        .size(13.0)
                        .strong()
                        .color(INK),
                );
                ui.checkbox(&mut self.edit.rotate_all, "Tüm sayfalara uygula");
                ui.horizontal_wrapped(|ui| {
                    if small_button(ui, "90° ↻", ORANGE).clicked() {
                        self.add_rotate(90);
                    }
                    if small_button(ui, "90° ↺", ORANGE).clicked() {
                        self.add_rotate(270);
                    }
                    if small_button(ui, "180°", ORANGE).clicked() {
                        self.add_rotate(180);
                    }
                });
                ui.add_space(3.0);
                if small_button(ui, "Bu Sayfayı Sil", RED).clicked() {
                    let page = self.edit.page;
                    let already = self.edit.current().iter().any(|i| {
                        i.page == page && matches!(i.action, EditAction::DeletePage)
                    });
                    if !already {
                        self.edit.push(EditItem {
                            page,
                            action: EditAction::DeletePage,
                        });
                    }
                }
                if small_button(ui, "Silme İşlemini Geri Al", SLATE).clicked() {
                    let page = self.edit.page;
                    if let Some(pos) = self.edit.current().iter().rposition(|i| {
                        i.page == page && matches!(i.action, EditAction::DeletePage)
                    }) {
                        self.edit.actions.remove(pos);
                        self.edit.cursor = self.edit.actions.len();
                    }
                }
            });

        ui.add_space(8.0);
        Frame::new()
            .fill(CARD)
            .stroke(Stroke::new(1.0_f32, BORDER))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(280.0);
                ui.label(RichText::new("Filigran").size(13.0).strong().color(INK));
                ui.add_space(4.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.edit.wm.text)
                        .hint_text("Filigran metni")
                        .desired_width(f32::INFINITY),
                );
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Boyut").size(11.0).color(MUTED));
                    ui.add(egui::TextEdit::singleline(&mut self.edit.wm.size).desired_width(45.0));
                    ui.label(RichText::new("Saydamlık").size(11.0).color(MUTED));
                    ui.add(egui::TextEdit::singleline(&mut self.edit.wm.alpha).desired_width(45.0));
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Renk").size(11.0).color(MUTED));
                    color_picker(ui, "edit_wm_color", &mut self.edit.wm.color);
                });
                egui::ComboBox::from_id_salt("wm_mode")
                    .selected_text(self.edit.wm.mode.title())
                    .width(120.0)
                    .show_ui(ui, |ui| {
                        for mode in crate::model::WatermarkMode::all() {
                            ui.selectable_value(&mut self.edit.wm.mode, mode, mode.title());
                        }
                    });
                ui.checkbox(&mut self.edit.wm_all, "Tüm sayfalara ekle");
                if small_button(ui, "Filigran Ekle", PURPLE).clicked() {
                    self.add_watermark();
                }
            });
    }

    fn add_rotate(&mut self, angle: i32) {
        let page = if self.edit.rotate_all {
            0
        } else {
            self.edit.page
        };
        self.edit.push(EditItem {
            page,
            action: EditAction::Rotate { angle },
        });
    }

    fn add_watermark(&mut self) {
        if self.edit.wm.text.trim().is_empty() {
            self.error("Eksik Bilgi", "Filigran metni boş olamaz.");
            return;
        }
        let page = if self.edit.wm_all { 0 } else { self.edit.page };
        self.edit.push(EditItem {
            page,
            action: EditAction::Watermark {
                text: self.edit.wm.text.clone(),
                mode: self.edit.wm.mode,
                size: parse_f32(&self.edit.wm.size, 48.0).clamp(6.0, 300.0),
                alpha: parse_f32(&self.edit.wm.alpha, 0.25).clamp(0.02, 1.0),
                color: self.edit.wm.color.clone(),
            },
        });
    }

    fn start_edit_save(&mut self, ctx: &egui::Context) {
        if self.edit.bytes.is_empty() {
            self.error("Eksik Bilgi", "Önce bir PDF açın.");
            return;
        }
        if self.edit.current().is_empty() {
            self.error(
                "Değişiklik Yok",
                "Kaydetmeden önce en az bir düzenleme yapın.",
            );
            return;
        }
        let Some(fonts) = self.fonts.clone() else {
            self.error(
                "Yazı Tipi Hatası",
                self.fonts_error
                    .clone()
                    .unwrap_or_else(|| "Sistem yazı tipi bulunamadı.".to_string()),
            );
            return;
        };
        let Some(src_path) = self.edit.path.clone() else {
            return;
        };
        let default_name = format!("{}_duzenlenmis.pdf", util::stem(&src_path));
        let default_dir = dir_of(&src_path);
        let Some(out) = (|| {
            rfd::FileDialog::new()
                .set_title("Düzenlenmiş PDF'i kaydet")
                .set_directory(&default_dir)
                .set_file_name(&default_name)
                .add_filter("PDF", &["pdf"])
                .save_file()
        })() else {
            return;
        };
        let src = self.edit.bytes.clone();
        let edits = self.edit.current().to_vec();
        self.spawn_job(ctx, Tab::Edit, move |jc| {
            jc.progress(0.05, "Düzenlemeler uygulanıyor...");
            crate::pdfcore::apply_edits_and_save(&src, &out, &edits, &fonts, |p, t| {
                let ratio = if t == 0 { 1.0 } else { p as f32 / t as f32 };
                jc.progress(ratio, format!("Sayfa {p} / {t}"));
            })?;
            jc.progress(1.0, "Kaydedildi");
            jc.done(
                format!("Kaydedildi: {}", out.display()),
                format!(
                    "{} değişiklik PDF'e uygulandı.\n\nÇıktı dosyası:\n{}",
                    edits.len(),
                    out.display()
                ),
            );
            Ok(())
        });
    }

    pub fn edit_dialogs(&mut self, ctx: &egui::Context) {
        if self.edit.show_text_dialog {
            let mut add = false;
            let mut cancel = false;
            egui::Modal::new(egui::Id::new("edit_text_dialog"))
                .frame(
                    Frame::new()
                        .fill(CARD)
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(Margin::same(18)),
                )
                .show(ctx, |ui| {
                    ui.set_width(420.0);
                    ui.label(RichText::new("Metin Ekle").size(16.0).strong().color(NAVY));
                    ui.add_space(4.0);
                    ui.add(
                        egui::TextEdit::multiline(&mut self.edit.text.text)
                            .hint_text("Sayfaya eklenecek metin...")
                            .desired_width(f32::INFINITY)
                            .desired_rows(3),
                    );
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Boyut").size(11.0).color(MUTED));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.edit.text.size)
                                .desired_width(50.0),
                        );
                        ui.label(RichText::new("Renk").size(11.0).color(MUTED));
                        color_picker(ui, "text_dialog_color", &mut self.edit.text.color);
                        ui.checkbox(&mut self.edit.text.serif, "Serif");
                        ui.checkbox(&mut self.edit.text.bold, "Kalın");
                    });
                    ui.checkbox(
                        &mut self.edit.text.all_pages,
                        "Tüm sayfalara aynı konumda ekle",
                    );
                    if let Some((x, y)) = self.edit.pending_pos {
                        ui.label(
                            RichText::new(format!("Konum: {x:.0}, {y:.0} pt"))
                                .size(10.5)
                                .color(MUTED),
                        );
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if action_button(ui, "Ekle", GREEN).clicked() {
                            add = true;
                        }
                        if ghost_button(ui, "Vazgeç").clicked() {
                            cancel = true;
                        }
                    });
                });
            if add {
                let text = self.edit.text.text.trim_end().to_string();
                if text.trim().is_empty() {
                    self.error("Eksik Bilgi", "Metin boş olamaz.");
                } else if let Some((x, y)) = self.edit.pending_pos {
                    let page = if self.edit.text.all_pages {
                        0
                    } else {
                        self.edit.page
                    };
                    self.edit.push(EditItem {
                        page,
                        action: EditAction::Text {
                            x,
                            y,
                            text,
                            size: parse_f32(&self.edit.text.size, 18.0).clamp(4.0, 300.0),
                            color: self.edit.text.color.clone(),
                            serif: self.edit.text.serif,
                            bold: self.edit.text.bold,
                        },
                    });
                    self.edit.show_text_dialog = false;
                    self.edit.pending_pos = None;
                }
            }
            if cancel {
                self.edit.show_text_dialog = false;
                self.edit.pending_pos = None;
            }
        }

        if self.edit.show_edit_dialog {
            let Some(selected) = self.edit.selected.clone() else {
                self.edit.show_edit_dialog = false;
                return;
            };
            let mut apply = false;
            let mut cancel = false;
            egui::Modal::new(egui::Id::new("edit_existing_text_dialog"))
                .frame(
                    Frame::new()
                        .fill(CARD)
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(Margin::same(18)),
                )
                .show(ctx, |ui| {
                    ui.set_width(440.0);
                    ui.label(
                        RichText::new("Mevcut Metni Düzenle")
                            .size(16.0)
                            .strong()
                            .color(NAVY),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!("Özgün metin: \"{}\"", truncate(&selected.text, 80)))
                            .size(11.0)
                            .color(MUTED),
                    );
                    ui.add(
                        egui::TextEdit::multiline(&mut self.edit.edit_new)
                            .desired_width(f32::INFINITY)
                            .desired_rows(2),
                    );
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Boyut").size(11.0).color(MUTED));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.edit.edit_size)
                                .desired_width(50.0),
                        );
                        ui.label(RichText::new("Renk").size(11.0).color(MUTED));
                        color_picker(ui, "edit_old_color", &mut self.edit.edit_color);
                    });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Arka plan").size(11.0).color(MUTED));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.edit.edit_bg)
                                .desired_width(90.0),
                        );
                        ui.label(
                            RichText::new("(#FFFFFF gibi)")
                                .size(10.0)
                                .color(MUTED),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.edit.edit_serif, "Serif");
                        ui.checkbox(&mut self.edit.edit_bold, "Kalın");
                    });
                    ui.label(
                        RichText::new(
                            "Not: Özgün metin gerçek metin nesnesinden silinmez; üzeri arka plan rengiyle kapatılıp yeni metin yazılır.",
                        )
                        .size(10.0)
                        .color(MUTED),
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if action_button(ui, "Değiştir", GREEN).clicked() {
                            apply = true;
                        }
                        if ghost_button(ui, "Vazgeç").clicked() {
                            cancel = true;
                        }
                    });
                });
            if apply {
                self.edit.push(EditItem {
                    page: self.edit.page,
                    action: EditAction::TextEdit {
                        idx: selected.idx,
                        bounds: selected.bounds,
                        old: selected.text.clone(),
                        new: self.edit.edit_new.clone(),
                        size: parse_f32(&self.edit.edit_size, selected.size).clamp(4.0, 300.0),
                        color: self.edit.edit_color.clone(),
                        bg: self.edit.edit_bg.clone(),
                        origin: selected.origin,
                        serif: self.edit.edit_serif,
                        bold: self.edit.edit_bold,
                    },
                });
                self.edit.show_edit_dialog = false;
                self.edit.selected = None;
            }
            if cancel {
                self.edit.show_edit_dialog = false;
                self.edit.selected = None;
            }
        }
    }
}

fn truncate(s: &str, n: usize) -> String {
    let s = s.replace(['\n', '\r'], " ");
    if s.chars().count() <= n {
        s
    } else {
        let mut t: String = s.chars().take(n).collect();
        t.push('…');
        t
    }
}

fn draw_action(painter: &egui::Painter, page_rect: Rect, scale: f32, page_h: f32, item: &EditItem) {
    let to_screen = |x: f32, y: f32| {
        Pos2::new(
            page_rect.left() + x * scale,
            page_rect.top() + (page_h - y) * scale,
        )
    };
    let color_of = |name: &str, alpha: u8| {
        let c = crate::model::palette_color(name);
        Color32::from_rgba_unmultiplied(c[0], c[1], c[2], alpha)
    };
    match &item.action {
        EditAction::Text {
            x,
            y,
            text,
            size,
            color,
            serif,
            bold,
        } => {
            let family = if *bold {
                FontFamily::Name("bold".into())
            } else if *serif {
                FontFamily::Name("serif".into())
            } else {
                FontFamily::Proportional
            };
            let id = FontId::new((size * scale).max(4.0), family);
            painter.text(
                to_screen(*x, *y),
                Align2::LEFT_BOTTOM,
                text,
                id,
                color_of(color, 255),
            );
        }
        EditAction::Rectangle {
            x,
            y,
            w,
            h,
            fill,
            color,
            lw,
        } => {
            let rect = Rect::from_min_max(to_screen(*x, *y + *h), to_screen(*x + *w, *y));
            if *fill {
                painter.rect_filled(rect, CornerRadius::ZERO, color_of(color, 255));
            } else {
                painter.rect_stroke(
                    rect,
                    CornerRadius::ZERO,
                    Stroke::new((lw * scale).max(1.0), color_of(color, 255)),
                    StrokeKind::Inside,
                );
            }
        }
        EditAction::Circle {
            cx,
            cy,
            r,
            fill,
            color,
            lw,
        } => {
            let c = to_screen(*cx, *cy);
            if *fill {
                painter.circle_filled(c, r * scale, color_of(color, 255));
            } else {
                painter.circle_stroke(
                    c,
                    r * scale,
                    Stroke::new((lw * scale).max(1.0), color_of(color, 255)),
                );
            }
        }
        EditAction::Line {
            x1,
            y1,
            x2,
            y2,
            color,
            width,
        } => {
            painter.line_segment(
                [to_screen(*x1, *y1), to_screen(*x2, *y2)],
                Stroke::new((width * scale).max(1.0), color_of(color, 255)),
            );
        }
        EditAction::Image { x, y, w, h, .. } => {
            let rect = Rect::from_min_max(to_screen(*x, *y + *h), to_screen(*x + *w, *y));
            painter.rect_filled(
                rect,
                CornerRadius::ZERO,
                Color32::from_rgba_unmultiplied(80, 120, 200, 40),
            );
            painter.rect_stroke(
                rect,
                CornerRadius::ZERO,
                Stroke::new(1.0_f32, BLUE),
                StrokeKind::Inside,
            );
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Resim",
                FontId::proportional(10.0),
                BLUE,
            );
        }
        EditAction::Watermark {
            text,
            size,
            alpha,
            color,
            ..
        } => {
            let a = (alpha.clamp(0.02, 1.0) * 255.0) as u8;
            painter.text(
                page_rect.center(),
                Align2::CENTER_CENTER,
                text,
                FontId::new((size * scale).max(4.0), FontFamily::Proportional),
                color_of(color, a),
            );
        }
        EditAction::TextEdit {
            bounds,
            new,
            size,
            color,
            bg,
            origin,
            serif,
            ..
        } => {
            let (l, b, r, t) = *bounds;
            let rect = Rect::from_min_max(to_screen(l, t), to_screen(r, b));
            painter.rect_filled(rect, CornerRadius::ZERO, color_of(bg, 255));
            if !new.is_empty() {
                let family = if *serif {
                    FontFamily::Name("serif".into())
                } else {
                    FontFamily::Proportional
                };
                painter.text(
                    to_screen(origin.0, origin.1),
                    Align2::LEFT_BOTTOM,
                    new,
                    FontId::new((size * scale).max(4.0), family),
                    color_of(color, 255),
                );
            }
        }
        EditAction::Rotate { .. } => {}
        EditAction::DeletePage => {
            painter.rect_filled(
                page_rect,
                CornerRadius::ZERO,
                Color32::from_rgba_unmultiplied(220, 38, 38, 45),
            );
            painter.text(
                page_rect.center(),
                Align2::CENTER_CENTER,
                "Bu sayfa silinecek",
                FontId::proportional(16.0),
                RED,
            );
        }
    }
}

#[cfg(test)]
mod edit_zoom_tests {
    use super::*;

    #[test]
    fn zoom_steps_follow_presets() {
        assert_eq!(zoom_step_pct(28.0, true), 33.0);
        assert_eq!(zoom_step_pct(100.0, true), 125.0);
        assert_eq!(zoom_step_pct(100.0, false), 75.0);
        assert_eq!(zoom_step_pct(60.0, false), 50.0);
    }

    #[test]
    fn zoom_steps_clamp_to_limits() {
        assert_eq!(zoom_step_pct(500.0, true), ZOOM_MAX_PCT);
        assert_eq!(zoom_step_pct(10.0, false), ZOOM_MIN_PCT);
        assert_eq!(zoom_step_pct(0.0, true), ZOOM_MIN_PCT);
        assert_eq!(zoom_step_pct(900.0, false), ZOOM_MAX_PCT);
    }

    #[test]
    fn preview_scale_limit_stays_within_budget() {
        let limit = crate::app::preview_scale_limit(595.28, 841.89);
        assert!((0.5..=8.0).contains(&limit));
        assert!(841.89 * limit <= 7000.5);
        assert!(595.28 * 841.89 * limit * limit <= 28_000_000.0 * 1.01);
    }
}
