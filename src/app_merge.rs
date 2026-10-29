use eframe::egui::{self, Align, Layout, RichText, Stroke};
use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::app::*;
use crate::model::{PdfFileEntry, Tab};

impl PdfToolboxApp {
    pub fn ui_merge(&mut self, ui: &mut egui::Ui) {
        self.page_title(ui, "PDF Birleştir", NAVY);
        ui.label(
            RichText::new(
                "Birden fazla PDF'yi sırayla birleştirin. Her dosya için istediğiniz sayfaları seçebilirsiniz.",
            )
            .size(12.0)
            .color(MUTED),
        );
        ui.add_space(10.0);

        card(ui, "PDF Dosyaları", |ui| {
            ui.horizontal(|ui| {
                if action_button(ui, "PDF Ekle", BLUE).clicked() {
                    let files = pick_files("PDF dosyaları seç", "PDF", &["pdf"]);
                    self.add_merge_files(&files);
                }
                if ghost_button(ui, "Klasörden Ekle").clicked() {
                    if let Some(dir) = pick_folder("PDF klasörü seç") {
                        let files = crate::convert::collect_files_recursive(&dir, &["pdf"]);
                        self.add_merge_files(&files);
                    }
                }
                if ghost_button(ui, "Listeyi Temizle").clicked() && !self.merge.files.is_empty() {
                    self.merge.files.clear();
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} dosya", self.merge.files.len()))
                            .size(12.0)
                            .color(MUTED),
                    );
                });
            });
            ui.add_space(4.0);
            if self.merge.files.is_empty() {
                ui.label(
                    RichText::new(
                        "Henüz PDF eklenmedi. Dosyaları bu pencereye sürükleyebilirsiniz.",
                    )
                    .size(12.0)
                    .color(MUTED),
                );
            }
            let mut remove: Option<usize> = None;
            let mut move_up: Option<usize> = None;
            let mut move_down: Option<usize> = None;
            let mut open_pages: Option<usize> = None;
            for (i, file) in self.merge.files.iter_mut().enumerate() {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(248, 250, 252))
                    .stroke(Stroke::new(1.0_f32, BORDER))
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::symmetric(10, 7))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{}.", i + 1))
                                    .size(12.0)
                                    .strong()
                                    .color(BLUE),
                            );
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&file.name).size(12.5).strong().color(INK));
                                let count = file.selected.len();
                                ui.label(
                                    RichText::new(format!(
                                        "{} sayfa • Seçili: {} ({})",
                                        file.total_pages,
                                        count,
                                        crate::util::short_ranges(&file.selected, 42)
                                    ))
                                    .size(10.5)
                                    .color(MUTED),
                                );
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if small_button(ui, "Sil", RED).clicked() {
                                    remove = Some(i);
                                }
                                if small_button(ui, "▼", SLATE).clicked() {
                                    move_down = Some(i);
                                }
                                if small_button(ui, "▲", SLATE).clicked() {
                                    move_up = Some(i);
                                }
                                if small_button(ui, "Sayfalar", BLUE).clicked() {
                                    open_pages = Some(i);
                                }
                            });
                        });
                    });
                ui.add_space(3.0);
            }
            if let Some(i) = move_up {
                if i > 0 {
                    self.merge.files.swap(i, i - 1);
                }
            }
            if let Some(i) = move_down {
                if i + 1 < self.merge.files.len() {
                    self.merge.files.swap(i, i + 1);
                }
            }
            if let Some(i) = remove {
                self.confirm = Some(ConfirmBox {
                    title: "Dosyayı Kaldır".to_string(),
                    text: format!("\"{}\" listeden kaldırılsın mı?", self.merge.files[i].name),
                    action: ConfirmAction::RemoveMerge(i),
                });
            }
            if let Some(i) = open_pages {
                let file = &self.merge.files[i];
                self.page_dialog = Some(PageDialog {
                    target: PageTarget::Merge(i),
                    total: file.total_pages,
                    selected: (1..=file.total_pages)
                        .map(|p| file.selected.contains(&p))
                        .collect(),
                    ranges: crate::util::format_ranges(&file.selected),
                    error: None,
                });
            }
        });

        card(ui, "Çıktı", |ui| {
            ui.horizontal(|ui| {
                if action_button(ui, "Klasör Seç", SLATE).clicked() {
                    if let Some(dir) = pick_folder("Çıktı klasörü seç") {
                        self.merge.out_dir = Some(dir);
                    }
                }
                let text = self
                    .merge
                    .out_dir
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "İlk PDF'nin klasörü".to_string());
                ui.label(RichText::new(text).size(11.5).color(MUTED));
                if self.merge.out_dir.is_some() && ui.small_button("Sıfırla").clicked() {
                    self.merge.out_dir = None;
                }
            });
            ui.add_space(6.0);
            let ready = !self.merge.files.is_empty() || self.is_running(Tab::Merge);
            let running = self.is_running(Tab::Merge);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(ready && !running, |ui| {
                    if action_button(ui, "PDF'leri Birleştir", BLUE).clicked() {
                        self.start_merge(ui.ctx());
                    }
                });
                if running {
                    ui.label(
                        RichText::new("Birleştirme sürüyor...")
                            .size(12.0)
                            .color(ORANGE),
                    );
                }
            });
            ui.add_space(4.0);
            self.progress_ui(ui, Tab::Merge);
            self.status_label(ui, Tab::Merge);
        });
    }

    fn start_merge(&mut self, ctx: &egui::Context) {
        if self.merge.files.is_empty() {
            self.error("Eksik Bilgi", "Birleştirmek için en az bir PDF ekleyin.");
            return;
        }
        if self.fonts.is_none() {
            // Birleştirme yazı tipi gerektirmez; yine de kullanıcıyı bilgilendirmiyoruz.
        }
        let items: Vec<(PathBuf, Vec<u32>)> = self
            .merge
            .files
            .iter()
            .map(|f| (f.path.clone(), f.selected.clone()))
            .collect();
        let first = self.merge.files[0].path.clone();
        let out_dir = output_dir(&self.merge.out_dir, Some(&first));

        self.spawn_job(ctx, Tab::Merge, move |jc| {
            let total = items.len() as u32;
            let mut sets: Vec<(Vec<u8>, Vec<u32>)> = Vec::new();
            for (i, (path, pages)) in items.iter().enumerate() {
                jc.progress(
                    i as f32 / total.max(1) as f32,
                    format!("Okunuyor: {}", crate::util::file_name(path)),
                );
                if pages.is_empty() {
                    continue;
                }
                let bytes = std::fs::read(path)
                    .map_err(|e| anyhow::anyhow!("{} okunamadı: {e}", path.display()))?;
                sets.push((bytes, pages.clone()));
            }
            if sets.is_empty() {
                return Err(anyhow::anyhow!(
                    "Seçili sayfa bulunan PDF yok. En az bir dosyada sayfa seçin."
                ));
            }
            jc.progress(0.75, "PDF'ler birleştiriliyor...");
            std::fs::create_dir_all(&out_dir)?;
            let out = crate::util::unique_output(&out_dir, "birlesik", "pdf");
            let bytes = crate::pdfcore::merge_page_sets_to_bytes(&sets)?;
            std::fs::write(&out, bytes)?;
            jc.progress(1.0, "Tamamlandı");
            jc.done(
                format!("Birleştirildi: {}", out.display()),
                format!(
                    "{} PDF birleştirildi.\n\nÇıktı dosyası:\n{}",
                    sets.len(),
                    out.display()
                ),
            );
            Ok(())
        });
    }

    pub fn ui_split(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.page_title(ui, "PDF Böl", NAVY);
        ui.label(
            RichText::new(
                "Bir PDF'yi sayfa aralıklarına göre ayırın veya her sayfayı ayrı dosya yapın.",
            )
            .size(12.0)
            .color(MUTED),
        );
        ui.add_space(10.0);

        card(ui, "Kaynak PDF", |ui| {
            ui.horizontal(|ui| {
                if action_button(ui, "PDF Aç", TEAL).clicked() {
                    if let Some(path) = pick_file("Bölünecek PDF'yi seç", "PDF", &["pdf"]) {
                        self.open_split(ctx, path);
                    }
                }
                if let Some(file) = &self.split.file {
                    ui.label(RichText::new(&file.name).size(12.5).strong().color(INK));
                    ui.label(
                        RichText::new(format!("{} sayfa", file.total_pages))
                            .size(11.5)
                            .color(MUTED),
                    );
                } else {
                    ui.label(RichText::new("PDF seçilmedi").size(12.0).color(MUTED));
                }
            });
        });

        card(ui, "Bölme Yöntemi", |ui| {
            for mode in [SplitMode::Selected, SplitMode::EveryPage, SplitMode::Chunks] {
                ui.radio_value(&mut self.split.mode, mode, mode.title());
            }
            ui.add_space(6.0);
            match self.split.mode {
                SplitMode::Selected => {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Sayfalar:").size(12.0).color(MUTED));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.split.ranges)
                                .hint_text("1-3, 5, 8-10")
                                .desired_width(220.0),
                        );
                        if ui.button("Sayfa Seç").clicked() {
                            if let Some(file) = &self.split.file {
                                let selected =
                                    crate::util::parse_ranges(&self.split.ranges, file.total_pages)
                                        .unwrap_or_else(|_| (1..=file.total_pages).collect());
                                self.page_dialog = Some(PageDialog {
                                    target: PageTarget::Split,
                                    total: file.total_pages,
                                    selected: (1..=file.total_pages)
                                        .map(|p| selected.contains(&p))
                                        .collect(),
                                    ranges: crate::util::format_ranges(&selected),
                                    error: None,
                                });
                            }
                        }
                        if let Some(file) = &self.split.file {
                            if ui.small_button("Tümü").clicked() {
                                self.split.ranges = format!("1-{}", file.total_pages);
                            }
                        }
                    });
                    ui.label(
                        RichText::new("Seçilen sayfalar tek bir PDF dosyası olarak kaydedilir.")
                            .size(10.5)
                            .color(MUTED),
                    );
                }
                SplitMode::EveryPage => {
                    ui.label(
                        RichText::new(
                            "Her sayfa ayrı bir PDF olarak kaydedilir: dosya_sayfa_1.pdf, dosya_sayfa_2.pdf ...",
                        )
                        .size(11.0)
                        .color(MUTED),
                    );
                }
                SplitMode::Chunks => {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Her dosyada sayfa sayısı:")
                                .size(12.0)
                                .color(MUTED),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut self.split.chunk).desired_width(60.0),
                        );
                    });
                    ui.label(
                        RichText::new("Sayfalar sırayla gruplanır: dosya_bolum_1.pdf ...")
                            .size(10.5)
                            .color(MUTED),
                    );
                }
            }
        });

        card(ui, "Çıktı ve İşlem", |ui| {
            ui.horizontal(|ui| {
                if action_button(ui, "Klasör Seç", SLATE).clicked() {
                    if let Some(dir) = pick_folder("Çıktı klasörü seç") {
                        self.split.out_dir = Some(dir);
                    }
                }
                let text = self
                    .split
                    .out_dir
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .or_else(|| {
                        self.split
                            .file
                            .as_ref()
                            .map(|f| dir_of(&f.path).display().to_string())
                    })
                    .unwrap_or_else(|| "Seçilmedi".to_string());
                ui.label(RichText::new(text).size(11.5).color(MUTED));
                if self.split.out_dir.is_some() && ui.small_button("Sıfırla").clicked() {
                    self.split.out_dir = None;
                }
            });
            ui.add_space(6.0);
            let ready = self.split.file.is_some() && !self.is_running(Tab::Split);
            ui.add_enabled_ui(ready, |ui| {
                if action_button(ui, "PDF'i Böl", TEAL).clicked() {
                    self.start_split(ctx);
                }
            });
            ui.add_space(4.0);
            self.progress_ui(ui, Tab::Split);
            self.status_label(ui, Tab::Split);
        });
    }

    fn start_split(&mut self, ctx: &egui::Context) {
        let Some(file) = &self.split.file else {
            self.error("Eksik Bilgi", "Önce bölünecek PDF'yi açın.");
            return;
        };
        let total = file.total_pages;
        let bytes = self.split.bytes.clone();
        let src_name = file.name.clone();
        let source = file.path.clone();
        let stem = crate::util::stem(&source);
        let out_dir = output_dir(&self.split.out_dir, Some(&source));
        let mode = self.split.mode;
        let pages = match crate::util::parse_ranges(&self.split.ranges, total) {
            Ok(p) => p,
            Err(e) if mode != SplitMode::Chunks => {
                self.error("Sayfa Seçimi", e);
                return;
            }
            Err(_) => (1..=total).collect(),
        };
        let chunk = parse_u32(&self.split.chunk, 2).max(1);

        self.spawn_job(ctx, Tab::Split, move |jc| {
            std::fs::create_dir_all(&out_dir)?;
            let mut produced = Vec::new();
            match mode {
                SplitMode::Selected => {
                    jc.progress(0.3, "Sayfalar çıkarılıyor...");
                    let out =
                        crate::util::unique_output(&out_dir, &format!("{stem}_secili"), "pdf");
                    let data = crate::pdfcore::extract_pages_to_bytes(&bytes, &pages)?;
                    std::fs::write(&out, data)?;
                    produced.push(out);
                    jc.progress(1.0, "Tamamlandı");
                }
                SplitMode::EveryPage => {
                    let count = pages.len() as u32;
                    for (i, page) in pages.iter().enumerate() {
                        jc.progress(
                            i as f32 / count.max(1) as f32,
                            format!("Sayfa {page} kaydediliyor..."),
                        );
                        let out = crate::util::unique_output(
                            &out_dir,
                            &format!("{stem}_sayfa_{page}"),
                            "pdf",
                        );
                        let data = crate::pdfcore::extract_pages_to_bytes(&bytes, &[*page])?;
                        std::fs::write(&out, data)?;
                        produced.push(out);
                    }
                    jc.progress(1.0, "Tamamlandı");
                }
                SplitMode::Chunks => {
                    let count = pages.len().div_ceil(chunk as usize) as u32;
                    for (i, group) in pages.chunks(chunk as usize).enumerate() {
                        jc.progress(
                            i as f32 / count.max(1) as f32,
                            format!("Bölüm {} kaydediliyor...", i + 1),
                        );
                        let out = crate::util::unique_output(
                            &out_dir,
                            &format!("{stem}_bolum_{}", i + 1),
                            "pdf",
                        );
                        let data = crate::pdfcore::extract_pages_to_bytes(&bytes, group)?;
                        std::fs::write(&out, data)?;
                        produced.push(out);
                    }
                    jc.progress(1.0, "Tamamlandı");
                }
            }
            let list: Vec<String> = produced
                .iter()
                .take(12)
                .map(|p| format!("• {}", p.display()))
                .collect();
            let more = if produced.len() > 12 {
                format!("\n... ve {} dosya daha", produced.len() - 12)
            } else {
                String::new()
            };
            jc.done(
                format!("{} dosya oluşturuldu.", produced.len()),
                format!(
                    "{src_name} bölündü.\n\n{}{more}\n\nKlasör:\n{}",
                    list.join("\n"),
                    out_dir.display()
                ),
            );
            Ok(())
        });
    }

    #[allow(dead_code)]
    fn selected_pages_of(file: &PdfFileEntry) -> Vec<u32> {
        let set: BTreeSet<u32> = file.selected.iter().copied().collect();
        set.into_iter().collect()
    }
}
