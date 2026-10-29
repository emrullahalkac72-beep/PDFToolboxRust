use eframe::egui::{self, Align, Color32, Layout, RichText};

use crate::app::*;
use crate::export::ExportFormat;
use crate::model::{OcrMode, Tab};

impl PdfToolboxApp {
    pub fn ui_pageno(&mut self, ui: &mut egui::Ui) {
        self.page_title(ui, "Sayfa Numarası", NAVY);
        ui.label(
            RichText::new(
                "PDF sayfalarına otomatik numara ekleyin. Konum, başlangıç numarası ve biçim ayarlanabilir.",
            )
            .size(12.0)
            .color(MUTED),
        );
        ui.add_space(10.0);

        card(ui, "Kaynak PDF", |ui| {
            ui.horizontal(|ui| {
                if action_button(ui, "PDF Aç", Color32::from_rgb(2, 132, 199)).clicked() {
                    if let Some(path) = pick_file("Numaralandırılacak PDF'yi seç", "PDF", &["pdf"])
                    {
                        self.open_pageno(path);
                    }
                }
                if let Some(path) = &self.pageno.path {
                    ui.label(
                        RichText::new(crate::util::file_name(path))
                            .size(12.5)
                            .strong()
                            .color(INK),
                    );
                    ui.label(
                        RichText::new(format!("{} sayfa", self.pageno.total))
                            .size(11.5)
                            .color(MUTED),
                    );
                } else {
                    ui.label(RichText::new("PDF seçilmedi").size(12.0).color(MUTED));
                }
            });
        });

        card(ui, "Numaralandırma Ayarları", |ui| {
            egui::Grid::new("pageno_grid")
                .num_columns(2)
                .spacing([18.0, 8.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Başlangıç numarası").size(12.0).color(MUTED));
                    ui.add(egui::TextEdit::singleline(&mut self.pageno.start).desired_width(70.0));
                    ui.end_row();

                    ui.label(RichText::new("Yazı boyutu").size(12.0).color(MUTED));
                    ui.add(egui::TextEdit::singleline(&mut self.pageno.size).desired_width(70.0));
                    ui.end_row();

                    ui.label(RichText::new("Dikey konum").size(12.0).color(MUTED));
                    egui::ComboBox::from_id_salt("pageno_vertical")
                        .selected_text(&self.pageno.vertical)
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.pageno.vertical,
                                "Alt".to_string(),
                                "Alt",
                            );
                            ui.selectable_value(
                                &mut self.pageno.vertical,
                                "Üst".to_string(),
                                "Üst",
                            );
                        });
                    ui.end_row();

                    ui.label(RichText::new("Yatay konum").size(12.0).color(MUTED));
                    egui::ComboBox::from_id_salt("pageno_horizontal")
                        .selected_text(&self.pageno.horizontal)
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            for value in ["Sol", "Orta", "Sağ"] {
                                ui.selectable_value(
                                    &mut self.pageno.horizontal,
                                    value.to_string(),
                                    value,
                                );
                            }
                        });
                    ui.end_row();

