use eframe::egui::{self, Align, Color32, CornerRadius, Layout, RichText, Stroke, Vec2};
use std::path::PathBuf;

use crate::app::*;
use crate::convert::Engine;
use crate::model::{FileRow, Tab};

pub fn status_color(status: &str) -> Color32 {
    if status.starts_with("Tamam") || status.starts_with("✓") {
        GREEN
    } else if status.starts_with("Hata") || status.starts_with("✗") {
        RED
    } else if status.contains("...") || status.contains("İşleniyor") {
        ORANGE
    } else {
        MUTED
    }
}

pub fn draw_file_rows(ui: &mut egui::Ui, rows: &mut [FileRow]) -> Option<usize> {
    let mut remove = None;
    for (i, row) in rows.iter_mut().enumerate() {
        egui::Frame::new()
            .fill(Color32::from_rgb(248, 250, 252))
            .stroke(Stroke::new(1.0_f32, BORDER))
            .corner_radius(8.0)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::new(44.0, 24.0), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(rect, CornerRadius::same(5), NAVY_2);
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        if row.ext.is_empty() {
                            "DOSYA"
                        } else {
                            row.ext.as_str()
                        },
                        egui::FontId::proportional(9.5),
                        Color32::WHITE,
                    );
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&row.name).size(12.0).strong().color(INK));
                        ui.label(
                            RichText::new(&row.status)
                                .size(10.5)
                                .color(status_color(&row.status)),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if small_button(ui, "Sil", RED).clicked() {
                            remove = Some(i);
                        }
                    });
                });
            });
        ui.add_space(3.0);
    }
    remove
}

fn out_dir_row(ui: &mut egui::Ui, chosen: &mut Option<PathBuf>, source: Option<&std::path::Path>) {
    ui.horizontal(|ui| {
        if action_button(ui, "Klasör Seç", SLATE).clicked() {
            if let Some(dir) = pick_folder("Çıktı klasörü seç") {
                *chosen = Some(dir);
            }
        }
        let text = chosen
            .as_ref()
            .map(|p| p.display().to_string())
            .or_else(|| {
                source
                    .map(|p| dir_of(p).display().to_string())
                    .or_else(|| Some("Kaynak dosyaların klasörü".to_string()))
            })
            .unwrap_or_default();
        ui.label(RichText::new(text).size(11.5).color(MUTED));
        if chosen.is_some() && ui.small_button("Sıfırla").clicked() {
            *chosen = None;
        }
    });
}

fn engine_combo(ui: &mut egui::Ui, engine: &mut Engine) {
    egui::ComboBox::from_id_salt("engine_combo")
        .selected_text(engine.title())
        .width(180.0)
        .show_ui(ui, |ui| {
            for e in Engine::all() {
                ui.selectable_value(engine, e, e.title());
            }
        });
}