                    ui.label(RichText::new("Biçim").size(12.0).color(MUTED));
                    ui.vertical(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.pageno.format)
                                .desired_width(240.0),
                        );
                        ui.label(
                            RichText::new("{n} = sayfa numarası, {t} = toplam sayfa")
                                .size(10.5)
                                .color(MUTED),
                        );
                    });
                    ui.end_row();
                });
            ui.checkbox(
                &mut self.pageno.skip_first,
                "İlk sayfaya numara ekleme (kapak sayfası)",
            );
            if self.pageno.total > 0 {
                let sample = crate::overlay::page_number_text(
                    &self.pageno.format,
                    if self.pageno.skip_first { 2 } else { 1 },
                    parse_u32(&self.pageno.start, 1),
                    self.pageno.total,
                );
                ui.label(
                    RichText::new(format!("Örnek: {sample}"))
                        .size(11.0)
                        .color(BLUE),
                );
            }
        });

        card(ui, "Kaydet ve Uygula", |ui| {
            ui.horizontal(|ui| {
                if action_button(ui, "Klasör Seç", SLATE).clicked() {
                    if let Some(dir) = pick_folder("Çıktı klasörü seç") {
                        self.pageno.out_dir = Some(dir);
                    }
                }
                let text = self
                    .pageno
                    .out_dir
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .or_else(|| {
                        self.pageno
                            .path
                            .as_ref()
                            .map(|p| dir_of(p).display().to_string())
                    })
                    .unwrap_or_else(|| "PDF seçilmedi".to_string());
                ui.label(RichText::new(text).size(11.5).color(MUTED));
            });
            ui.add_space(6.0);
            let running = self.is_running(Tab::PageNo);
            let ready = self.pageno.path.is_some() && self.pageno.total > 0;
            ui.add_enabled_ui(!running && ready, |ui| {
                if action_button(ui, "Numaralandır ve Kaydet", Color32::from_rgb(2, 132, 199))
                    .clicked()
                {
                    self.start_pageno(ui.ctx());
                }
            });
            ui.add_space(4.0);
            self.progress_ui(ui, Tab::PageNo);
            self.status_label(ui, Tab::PageNo);
        });
    }

    fn start_pageno(&mut self, ctx: &egui::Context) {
        let Some(src_path) = self.pageno.path.clone() else {
            self.error("Eksik Bilgi", "Önce bir PDF açın.");
            return;
        };
        let Some(fonts) = self.fonts.clone() else {
            self.error(
                "Yazı Tipi Hatası",
                self.fonts_error.clone().unwrap_or_default(),
            );
            return;
        };
        let default_name = format!("{}_numarali.pdf", crate::util::stem(&src_path));
        let default_dir = self
            .pageno
            .out_dir
            .clone()
            .unwrap_or_else(|| dir_of(&src_path));
        let Some(out) = rfd::FileDialog::new()
            .set_title("Numaralandırılmış PDF'i kaydet")
            .set_directory(&default_dir)
            .set_file_name(&default_name)
            .add_filter("PDF", &["pdf"])
            .save_file()
        else {
            return;
        };
        let bytes = self.pageno.bytes.clone();
        let start = parse_u32(&self.pageno.start, 1);
        let size = parse_f32(&self.pageno.size, 12.0).clamp(4.0, 100.0);
        let position = format!("{} {}", self.pageno.vertical, self.pageno.horizontal);
        let format = self.pageno.format.clone();
        let skip_first = self.pageno.skip_first;

        if let Some(parent) = out.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        self.spawn_job(ctx, Tab::PageNo, move |jc| {
            jc.progress(0.02, "Sayfa numaraları ekleniyor...");
            let written = crate::pdfcore::add_page_numbers(
                &bytes,
                &out,
                start,
                size,
                &position,
                &format,
                skip_first,
                &fonts,
                |p, t| {
                    if t > 0 {
                        jc.progress(p as f32 / t as f32, format!("Sayfa {p} / {t}"));
                    }
                },
            )?;
            jc.progress(1.0, "Kaydedildi");
            jc.done(
                format!("{written} sayfa numaralandırıldı."),
                format!(
                    "{written} sayfaya numara eklendi.\n\nÇıktı dosyası:\n{}",
                    out.display()
                ),
            );
            Ok(())
        });
    }

    pub fn ui_ocr(&mut self, ui: &mut egui::Ui) {
        self.page_title(ui, "Taranmış PDF / OCR", NAVY);
        ui.label(
            RichText::new(
                "Taranmış PDF'leri aranabilir PDF'e veya düzenlenebilir metin PDF'ine dönüştürün. Tesseract OCR kullanılır.",
            )
            .size(12.0)
            .color(MUTED),
        );
        ui.add_space(10.0);

        card(ui, "Kaynak PDF", |ui| {
            ui.horizontal(|ui| {
                if action_button(ui, "PDF Aç", PINK).clicked() {
                    if let Some(path) = pick_file("Taranmış PDF'yi seç", "PDF", &["pdf"]) {
                        self.ocr.path = Some(path.clone());
                        self.note(Tab::Ocr, crate::util::file_name(&path));
                    }
                }
                if let Some(path) = &self.ocr.path {
                    ui.label(
                        RichText::new(crate::util::file_name(path))
                            .size(12.5)
                            .strong()
                            .color(INK),
                    );
                } else {
                    ui.label(RichText::new("PDF seçilmedi").size(12.0).color(MUTED));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button("Tesseract Seç").clicked() {
                        if let Some(p) =
                            pick_file("tesseract.exe dosyasını seç", "Tesseract", &["exe"])
                        {
                            self.ocr.tess = Some(p);
                        }
                    }
                    let text = self
                        .ocr
                        .tess
                        .as_ref()
                        .map(|p| crate::util::file_name(p))
                        .unwrap_or_else(|| "Tesseract bulunamadı".to_string());
                    ui.label(
                        RichText::new(text)
                            .size(10.5)
                            .color(if self.ocr.tess.is_some() { GREEN } else { RED }),
                    );
                });
            });
        });

        card(ui, "OCR Ayarları", |ui| {
            ui.horizontal(|ui| {
                ui.radio_value(
                    &mut self.ocr.mode,
                    OcrMode::Searchable,
                    "Aranabilir PDF (görüntü + gizli metin)",
                );
                ui.radio_value(
                    &mut self.ocr.mode,
                    OcrMode::Text,
                    "Metin PDF (yalnızca tanınan metin)",
                );
            });
            ui.add_space(4.0);
            egui::Grid::new("ocr_grid")
                .num_columns(2)
                .spacing([18.0, 8.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Dil").size(12.0).color(MUTED));
                    egui::ComboBox::from_id_salt("ocr_lang")
                        .selected_text(lang_title(&self.ocr.lang))
                        .width(220.0)
                        .show_ui(ui, |ui| {
                            for (code, name) in [
                                ("tur", "Türkçe"),
                                ("tur+eng", "Türkçe + İngilizce"),
                                ("eng", "İngilizce"),
                                ("deu", "Almanca"),
                                ("fra", "Fransızca"),
                                ("ara", "Arapça"),
                                ("rus", "Rusça"),
                            ] {
                                ui.selectable_value(&mut self.ocr.lang, code.to_string(), name);
                            }
                        });
                    ui.end_row();

                    ui.label(RichText::new("DPI").size(12.0).color(MUTED));
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.ocr.dpi).desired_width(70.0));
                        ui.label(
                            RichText::new("300 önerilir (200–600)")
                                .size(10.5)
                                .color(MUTED),
                        );
                    });
                    ui.end_row();
                });
            ui.label(
                RichText::new(
                    "Not: Türkçe için Tesseract kurulumunda 'Turkish' dil paketi seçilmiş olmalıdır.",
                )
                .size(10.5)
                .color(MUTED),
            );
        });

        card(ui, "Kaydet ve Başlat", |ui| {
            ui.horizontal(|ui| {
                if action_button(ui, "Klasör Seç", SLATE).clicked() {
                    if let Some(dir) = pick_folder("Çıktı klasörü seç") {
                        self.ocr.out_dir = Some(dir);
                    }
                }
                let text = self
                    .ocr
                    .out_dir
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .or_else(|| {
                        self.ocr
                            .path
                            .as_ref()
                            .map(|p| dir_of(p).display().to_string())
                    })
                    .unwrap_or_else(|| "PDF seçilmedi".to_string());
                ui.label(RichText::new(text).size(11.5).color(MUTED));
            });
            ui.add_space(6.0);
            let running = self.is_running(Tab::Ocr);
            ui.add_enabled_ui(!running && self.ocr.path.is_some(), |ui| {
                if action_button(ui, "OCR Başlat", PINK).clicked() {
                    self.start_ocr(ui.ctx());
                }
            });
            ui.add_space(4.0);
            self.progress_ui(ui, Tab::Ocr);
            self.status_label(ui, Tab::Ocr);
        });
    }

    fn start_ocr(&mut self, ctx: &egui::Context) {
        let Some(src) = self.ocr.path.clone() else {
            self.error("Eksik Bilgi", "Önce taranmış PDF'yi seçin.");
            return;
        };
        let mode = self.ocr.mode;
        let default_name = format!("{}_ocr.pdf", crate::util::stem(&src));
        let default_dir = self.ocr.out_dir.clone().unwrap_or_else(|| dir_of(&src));
        let Some(out) = rfd::FileDialog::new()
            .set_title("OCR sonucunu kaydet")
            .set_directory(&default_dir)
            .set_file_name(&default_name)
            .add_filter("PDF", &["pdf"])
            .save_file()
        else {
            return;
        };
        let lang = self.ocr.lang.clone();
        let dpi = parse_u32(&self.ocr.dpi, 300).clamp(70, 1200);
        let tess = self.ocr.tess.clone();

        self.spawn_job(ctx, Tab::Ocr, move |jc| {
            jc.progress(0.01, "OCR hazırlanıyor...");
            let pages =
                crate::ocr::ocr_pdf(&src, &out, mode, &lang, dpi, tess.as_deref(), |p, t| {
                    if t > 0 {
                        jc.progress(p as f32 / t as f32, format!("OCR: sayfa {p} / {t}"));
                    }
                })?;
            jc.progress(1.0, "Tamamlandı");
            jc.done(
                format!("{pages} sayfa işlendi."),
                format!(
                    "{pages} sayfa OCR ile işlendi.\n\nÇıktı dosyası:\n{}",
                    out.display()
                ),
            );
            Ok(())
        });
    }

    pub fn ui_export(&mut self, ui: &mut egui::Ui) {
        self.page_title(ui, "PDF → Diğer Formatlar", NAVY);
        ui.label(
            RichText::new(
                "PDF sayfalarını PNG, JPG, DOCX, DOC, XLSX, TXT, CSV veya PPTX olarak dışa aktarın.",
            )
            .size(12.0)
            .color(MUTED),
        );
        ui.add_space(10.0);

        card(ui, "Kaynak PDF", |ui| {
            ui.horizontal(|ui| {
                if action_button(ui, "PDF Aç", SLATE).clicked() {
                    if let Some(path) = pick_file("Dışa aktarılacak PDF'yi seç", "PDF", &["pdf"])
                    {
                        self.open_export(path);
                    }
                }
                if let Some(path) = &self.export.path {
                    ui.label(
                        RichText::new(crate::util::file_name(path))
                            .size(12.5)
                            .strong()
                            .color(INK),
                    );
                    ui.label(
                        RichText::new(format!("{} sayfa", self.export.total))
                            .size(11.5)
                            .color(MUTED),
                    );
                } else {
                    ui.label(RichText::new("PDF seçilmedi").size(12.0).color(MUTED));
                }
            });
        });

        card(ui, "Sayfalar", |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Sayfa aralığı:").size(12.0).color(MUTED));
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.export.ranges)
                        .hint_text("1-5, 8, 10-12")
                        .desired_width(230.0),
                );
                if resp.lost_focus() {
                    if let Ok(pages) =
                        crate::util::parse_ranges(&self.export.ranges, self.export.total)
                    {
                        self.export.selected = pages;
                    }
                }
                if ui.button("Sayfa Seç").clicked() && self.export.total > 0 {
                    self.page_dialog = Some(PageDialog {
                        target: PageTarget::Export,
                        total: self.export.total,
                        selected: (1..=self.export.total)
                            .map(|p| self.export.selected.contains(&p))
                            .collect(),
                        ranges: crate::util::format_ranges(&self.export.selected),
                        error: None,
                    });
                }
                if self.export.total > 0 && ui.small_button("Tümü").clicked() {
                    self.export.selected = (1..=self.export.total).collect();
                    self.export.ranges = format!("1-{}", self.export.total);
                }
                ui.label(
                    RichText::new(format!("{} sayfa seçili", self.export.selected.len()))
                        .size(11.0)
                        .color(MUTED),
                );
            });
        });

        card(ui, "Format ve Çıktı", |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Format:").size(12.0).color(MUTED));
                egui::ComboBox::from_id_salt("export_format")
                    .selected_text(self.export.format.title())
                    .width(120.0)
                    .show_ui(ui, |ui| {
                        for fmt in ExportFormat::all() {
                            ui.selectable_value(&mut self.export.format, fmt, fmt.title());
                        }
                    });
                if matches!(self.export.format, ExportFormat::Png | ExportFormat::Jpg) {
                    ui.label(RichText::new("DPI:").size(12.0).color(MUTED));
                    ui.add(egui::TextEdit::singleline(&mut self.export.dpi).desired_width(55.0));
                    ui.checkbox(&mut self.export.merge_images, "Tek görselde birleştir");
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if action_button(ui, "Klasör Seç", SLATE).clicked() {
                    if let Some(dir) = pick_folder("Çıktı klasörü seç") {
                        self.export.out_dir = Some(dir);
                    }
                }
                let text = self
                    .export
                    .out_dir
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .or_else(|| {
                        self.export
                            .path
                            .as_ref()
                            .map(|p| dir_of(p).display().to_string())
                    })
                    .unwrap_or_else(|| "PDF seçilmedi".to_string());
                ui.label(RichText::new(text).size(11.5).color(MUTED));
            });
            ui.add_space(6.0);
            let running = self.is_running(Tab::Export);
            ui.add_enabled_ui(
                !running && !self.export.selected.is_empty() && !self.export.bytes.is_empty(),
                |ui| {
                    if action_button(ui, "Dışa Aktar", SLATE).clicked() {
                        self.start_export(ui.ctx());
                    }
                },
            );
            ui.add_space(4.0);
            self.progress_ui(ui, Tab::Export);
            self.status_label(ui, Tab::Export);
        });
    }

    fn start_export(&mut self, ctx: &egui::Context) {
        let Some(path) = self.export.path.clone() else {
            self.error("Eksik Bilgi", "Önce bir PDF açın.");
            return;
        };
        if self.export.selected.is_empty() {
            self.error("Eksik Bilgi", "En az bir sayfa seçin.");
            return;
        }
        let bytes = self.export.bytes.clone();
        let pages = self.export.selected.clone();
        let fmt = self.export.format;
        let dpi = parse_u32(&self.export.dpi, 150).clamp(50, 600);
        let merge = self.export.merge_images;
        let out_dir = output_dir(&self.export.out_dir, Some(&path));
        let soffice = self.docx.soffice.clone();
        if matches!(fmt, ExportFormat::Doc) && soffice.is_none() {
            self.error(
                "LibreOffice Bulunamadı",
                "DOC çıktısı için LibreOffice gerekir. DOCX → PDF sekmesinden soffice.exe yolunu seçebilir veya doğrudan DOCX formatını kullanabilirsiniz.",
            );
            return;
        }
        if let Err(e) = std::fs::create_dir_all(&out_dir) {
            self.error("Klasör Oluşturulamadı", format!("{e}"));
            return;
        }

        self.spawn_job(ctx, Tab::Export, move |jc| {
            let count = crate::export::export(
                &path,
                &bytes,
                &pages,
                &out_dir,
                fmt,
                dpi,
                merge,
                soffice.as_deref(),
                |p, t| {
                    if t > 0 {
                        jc.progress(p as f32 / t as f32, format!("Dışa aktarılıyor: {p} / {t}"));
                    }
                },
            )?;
            jc.progress(1.0, "Tamamlandı");
            jc.done(
                format!("{count} sayfa dışa aktarıldı."),
                format!(
                    "{count} sayfa {} formatında kaydedildi.\n\nKlasör:\n{}",
                    fmt.title(),
                    out_dir.display()
                ),
            );
            Ok(())
        });
    }

    pub fn ui_print(&mut self, ui: &mut egui::Ui) {
        self.page_title(ui, "PDF Yazdır", NAVY);
        ui.label(
            RichText::new(
                "PDF'i doğrudan yazıcıya gönderin. Sayfa aralığı, kopya sayısı ve kâğıda sığdırma ayarlanabilir; ek bir PDF okuyucu gerekmez.",
            )
            .size(12.0)
            .color(MUTED),
        );
        ui.add_space(10.0);

        if !self.print.printers_loaded {
            self.refresh_printers();
        }

        card(ui, "Kaynak PDF", |ui| {
            ui.horizontal(|ui| {
                if action_button(ui, "PDF Aç", PRINT_COLOR).clicked() {
                    if let Some(path) = pick_file("Yazdırılacak PDF'yi seç", "PDF", &["pdf"]) {
                        self.open_print(path);
                    }
                }
                if let Some(path) = &self.print.path {
                    ui.label(
                        RichText::new(crate::util::file_name(path))
                            .size(12.5)
                            .strong()
                            .color(INK),
                    );
                    ui.label(
                        RichText::new(format!("{} sayfa", self.print.total))
                            .size(11.5)
                            .color(MUTED),
                    );
                } else {
                    ui.label(RichText::new("PDF seçilmedi").size(12.0).color(MUTED));
                }
            });
        });

        card(ui, "Sayfalar", |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Sayfa aralığı:").size(12.0).color(MUTED));
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.print.ranges)
                        .hint_text("1-5, 8, 10-12")
                        .desired_width(220.0),
                );
                if resp.lost_focus() {
                    if let Ok(pages) =
                        crate::util::parse_ranges(&self.print.ranges, self.print.total)
                    {
                        self.print.selected = pages;
                    }
                }
                if ui.button("Sayfa Seç").clicked() && self.print.total > 0 {
                    self.page_dialog = Some(PageDialog {
                        target: PageTarget::Print,
                        total: self.print.total,
                        selected: (1..=self.print.total)
                            .map(|p| self.print.selected.contains(&p))
                            .collect(),
                        ranges: crate::util::format_ranges(&self.print.selected),
                        error: None,
                    });
                }
                if self.print.total > 0 && ui.small_button("Tümü").clicked() {
                    self.print.selected = (1..=self.print.total).collect();
                    self.print.ranges = format!("1-{}", self.print.total);
                }
                ui.label(
                    RichText::new(format!("{} sayfa seçili", self.print.selected.len()))
                        .size(11.0)
                        .color(MUTED),
                );
            });
        });

        card(ui, "Yazıcı ve Çıktı", |ui| {
            let mut refresh = false;
            ui.horizontal(|ui| {
                ui.label(RichText::new("Yazıcı:").size(12.0).color(MUTED));
                let selected = self
                    .print
                    .printer
                    .clone()
                    .unwrap_or_else(|| "Varsayılan yazıcı".to_string());
                egui::ComboBox::from_id_salt("print_printer")
                    .selected_text(selected)
                    .width(300.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.print.printer,
                            None,
                            "Varsayılan yazıcı",
                        );
                        for p in &self.print.printers {
                            let label = if p.is_default {
                                format!("{} (varsayılan)", p.name)
                            } else {
                                p.name.clone()
                            };
                            ui.selectable_value(
                                &mut self.print.printer,
                                Some(p.name.clone()),
                                label,
                            );
                        }
                    });
                if ghost_button(ui, "Yenile").clicked() {
                    refresh = true;
                }
            });
            if refresh {
                self.refresh_printers();
            }
            if self.print.printers.is_empty() {
                ui.label(
                    RichText::new(
                        "Kurulu yazıcı bulunamadı. Yazıcıyı Windows'a ekleyip 'Yenile' düğmesine basın.",
                    )
                    .size(10.5)
                    .color(RED),
                );
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Kopya:").size(12.0).color(MUTED));
                ui.add(egui::TextEdit::singleline(&mut self.print.copies).desired_width(50.0));
                ui.checkbox(&mut self.print.fit, "Sayfayı kâğıda sığdır");
                ui.label(
                    RichText::new("(işaretlenmezse gerçek boyutta yazdırılır)")
                        .size(10.0)
                        .color(MUTED),
                );
            });
            ui.add_space(6.0);
            let copies = parse_u32(&self.print.copies, 1).clamp(1, 99);
            ui.label(
                RichText::new(format!(
                    "{} sayfa × {copies} kopya = {} sayfa yazdırılacak",
                    self.print.selected.len(),
                    self.print.selected.len() as u32 * copies
                ))
                .size(11.0)
                .color(MUTED),
            );
            ui.add_space(6.0);
            let running = self.is_running(Tab::Print);
            let ready = self.print.path.is_some() && !self.print.selected.is_empty();
            ui.add_enabled_ui(!running && ready, |ui| {
                if action_button(ui, "Yazdır", PRINT_COLOR).clicked() {
                    self.start_print(ui.ctx());
                }
            });
            ui.add_space(4.0);
            self.progress_ui(ui, Tab::Print);
            self.status_label(ui, Tab::Print);
        });
    }

    fn start_print(&mut self, ctx: &egui::Context) {
        let Some(path) = self.print.path.clone() else {
            self.error("Eksik Bilgi", "Önce yazdırılacak PDF'yi seçin.");
            return;
        };
        if self.print.selected.is_empty() {
            self.error("Eksik Bilgi", "En az bir sayfa seçin.");
            return;
        }
        let bytes = self.print.bytes.clone();
        let pages = self.print.selected.clone();
        let copies = parse_u32(&self.print.copies, 1).clamp(1, 99);
        let fit = self.print.fit;
        let printer = self.print.printer.clone().unwrap_or_default();
        let title = crate::util::file_name(&path);

        self.spawn_job(ctx, Tab::Print, move |jc| {
            jc.progress(0.01, "Yazdırma hazırlanıyor...");
            let job = crate::print::PrintJob {
                printer,
                pages: pages.clone(),
                copies,
                fit,
                title: title.clone(),
            };
            let printed = crate::print::print_pdf(&bytes, &job, |done, total| {
                if total > 0 {
                    jc.progress(
                        done as f32 / total as f32,
                        format!("Yazdırılıyor: {done} / {total}"),
                    );
                }
            })?;
            jc.progress(1.0, "Yazdırma kuyruğa eklendi");
            jc.done(
                format!("{printed} sayfa yazdırıldı."),
                format!(
                    "{printed} sayfa yazıcıya gönderildi.\n\n\
                     Sayfalar: {}\nKopya: {copies}\n\n\
                     Belge yazdırma kuyruğunda görünmüyorsa yazıcının açık olduğunu kontrol edin.",
                    crate::util::format_ranges(&pages)
                ),
            );
            Ok(())
        });
    }
}

fn lang_title(code: &str) -> &'static str {
    match code {
        "tur" => "Türkçe",
        "tur+eng" => "Türkçe + İngilizce",
        "eng" => "İngilizce",
        "deu" => "Almanca",
        "fra" => "Fransızca",
        "ara" => "Arapça",
        "rus" => "Rusça",
        _ => "Özel",
    }
}