impl PdfToolboxApp {
    pub fn ui_convert(&mut self, ui: &mut egui::Ui) {
        self.page_title(ui, "Dosya → PDF", NAVY);
        ui.label(
            RichText::new(
                "DOC, DOCX, ODT, RTF, XLS, XLSX, XLSM, UDF ve görselleri PDF'e dönüştürün. Office dosyaları için Microsoft Office veya LibreOffice kullanılır.",
            )
            .size(12.0)
            .color(MUTED),
        );
        ui.add_space(10.0);

        card(ui, "Dosyalar", |ui| {
            ui.horizontal(|ui| {
                if action_button(ui, "Dosya Ekle", GREEN).clicked() {
                    let files = pick_files(
                        "Dönüştürülecek dosyaları seç",
                        "Desteklenen dosyalar",
                        &[
                            "pdf", "doc", "docx", "odt", "rtf", "xls", "xlsx", "xlsm", "udf",
                            "png", "jpg", "jpeg", "bmp", "tif", "tiff", "webp",
                        ],
                    );
                    for f in files {
                        self.convert.rows.push(FileRow::new(f));
                    }
                }
                if ghost_button(ui, "Klasörden Ekle").clicked() {
                    if let Some(dir) = pick_folder("Dosya klasörü seç") {
                        let files = crate::convert::collect_files_recursive(
                            &dir,
                            &[
                                "pdf", "doc", "docx", "odt", "rtf", "xls", "xlsx", "xlsm", "udf",
                                "png", "jpg", "jpeg", "bmp", "tif", "tiff", "webp",
                            ],
                        );
                        for f in files {
                            self.convert.rows.push(FileRow::new(f));
                        }
                    }
                }
                if ghost_button(ui, "Listeyi Temizle").clicked() && !self.is_running(Tab::Convert) {
                    self.convert.rows.clear();
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} dosya", self.convert.rows.len()))
                            .size(12.0)
                            .color(MUTED),
                    );
                });
            });
            ui.add_space(4.0);
            if self.convert.rows.is_empty() {
                ui.label(
                    RichText::new("Dosyaları seçin veya bu pencereye sürükleyin.")
                        .size(12.0)
                        .color(MUTED),
                );
            }
            if let Some(i) = draw_file_rows(ui, &mut self.convert.rows) {
                self.confirm = Some(ConfirmBox {
                    title: "Dosyayı Kaldır".to_string(),
                    text: "Bu dosya listeden kaldırılsın mı?".to_string(),
                    action: ConfirmAction::RemoveConvert(i),
                });
            }
        });

        card(ui, "Ayarlar", |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Dönüştürme motoru:").size(12.0).color(MUTED));
                engine_combo(ui, &mut self.convert.engine);
                if self.convert.engine == Engine::Word && !crate::convert::word_available() {
                    ui.label(
                        RichText::new("Microsoft Word bulunamadı; LibreOffice kullanılacak.")
                            .size(10.5)
                            .color(ORANGE),
                    );
                }
            });
            let has_sheets = self
                .convert
                .rows
                .iter()
                .any(|r| matches!(r.ext.as_str(), "XLS" | "XLSX" | "XLSM"));
            if has_sheets {
                let excel = crate::convert::excel_available();
                ui.label(
                    RichText::new(if excel {
                        "Excel dosyaları Microsoft Excel ile, olmazsa LibreOffice (Calc) ile PDF'e aktarılır."
                    } else {
                        "Excel dosyaları LibreOffice (Calc) ile PDF'e aktarılır; Microsoft Excel bulunamadı."
                    })
                    .size(10.5)
                    .color(if excel { MUTED } else { ORANGE }),
                );
            }
            ui.add_space(4.0);
            out_dir_row(
                ui,
                &mut self.convert.out_dir,
                self.convert.rows.first().map(|r| r.path.as_path()),
            );
            ui.add_space(6.0);
            let running = self.is_running(Tab::Convert);
            ui.add_enabled_ui(!running && !self.convert.rows.is_empty(), |ui| {
                if action_button(ui, "PDF'e Dönüştür", GREEN).clicked() {
                    self.start_convert(ui.ctx());
                }
            });
            ui.add_space(4.0);
            self.progress_ui(ui, Tab::Convert);
            self.status_label(ui, Tab::Convert);
        });
    }

    fn start_convert(&mut self, ctx: &egui::Context) {
        if self.convert.rows.is_empty() {
            self.error("Eksik Bilgi", "Dönüştürülecek dosya ekleyin.");
            return;
        }
        let engine = self.convert.engine;
        let soffice = self.docx.soffice.clone();
        let rows: Vec<PathBuf> = self.convert.rows.iter().map(|r| r.path.clone()).collect();
        let out_dir = output_dir(&self.convert.out_dir, rows.first().map(|p| p.as_path()));

        self.spawn_job(ctx, Tab::Convert, move |jc| {
            std::fs::create_dir_all(&out_dir)?;
            let total = rows.len();
            let mut ok = 0usize;
            let mut errors = Vec::new();
            for (i, src) in rows.iter().enumerate() {
                let name = crate::util::file_name(src);
                jc.row(i, "İşleniyor...");
                jc.progress(
                    i as f32 / total.max(1) as f32,
                    format!("Dönüştürülüyor: {name}"),
                );
                let out = crate::util::unique_output(&out_dir, &crate::util::stem(src), "pdf");
                match crate::convert::convert_any_to_pdf(src, &out, engine, soffice.as_deref()) {
                    Ok(()) => {
                        ok += 1;
                        jc.row(i, "Tamamlandı ✓");
                    }
                    Err(e) => {
                        let msg = format!("{e:#}");
                        jc.row(i, format!("Hata: {}", msg.lines().next().unwrap_or("")));
                        errors.push(format!("{name}: {msg}"));
                    }
                }
            }
            jc.progress(1.0, "Tamamlandı");
            let mut message = format!(
                "{ok} / {total} dosya PDF'e dönüştürüldü.\n\nKlasör:\n{}",
                out_dir.display()
            );
            if !errors.is_empty() {
                message.push_str("\n\nHatalar:\n");
                for e in errors.iter().take(10) {
                    message.push_str(&format!("• {e}\n"));
                }
            }
            jc.done(format!("{ok} dosya dönüştürüldü."), message);
            Ok(())
        });
    }

    pub fn ui_docx(&mut self, ui: &mut egui::Ui) {
        self.page_title(ui, "DOCX → PDF", NAVY);
        ui.label(
            RichText::new(
                "Word belgelerini yüksek kalitede PDF'e dönüştürün. Microsoft Word kuruluysa doğrudan Word, aksi halde LibreOffice kullanılır.",
            )
            .size(12.0)
            .color(MUTED),
        );
        ui.add_space(10.0);

        card(ui, "Word Belgeleri", |ui| {
            ui.horizontal(|ui| {
                if action_button(ui, "DOCX Ekle", BLUE).clicked() {
                    let files = pick_files(
                        "Word belgelerini seç",
                        "Word / Writer belgeleri",
                        &["doc", "docx", "odt", "rtf"],
                    );
                    for f in files {
                        self.docx.rows.push(FileRow::new(f));
                    }
                }
                if ghost_button(ui, "Klasörden Ekle").clicked() {
                    if let Some(dir) = pick_folder("Belge klasörü seç") {
                        let files = crate::convert::collect_files_recursive(
                            &dir,
                            &["doc", "docx", "odt", "rtf"],
                        );
                        for f in files {
                            self.docx.rows.push(FileRow::new(f));
                        }
                    }
                }
                if ghost_button(ui, "Listeyi Temizle").clicked() && !self.is_running(Tab::Docx) {
                    self.docx.rows.clear();
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} belge", self.docx.rows.len()))
                            .size(12.0)
                            .color(MUTED),
                    );
                });
            });
            ui.add_space(4.0);
            if self.docx.rows.is_empty() {
                ui.label(
                    RichText::new("DOCX dosyalarını seçin veya bu pencereye sürükleyin.")
                        .size(12.0)
                        .color(MUTED),
                );
            }
            if let Some(i) = draw_file_rows(ui, &mut self.docx.rows) {
                self.confirm = Some(ConfirmBox {
                    title: "Belgeyi Kaldır".to_string(),
                    text: "Bu belge listeden kaldırılsın mı?".to_string(),
                    action: ConfirmAction::RemoveDocx(i),
                });
            }
        });

        card(ui, "Dönüştürme Motoru", |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Motor:").size(12.0).color(MUTED));
                engine_combo(ui, &mut self.docx.engine);
                let word = crate::convert::word_available();
                ui.label(
                    RichText::new(if word {
                        "Microsoft Word kullanılabilir"
                    } else {
                        "Microsoft Word bulunamadı"
                    })
                    .size(10.5)
                    .color(if word { GREEN } else { MUTED }),
                );
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if action_button(ui, "soffice.exe Seç", SLATE).clicked() {
                    if let Some(path) =
                        pick_file("soffice.exe dosyasını seç", "LibreOffice", &["exe"])
                    {
                        self.docx.soffice = Some(path);
                    }
                }
                let text = self
                    .docx
                    .soffice
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| {
                        "LibreOffice otomatik aranacak (bulunamadıysa seçin)".to_string()
                    });
                ui.label(RichText::new(text).size(11.0).color(MUTED));
            });
            ui.add_space(4.0);
            out_dir_row(
                ui,
                &mut self.docx.out_dir,
                self.docx.rows.first().map(|r| r.path.as_path()),
            );
            ui.add_space(6.0);
            let running = self.is_running(Tab::Docx);
            ui.add_enabled_ui(!running && !self.docx.rows.is_empty(), |ui| {
                if action_button(ui, "PDF'e Çevir", BLUE).clicked() {
                    self.start_docx(ui.ctx());
                }
            });
            ui.add_space(4.0);
            self.progress_ui(ui, Tab::Docx);
            self.status_label(ui, Tab::Docx);
        });
    }

    fn start_docx(&mut self, ctx: &egui::Context) {
        if self.docx.rows.is_empty() {
            self.error("Eksik Bilgi", "Dönüştürülecek Word belgesi ekleyin.");
            return;
        }
        let engine = self.docx.engine;
        let soffice = self.docx.soffice.clone();
        let rows: Vec<PathBuf> = self.docx.rows.iter().map(|r| r.path.clone()).collect();
        let out_dir = output_dir(&self.docx.out_dir, rows.first().map(|p| p.as_path()));

        self.spawn_job(ctx, Tab::Docx, move |jc| {
            std::fs::create_dir_all(&out_dir)?;
            let total = rows.len();
            let mut ok = 0usize;
            let mut errors = Vec::new();
            for (i, src) in rows.iter().enumerate() {
                let name = crate::util::file_name(src);
                jc.row(i, "İşleniyor...");
                jc.progress(
                    i as f32 / total.max(1) as f32,
                    format!("Word → PDF: {name}"),
                );
                let out = crate::util::unique_output(&out_dir, &crate::util::stem(src), "pdf");
                match crate::convert::convert_any_to_pdf(src, &out, engine, soffice.as_deref()) {
                    Ok(()) => {
                        ok += 1;
                        jc.row(i, "Tamamlandı ✓");
                    }
                    Err(e) => {
                        let msg = format!("{e:#}");
                        jc.row(i, format!("Hata: {}", msg.lines().next().unwrap_or("")));
                        errors.push(format!("{name}: {msg}"));
                    }
                }
            }
            jc.progress(1.0, "Tamamlandı");
            let mut message = format!(
                "{ok} / {total} belge PDF'e dönüştürüldü.\n\nKlasör:\n{}",
                out_dir.display()
            );
            if !errors.is_empty() {
                message.push_str("\n\nHatalar:\n");
                for e in errors.iter().take(10) {
                    message.push_str(&format!("• {e}\n"));
                }
            }
            jc.done(format!("{ok} belge dönüştürüldü."), message);
            Ok(())
        });
    }

    pub fn ui_udf(&mut self, ui: &mut egui::Ui) {
        self.page_title(ui, "UDF → PDF", NAVY);
        ui.label(
            RichText::new(
                "UYAP Editör (.udf) belgelerini biçimlendirmeleriyle birlikte PDF'e dönüştürün.",
            )
            .size(12.0)
            .color(MUTED),
        );
        ui.add_space(10.0);

        card(ui, "UDF Belgeleri", |ui| {
            ui.horizontal(|ui| {
                if action_button(ui, "UDF Ekle", PURPLE).clicked() {
                    let files = pick_files("UDF belgelerini seç", "UDF", &["udf"]);
                    for f in files {
                        self.udf.rows.push(FileRow::new(f));
                    }
                }
                if ghost_button(ui, "Klasörden Ekle").clicked() {
                    if let Some(dir) = pick_folder("UDF klasörü seç") {
                        let files = crate::convert::collect_files_recursive(&dir, &["udf"]);
                        for f in files {
                            self.udf.rows.push(FileRow::new(f));
                        }
                    }
                }
                if ghost_button(ui, "Listeyi Temizle").clicked() && !self.is_running(Tab::Udf) {
                    self.udf.rows.clear();
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} belge", self.udf.rows.len()))
                            .size(12.0)
                            .color(MUTED),
                    );
                });
            });
            ui.add_space(4.0);
            if self.udf.rows.is_empty() {
                ui.label(
                    RichText::new("UDF dosyalarını seçin veya bu pencereye sürükleyin.")
                        .size(12.0)
                        .color(MUTED),
                );
            }
            if let Some(i) = draw_file_rows(ui, &mut self.udf.rows) {
                self.confirm = Some(ConfirmBox {
                    title: "Belgeyi Kaldır".to_string(),
                    text: "Bu belge listeden kaldırılsın mı?".to_string(),
                    action: ConfirmAction::RemoveUdf(i),
                });
            }
        });

        card(ui, "Çıktı ve İşlem", |ui| {
            out_dir_row(
                ui,
                &mut self.udf.out_dir,
                self.udf.rows.first().map(|r| r.path.as_path()),
            );
            ui.add_space(6.0);
            let running = self.is_running(Tab::Udf);
            ui.add_enabled_ui(!running && !self.udf.rows.is_empty(), |ui| {
                if action_button(ui, "PDF'e Çevir", PURPLE).clicked() {
                    self.start_udf(ui.ctx());
                }
            });
            ui.add_space(4.0);
            self.progress_ui(ui, Tab::Udf);
            self.status_label(ui, Tab::Udf);
        });
    }

    fn start_udf(&mut self, ctx: &egui::Context) {
        if self.udf.rows.is_empty() {
            self.error("Eksik Bilgi", "Dönüştürülecek UDF belgesi ekleyin.");
            return;
        }
        let rows: Vec<PathBuf> = self.udf.rows.iter().map(|r| r.path.clone()).collect();
        let out_dir = output_dir(&self.udf.out_dir, rows.first().map(|p| p.as_path()));

        self.spawn_job(ctx, Tab::Udf, move |jc| {
            std::fs::create_dir_all(&out_dir)?;
            let total = rows.len();
            let mut ok = 0usize;
            let mut errors = Vec::new();
            for (i, src) in rows.iter().enumerate() {
                let name = crate::util::file_name(src);
                jc.row(i, "İşleniyor...");
                jc.progress(i as f32 / total.max(1) as f32, format!("UDF → PDF: {name}"));
                let out = crate::util::unique_output(&out_dir, &crate::util::stem(src), "pdf");
                match crate::udf::udf_to_pdf(src, &out) {
                    Ok(()) => {
                        ok += 1;
                        jc.row(i, "Tamamlandı ✓");
                    }
                    Err(e) => {
                        let msg = format!("{e:#}");
                        jc.row(i, format!("Hata: {}", msg.lines().next().unwrap_or("")));
                        errors.push(format!("{name}: {msg}"));
                    }
                }
            }
            jc.progress(1.0, "Tamamlandı");
            let mut message = format!(
                "{ok} / {total} UDF belgesi PDF'e dönüştürüldü.\n\nKlasör:\n{}",
                out_dir.display()
            );
            if !errors.is_empty() {
                message.push_str("\n\nHatalar:\n");
                for e in errors.iter().take(10) {
                    message.push_str(&format!("• {e}\n"));
                }
            }
            jc.done(format!("{ok} belge dönüştürüldü."), message);
            Ok(())
        });
    }
}
