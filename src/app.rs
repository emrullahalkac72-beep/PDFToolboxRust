use eframe::egui::{
    self, Align, Color32, CornerRadius, FontFamily, FontId, Frame, Layout, Margin, RichText,
    Stroke, Vec2,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::convert::Engine;
use crate::export::ExportFormat;
use crate::jobs::{JobBus, JobCtx, JobMessage};
use crate::model::{OcrMode, PdfFileEntry, Tab, WatermarkMode, PALETTE};

pub const NAVY: Color32 = Color32::from_rgb(17, 32, 69);
pub const NAVY_2: Color32 = Color32::from_rgb(30, 58, 138);
pub const BG: Color32 = Color32::from_rgb(244, 246, 251);
pub const CARD: Color32 = Color32::WHITE;
pub const BORDER: Color32 = Color32::from_rgb(226, 232, 240);
pub const INK: Color32 = Color32::from_rgb(30, 41, 59);
pub const MUTED: Color32 = Color32::from_rgb(100, 116, 139);
pub const BLUE: Color32 = Color32::from_rgb(37, 99, 235);
pub const GREEN: Color32 = Color32::from_rgb(22, 163, 74);
pub const RED: Color32 = Color32::from_rgb(220, 38, 38);
pub const ORANGE: Color32 = Color32::from_rgb(234, 88, 12);
pub const PURPLE: Color32 = Color32::from_rgb(124, 58, 237);
pub const TEAL: Color32 = Color32::from_rgb(13, 148, 136);
pub const PINK: Color32 = Color32::from_rgb(219, 39, 119);
pub const SLATE: Color32 = Color32::from_rgb(71, 85, 105);
pub const PRINT_COLOR: Color32 = Color32::from_rgb(8, 145, 178);

pub const DEVELOPER: &str = "";

pub fn tab_color(tab: Tab) -> Color32 {
    match tab {
        Tab::Merge => BLUE,
        Tab::Split => TEAL,
        Tab::Convert => GREEN,
        Tab::Docx => Color32::from_rgb(37, 99, 235),
        Tab::Edit => ORANGE,
        Tab::Udf => PURPLE,
        Tab::PageNo => Color32::from_rgb(2, 132, 199),
        Tab::Ocr => PINK,
        Tab::Export => SLATE,
        Tab::Print => PRINT_COLOR,
    }
}

pub fn card(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).size(15.0).strong().color(INK));
            ui.add_space(4.0);
            add(ui);
        });
    ui.add_space(10.0);
}

pub fn action_button(ui: &mut egui::Ui, text: &str, color: Color32) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(Color32::WHITE).strong())
            .fill(color)
            .min_size(Vec2::new(120.0, 32.0))
            .corner_radius(8.0),
    )
}

pub fn small_button(ui: &mut egui::Ui, text: &str, color: Color32) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(Color32::WHITE))
            .fill(color)
            .min_size(Vec2::new(32.0, 24.0))
            .corner_radius(6.0),
    )
}

pub fn ghost_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(INK))
            .fill(Color32::from_rgb(241, 245, 249))
            .stroke(Stroke::new(1.0_f32, BORDER))
            .min_size(Vec2::new(32.0, 28.0))
            .corner_radius(6.0),
    )
}

pub fn color_picker(ui: &mut egui::Ui, id: &str, value: &mut String) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(
            PALETTE
                .iter()
                .find(|(k, _)| *k == value.as_str())
                .map(|(_, t)| *t)
                .unwrap_or("Özel"),
        )
        .width(130.0)
        .show_ui(ui, |ui| {
            for (key, name) in PALETTE {
                let c = crate::model::palette_color(key);
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::new(14.0, 14.0), egui::Sense::hover());
                    ui.painter().rect_filled(
                        rect,
                        CornerRadius::same(3),
                        Color32::from_rgb(c[0], c[1], c[2]),
                    );
                    ui.selectable_value(value, key.to_string(), name);
                });
            }
        });
}

pub fn parse_u32(text: &str, default: u32) -> u32 {
    text.trim().parse().unwrap_or(default)
}

pub fn parse_f32(text: &str, default: f32) -> f32 {
    text.trim().replace(',', ".").parse().unwrap_or(default)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SplitMode {
    Selected,
    EveryPage,
    Chunks,
}

impl SplitMode {
    pub fn title(self) -> &'static str {
        match self {
            SplitMode::Selected => "Seçilen sayfalar tek PDF",
            SplitMode::EveryPage => "Her sayfa ayrı PDF",
            SplitMode::Chunks => "Belirli sayıda grupla",
        }
    }
}

pub struct MergeState {
    pub files: Vec<PdfFileEntry>,
    pub out_dir: Option<PathBuf>,
}

impl Default for MergeState {
    fn default() -> Self {
        Self {
            files: Vec::new(),
            out_dir: None,
        }
    }
}

pub struct SplitState {
    pub file: Option<PdfFileEntry>,
    pub bytes: Vec<u8>,
    pub mode: SplitMode,
    pub ranges: String,
    pub chunk: String,
    pub out_dir: Option<PathBuf>,
}

impl Default for SplitState {
    fn default() -> Self {
        Self {
            file: None,
            bytes: Vec::new(),
            mode: SplitMode::Selected,
            ranges: String::new(),
            chunk: "2".to_string(),
            out_dir: None,
        }
    }
}

pub struct ConvertState {
    pub rows: Vec<crate::model::FileRow>,
    pub out_dir: Option<PathBuf>,
    pub engine: Engine,
}

impl Default for ConvertState {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            out_dir: None,
            engine: Engine::Auto,
        }
    }
}

pub struct DocxState {
    pub rows: Vec<crate::model::FileRow>,
    pub out_dir: Option<PathBuf>,
    pub engine: Engine,
    pub soffice: Option<PathBuf>,
}

impl Default for DocxState {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            out_dir: None,
            engine: Engine::Auto,
            soffice: None,
        }
    }
}

pub struct UdfState {
    pub rows: Vec<crate::model::FileRow>,
    pub out_dir: Option<PathBuf>,
}

impl Default for UdfState {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            out_dir: None,
        }
    }
}

pub struct PageNoState {
    pub path: Option<PathBuf>,
    pub bytes: Vec<u8>,
    pub total: u32,
    pub start: String,
    pub size: String,
    pub vertical: String,
    pub horizontal: String,
    pub format: String,
    pub skip_first: bool,
    pub out_dir: Option<PathBuf>,
}

impl Default for PageNoState {
    fn default() -> Self {
        Self {
            path: None,
            bytes: Vec::new(),
            total: 0,
            start: "1".to_string(),
            size: "12".to_string(),
            vertical: "Alt".to_string(),
            horizontal: "Orta".to_string(),
            format: "Sayfa {n} / {t}".to_string(),
            skip_first: false,
            out_dir: None,
        }
    }
}

pub struct OcrState {
    pub path: Option<PathBuf>,
    pub mode: OcrMode,
    pub lang: String,
    pub dpi: String,
    pub tess: Option<PathBuf>,
    pub out_dir: Option<PathBuf>,
}

impl Default for OcrState {
    fn default() -> Self {
        Self {
            path: None,
            mode: OcrMode::Searchable,
            lang: "tur".to_string(),
            dpi: "300".to_string(),
            tess: None,
            out_dir: None,
        }
    }
}

pub struct ExportState {
    pub path: Option<PathBuf>,
    pub bytes: Vec<u8>,
    pub total: u32,
    pub selected: Vec<u32>,
    pub ranges: String,
    pub format: ExportFormat,
    pub dpi: String,
    pub merge_images: bool,
    pub out_dir: Option<PathBuf>,
}

impl Default for ExportState {
    fn default() -> Self {
        Self {
            path: None,
            bytes: Vec::new(),
            total: 0,
            selected: Vec::new(),
            ranges: String::new(),
            format: ExportFormat::Png,
            dpi: "150".to_string(),
            merge_images: false,
            out_dir: None,
        }
    }
}

pub struct PrintState {
    pub path: Option<PathBuf>,
    pub bytes: Vec<u8>,
    pub total: u32,
    pub selected: Vec<u32>,
    pub ranges: String,
    /// Seçili yazıcı; `None` ise Windows varsayılan yazıcısı kullanılır.
    pub printer: Option<String>,
    pub printers: Vec<crate::print::PrinterInfo>,
    pub printers_loaded: bool,
    pub copies: String,
    pub fit: bool,
}

impl Default for PrintState {
    fn default() -> Self {
        Self {
            path: None,
            bytes: Vec::new(),
            total: 0,
            selected: Vec::new(),
            ranges: String::new(),
            printer: None,
            printers: Vec::new(),
            printers_loaded: false,
            copies: "1".to_string(),
            fit: true,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EditTool {
    Text,
    Rectangle,
    Circle,
    Line,
    Image,
    TextSelect,
}

impl EditTool {
    pub fn title(self) -> &'static str {
        match self {
            EditTool::Text => "Metin Ekle",
            EditTool::Rectangle => "Dikdörtgen",
            EditTool::Circle => "Daire",
            EditTool::Line => "Çizgi",
            EditTool::Image => "Resim Ekle",
            EditTool::TextSelect => "Metni Düzenle",
        }
    }
}

pub struct TextSettings {
    pub text: String,
    pub size: String,
    pub color: String,
    pub serif: bool,
    pub bold: bool,
    pub all_pages: bool,
}

impl Default for TextSettings {
    fn default() -> Self {
        Self {
            text: String::new(),
            size: "18".to_string(),
            color: "black".to_string(),
            serif: false,
            bold: false,
            all_pages: false,
        }
    }
}

pub struct WatermarkSettings {
    pub text: String,
    pub size: String,
    pub alpha: String,
    pub color: String,
    pub mode: WatermarkMode,
}

impl Default for WatermarkSettings {
    fn default() -> Self {
        Self {
            text: "GİZLİ".to_string(),
            size: "48".to_string(),
            alpha: "0.25".to_string(),
            color: "gray".to_string(),
            mode: WatermarkMode::Diagonal,
        }
    }
}

pub struct EditState {
    pub path: Option<PathBuf>,
    pub bytes: Vec<u8>,
    pub page: u32,
    pub total: u32,
    pub generation: u64,
    pub texture: Option<egui::TextureHandle>,
    pub rendered: Option<crate::model::RenderedPage>,
    pub page_w: f32,
    pub page_h: f32,
    /// Kullanıcı yakınlaştırma oranı (1.0 = %100, yani 96 DPI gerçek boyut).
    pub zoom: f32,
    /// Sayfa görünüm alanına sığdırılıyor mu?
    pub fit: bool,
    /// Son çizilen önizlemenin PDF noktası başına piksel ölçeği.
    pub preview_scale: f32,
    /// Son görüntüleme ölçeği (nokta → mantıksal piksel).
    pub view_scale: f32,
    /// Yakınlaştırma değişikliğinden bu yana geçen süreyi ölçmek için.
    pub zoom_changed_at: f64,
    /// Yakınlaştırma sonrası görünümü sayfa merkezine getir.
    pub center_view: bool,
    pub scroll: egui::Vec2,
    pub tool: EditTool,
    pub actions: Vec<crate::model::EditItem>,
    pub cursor: usize,
    pub text_objects: Vec<crate::model::TextObject>,
    /// "Metni Düzenle" aracında düzenlenebilir metin alanları vurgulanır.
    pub show_text_boxes: bool,
    pub selected: Option<crate::model::TextObject>,
    pub edit_new: String,
    pub edit_size: String,
    pub edit_color: String,
    pub edit_bg: String,
    pub edit_serif: bool,
    pub edit_bold: bool,
    pub text: TextSettings,
    pub wm: WatermarkSettings,
    pub wm_all: bool,
    pub shape_color: String,
    pub shape_fill: bool,
    pub shape_lw: String,
    pub rotate_all: bool,
    pub pending_pos: Option<(f32, f32)>,
    pub drag_start: Option<(f32, f32)>,
    pub drag_current: Option<(f32, f32)>,
    pub show_text_dialog: bool,
    pub show_edit_dialog: bool,
}

impl Default for EditState {
    fn default() -> Self {
        Self {
            path: None,
            bytes: Vec::new(),
            page: 1,
            total: 0,
            generation: 0,
            texture: None,
            rendered: None,
            page_w: 595.0,
            page_h: 842.0,
            zoom: 1.0,
            fit: true,
            preview_scale: 2.0,
            view_scale: 1.0,
            zoom_changed_at: 0.0,
            center_view: false,
            scroll: egui::Vec2::ZERO,
            tool: EditTool::Text,
            actions: Vec::new(),
            cursor: 0,
            text_objects: Vec::new(),
            show_text_boxes: true,
            selected: None,
            edit_new: String::new(),
            edit_size: "12".to_string(),
            edit_color: "black".to_string(),
            edit_bg: "#FFFFFF".to_string(),
            edit_serif: false,
            edit_bold: false,
            text: TextSettings::default(),
            wm: WatermarkSettings::default(),
            wm_all: true,
            shape_color: "red".to_string(),
            shape_fill: false,
            shape_lw: "2".to_string(),
            rotate_all: false,
            pending_pos: None,
            drag_start: None,
            drag_current: None,
            show_text_dialog: false,
            show_edit_dialog: false,
        }
    }
}

impl EditState {
    pub fn current(&self) -> &[crate::model::EditItem] {
        &self.actions[..self.cursor.min(self.actions.len())]
    }

    pub fn push(&mut self, item: crate::model::EditItem) {
        self.actions.truncate(self.cursor);
        self.actions.push(item);
        self.cursor = self.actions.len();
    }

    pub fn undo(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn redo(&mut self) {
        if self.cursor < self.actions.len() {
            self.cursor += 1;
        }
    }
}

pub struct MessageBox {
    pub title: String,
    pub text: String,
    pub error: bool,
}

pub struct ConfirmBox {
    pub title: String,
    pub text: String,
    pub action: ConfirmAction,
}

pub enum ConfirmAction {
    RemoveMerge(usize),
    RemoveConvert(usize),
    RemoveDocx(usize),
    RemoveUdf(usize),
    ClearEditActions,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PageTarget {
    Merge(usize),
    Split,
    Export,
    Print,
}

pub struct PageDialog {
    pub target: PageTarget,
    pub total: u32,
    pub selected: Vec<bool>,
    pub ranges: String,
    pub error: Option<String>,
}

pub struct PdfToolboxApp {
    pub tab: Tab,
    pub bus: JobBus,
    pub running: HashSet<Tab>,
    pub progress: HashMap<Tab, f32>,
    pub progress_text: HashMap<Tab, String>,
    pub status: HashMap<Tab, String>,
    pub message: Option<MessageBox>,
    pub confirm: Option<ConfirmBox>,
    pub page_dialog: Option<PageDialog>,
    pub merge: MergeState,
    pub split: SplitState,
    pub convert: ConvertState,
    pub docx: DocxState,
    pub udf: UdfState,
    pub edit: EditState,
    pub pageno: PageNoState,
    pub ocr: OcrState,
    pub export: ExportState,
    pub print: PrintState,
    pub fonts: Option<crate::fonts::FontSet>,
    pub fonts_error: Option<String>,
    /// Başlıkta ve sayfalarda çizilen uygulama simgesi.
    pub icon: Option<egui::TextureHandle>,
}

impl PdfToolboxApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_fonts(&cc.egui_ctx);
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::light();
        style.visuals.panel_fill = BG;
        style.visuals.window_fill = CARD;
        style.visuals.override_text_color = Some(INK);
        style.spacing.item_spacing = Vec2::new(8.0, 7.0);
        style.spacing.button_padding = Vec2::new(10.0, 5.0);
        cc.egui_ctx.set_style(style);

        let (fonts, fonts_error) = match crate::fonts::FontSet::load() {
            Ok(f) => (Some(f), None),
            Err(e) => (None, Some(format!("{e:#}"))),
        };

        let mut ocr = OcrState::default();
        ocr.tess = crate::ocr::find_tesseract(None);
        let mut docx = DocxState::default();
        docx.soffice = crate::convert::find_soffice(None);

        let icon = crate::icon::color_image(64).map(|image| {
            cc.egui_ctx
                .load_texture("app_icon", image, egui::TextureOptions::LINEAR)
        });

        Self {
            tab: Tab::Merge,
            bus: JobBus::new(),
            running: HashSet::new(),
            progress: HashMap::new(),
            progress_text: HashMap::new(),
            status: HashMap::new(),
            message: None,
            confirm: None,
            page_dialog: None,
            merge: MergeState::default(),
            split: SplitState::default(),
            convert: ConvertState::default(),
            docx,
            udf: UdfState::default(),
            edit: EditState::default(),
            pageno: PageNoState::default(),
            ocr,
            export: ExportState::default(),
            print: PrintState::default(),
            fonts,
            fonts_error,
            icon,
        }
    }

    /// Simgeyi verilen boyutta çizer. Doku yüklenemezse eski "PDF" rozetine düşer.
    pub fn draw_icon(&self, ui: &mut egui::Ui, size: f32) {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
        match &self.icon {
            Some(texture) => {
                ui.painter().image(
                    texture.id(),
                    rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            None => {
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(6), Color32::from_rgb(37, 99, 235));
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "PDF",
                    FontId::new(size * 0.36, FontFamily::Proportional),
                    Color32::WHITE,
                );
            }
        }
    }

    /// Sayfa başlığı: simge + başlık metni.
    pub fn page_title(&self, ui: &mut egui::Ui, title: &str, color: Color32) {
        ui.horizontal(|ui| {
            self.draw_icon(ui, 26.0);
            ui.label(RichText::new(title).size(20.0).strong().color(color));
        });
    }

    pub fn alert(&mut self, title: impl Into<String>, text: impl Into<String>) {
        self.message = Some(MessageBox {
            title: title.into(),
            text: text.into(),
            error: false,
        });
    }

    pub fn error(&mut self, title: impl Into<String>, text: impl Into<String>) {
        self.message = Some(MessageBox {
            title: title.into(),
            text: text.into(),
            error: true,
        });
    }

    pub fn note(&mut self, tab: Tab, text: impl Into<String>) {
        self.status.insert(tab, text.into());
    }

    pub fn is_running(&self, tab: Tab) -> bool {
        self.running.contains(&tab)
    }

    pub fn set_running(&mut self, tab: Tab) {
        self.running.insert(tab);
        self.progress.insert(tab, 0.0);
        self.progress_text
            .insert(tab, "İşlem başlatılıyor...".to_string());
    }

    pub fn spawn_job(
        &mut self,
        ctx: &egui::Context,
        tab: Tab,
        f: impl FnOnce(JobCtx) -> anyhow::Result<()> + Send + 'static,
    ) {
        self.set_running(tab);
        self.bus.spawn(ctx.clone(), tab, f);
    }

    pub fn progress_ui(&self, ui: &mut egui::Ui, tab: Tab) {
        let value = *self.progress.get(&tab).unwrap_or(&0.0);
        let text = self
            .progress_text
            .get(&tab)
            .cloned()
            .unwrap_or_else(|| "Hazır".to_string());
        let running = self.is_running(tab);
        if !running && value <= 0.001 {
            return;
        }
        ui.add(
            egui::ProgressBar::new(value)
                .animate(running)
                .show_percentage()
                .text(text),
        );
    }

    pub fn status_label(&self, ui: &mut egui::Ui, tab: Tab) {
        let text = self.status.get(&tab).cloned().unwrap_or_default();
        if !text.is_empty() {
            ui.label(RichText::new(text).size(12.0).color(MUTED));
        }
    }

    fn drain_jobs(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.bus.rx.try_recv() {
            match msg {
                JobMessage::Progress { tab, value, text } => {
                    self.progress.insert(tab, value);
                    self.progress_text.insert(tab, text);
                }
                JobMessage::Row { tab, index, status } => {
                    let rows = match tab {
                        Tab::Convert => Some(&mut self.convert.rows),
                        Tab::Docx => Some(&mut self.docx.rows),
                        Tab::Udf => Some(&mut self.udf.rows),
                        _ => None,
                    };
                    if let Some(rows) = rows {
                        if let Some(row) = rows.get_mut(index) {
                            row.status = status;
                        }
                    }
                }
                JobMessage::Done { tab, text, message } => {
                    self.running.remove(&tab);
                    self.progress.insert(tab, 1.0);
                    self.progress_text.insert(tab, text.clone());
                    self.status.insert(tab, text.clone());
                    if !message.is_empty() {
                        self.alert("İşlem Tamamlandı", message);
                    }
                }
                JobMessage::Failed { tab, error } => {
                    self.running.remove(&tab);
                    self.progress.insert(tab, 0.0);
                    self.progress_text
                        .insert(tab, "İşlem başarısız".to_string());
                    self.status
                        .insert(tab, error.lines().next().unwrap_or("Hata").to_string());
                    self.error("Hata", error);
                }
                JobMessage::Preview {
                    generation,
                    page,
                    image,
                } => {
                    if generation == self.edit.generation && page == self.edit.page {
                        let color = egui::ColorImage::from_rgba_unmultiplied(
                            [image.width as usize, image.height as usize],
                            &image.rgba,
                        );
                        let handle = ctx.load_texture(
                            format!("edit_page_{generation}_{page}"),
                            color,
                            egui::TextureOptions::LINEAR,
                        );
                        self.edit.texture = Some(handle);
                        self.edit.rendered = Some(image);
                    }
                }
            }
        }
    }

    pub fn add_merge_files(&mut self, paths: &[PathBuf]) {
        let mut errors = Vec::new();
        for path in paths {
            if crate::util::extension(path) != "pdf" {
                continue;
            }
            match std::fs::read(path) {
                Ok(bytes) => match crate::pdfcore::page_count(&bytes) {
                    Ok(total) => self
                        .merge
                        .files
                        .push(PdfFileEntry::new(path.clone(), total)),
                    Err(e) => errors.push(format!("{}: {e:#}", crate::util::file_name(path))),
                },
                Err(e) => errors.push(format!("{}: {e}", crate::util::file_name(path))),
            }
        }
        if !errors.is_empty() {
            self.error("Bazı PDF'ler eklenemedi", errors.join("\n"));
        }
    }

    pub fn open_split(&mut self, ctx: &egui::Context, path: PathBuf) {
        match std::fs::read(&path) {
            Ok(bytes) => match crate::pdfcore::page_count(&bytes) {
                Ok(total) => {
                    self.split.file = Some(PdfFileEntry::new(path, total));
                    self.split.bytes = bytes;
                    self.split.ranges = format!("1-{total}");
                    self.note(Tab::Split, format!("{total} sayfa yüklendi."));
                    ctx.request_repaint();
                }
                Err(e) => self.error("PDF açılamadı", format!("{e:#}")),
            },
            Err(e) => self.error("Dosya okunamadı", format!("{e}")),
        }
    }

    pub fn open_pageno(&mut self, path: PathBuf) {
        match std::fs::read(&path) {
            Ok(bytes) => match crate::pdfcore::page_count(&bytes) {
                Ok(total) => {
                    self.pageno.path = Some(path);
                    self.pageno.bytes = bytes;
                    self.pageno.total = total;
                    self.note(Tab::PageNo, format!("{total} sayfa yüklendi."));
                }
                Err(e) => self.error("PDF açılamadı", format!("{e:#}")),
            },
            Err(e) => self.error("Dosya okunamadı", format!("{e}")),
        }
    }

    pub fn open_export(&mut self, path: PathBuf) {
        match std::fs::read(&path) {
            Ok(bytes) => match crate::pdfcore::page_count(&bytes) {
                Ok(total) => {
                    self.export.path = Some(path);
                    self.export.bytes = bytes;
                    self.export.total = total;
                    self.export.selected = (1..=total).collect();
                    self.export.ranges = format!("1-{total}");
                    self.note(Tab::Export, format!("{total} sayfa yüklendi."));
                }
                Err(e) => self.error("PDF açılamadı", format!("{e:#}")),
            },
            Err(e) => self.error("Dosya okunamadı", format!("{e}")),
        }
    }

    pub fn open_print(&mut self, path: PathBuf) {
        match std::fs::read(&path) {
            Ok(bytes) => match crate::pdfcore::page_count(&bytes) {
                Ok(total) => {
                    self.print.path = Some(path);
                    self.print.bytes = bytes;
                    self.print.total = total;
                    self.print.selected = (1..=total).collect();
                    self.print.ranges = format!("1-{total}");
                    if !self.print.printers_loaded {
                        self.refresh_printers();
                    }
                    self.note(Tab::Print, format!("{total} sayfa yüklendi."));
                }
                Err(e) => self.error("PDF açılamadı", format!("{e:#}")),
            },
            Err(e) => self.error("Dosya okunamadı", format!("{e}")),
        }
    }

    /// Kurulu yazıcıları yeniler ve varsayılan yazıcıyı seçer.
    pub fn refresh_printers(&mut self) {
        let printers = crate::print::list_printers();
        if self.print.printer.is_none() {
            // Varsayılan yazıcı tanımlı değilse ilk yazıcıyı seç.
            self.print.printer = printers
                .iter()
                .find(|p| p.is_default)
                .or_else(|| printers.first())
                .map(|p| p.name.clone());
        }
        if let Some(chosen) = &self.print.printer {
            if !printers.iter().any(|p| &p.name == chosen) {
                self.print.printer = printers
                    .iter()
                    .find(|p| p.is_default)
                    .or_else(|| printers.first())
                    .map(|p| p.name.clone());
            }
        }
        self.print.printers = printers;
        self.print.printers_loaded = true;
    }

    pub fn open_edit(&mut self, ctx: &egui::Context, path: PathBuf) {
        match std::fs::read(&path) {
            Ok(bytes) => match crate::pdfcore::page_geometries(&bytes) {
                Ok(geoms) => {
                    if geoms.is_empty() {
                        self.error("PDF açılamadı", "PDF içinde sayfa bulunamadı.");
                        return;
                    }
                    self.edit.path = Some(path);
                    self.edit.bytes = bytes;
                    self.edit.total = geoms.len() as u32;
                    self.edit.page = 1;
                    self.edit.actions.clear();
                    self.edit.cursor = 0;
                    self.edit.texture = None;
                    self.edit.selected = None;
                    self.edit.zoom = 1.0;
                    self.edit.fit = true;
                    self.edit.preview_scale = 2.0;
                    self.edit.view_scale = 1.0;
                    self.edit.zoom_changed_at = 0.0;
                    self.edit.center_view = true;
                    self.edit.scroll = egui::Vec2::ZERO;
                    self.note(Tab::Edit, format!("{} sayfa yüklendi.", self.edit.total));
                    self.reload_edit_page(ctx);
                }
                Err(e) => self.error("PDF açılamadı", format!("{e:#}")),
            },
            Err(e) => self.error("Dosya okunamadı", format!("{e}")),
        }
    }

    pub fn reload_edit_page(&mut self, ctx: &egui::Context) {
        let edit = &mut self.edit;
        if edit.bytes.is_empty() || edit.page == 0 {
            return;
        }
        edit.generation += 1;
        edit.texture = None;
        edit.rendered = None;
        let index = (edit.page - 1) as usize;
        let geoms = match crate::pdfcore::page_geometries(&edit.bytes) {
            Ok(g) => g,
            Err(e) => {
                self.error("PDF açılamadı", format!("{e:#}"));
                return;
            }
        };
        let Some(geo) = geoms.get(index) else {
            return;
        };
        edit.page_w = geo.width;
        edit.page_h = geo.height;
        edit.preview_scale = (edit.view_scale * ctx.pixels_per_point())
            .max(1.0)
            .min(preview_scale_limit(edit.page_w, edit.page_h));
        edit.center_view = true;
        edit.text_objects =
            crate::textobj::extract_text_objects(&edit.bytes, index).unwrap_or_default();
        let scale = edit.preview_scale;
        self.bus.spawn_preview(
            ctx.clone(),
            edit.bytes.clone(),
            index,
            edit.page,
            edit.generation,
            scale,
        );
    }

    pub fn handle_dropped(&mut self, ctx: &egui::Context, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        match self.tab {
            Tab::Merge => self.add_merge_files(&paths),
            Tab::Split => {
                if let Some(p) = paths.iter().find(|p| crate::util::extension(p) == "pdf") {
                    self.open_split(ctx, p.clone());
                }
            }
            Tab::Convert => {
                for p in paths {
                    self.convert.rows.push(crate::model::FileRow::new(p));
                }
            }
            Tab::Docx => {
                for p in paths {
                    self.docx.rows.push(crate::model::FileRow::new(p));
                }
            }
            Tab::Udf => {
                for p in paths {
                    self.udf.rows.push(crate::model::FileRow::new(p));
                }
            }
            Tab::Edit => {
                if let Some(p) = paths.iter().find(|p| crate::util::extension(p) == "pdf") {
                    self.open_edit(ctx, p.clone());
                }
            }
            Tab::PageNo => {
                if let Some(p) = paths.iter().find(|p| crate::util::extension(p) == "pdf") {
                    self.open_pageno(p.clone());
                }
            }
            Tab::Ocr => {
                if let Some(p) = paths.iter().find(|p| crate::util::extension(p) == "pdf") {
                    self.ocr.path = Some(p.clone());
                    self.note(Tab::Ocr, crate::util::file_name(p));
                }
            }
            Tab::Export => {
                if let Some(p) = paths.iter().find(|p| crate::util::extension(p) == "pdf") {
                    self.open_export(p.clone());
                }
            }
            Tab::Print => {
                if let Some(p) = paths.iter().find(|p| crate::util::extension(p) == "pdf") {
                    self.open_print(p.clone());
                }
            }
        }
    }

    fn run_confirm(&mut self, action: ConfirmAction) {
        match action {
            ConfirmAction::RemoveMerge(i) => {
                if i < self.merge.files.len() {
                    self.merge.files.remove(i);
                }
            }
            ConfirmAction::RemoveConvert(i) => {
                if i < self.convert.rows.len() {
                    self.convert.rows.remove(i);
                }
            }
            ConfirmAction::RemoveDocx(i) => {
                if i < self.docx.rows.len() {
                    self.docx.rows.remove(i);
                }
            }
            ConfirmAction::RemoveUdf(i) => {
                if i < self.udf.rows.len() {
                    self.udf.rows.remove(i);
                }
            }
            ConfirmAction::ClearEditActions => {
                self.edit.actions.clear();
                self.edit.cursor = 0;
                self.edit.selected = None;
            }
        }
    }

    fn header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("app_header")
            .frame(
                Frame::new()
                    .fill(NAVY)
                    .inner_margin(Margin::symmetric(22, 13)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    self.draw_icon(ui, 42.0);
                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("PDF Toolbox")
                                .size(21.0)
                                .strong()
                                .color(Color32::WHITE),
                        );
                        ui.label(
                            RichText::new(
                                "Birleştir • Böl • Dönüştür • Düzenle • Yazdır • OCR • Sayfa Numarası",
                            )
                            .size(11.0)
                            .color(Color32::from_rgb(186, 200, 230)),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.vertical(|ui| {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new(DEVELOPER)
                                        .size(12.0)
                                        .strong()
                                        .color(Color32::WHITE),
                                );
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new("Sürüm 1.0.0")
                                        .size(10.5)
                                        .color(Color32::from_rgb(186, 200, 230)),
                                );
                            });
                        });
                    });
                });
            });
    }

    fn tab_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("app_tabs")
            .frame(
                Frame::new()
                    .fill(Color32::WHITE)
                    .inner_margin(Margin::symmetric(16, 8)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::horizontal()
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for tab in Tab::ALL {
                                let selected = self.tab == tab;
                                let color = tab_color(tab);
                                let button = egui::Button::new(
                                    RichText::new(tab.title())
                                        .size(12.5)
                                        .color(if selected { Color32::WHITE } else { INK })
                                        .strong(),
                                )
                                .fill(if selected {
                                    color
                                } else {
                                    Color32::from_rgb(248, 250, 252)
                                })
                                .stroke(Stroke::new(
                                    1.0_f32,
                                    if selected {
                                        color
                                    } else {
                                        Color32::from_rgb(226, 232, 240)
                                    },
                                ))
                                .min_size(Vec2::new(0.0, 33.0))
                                .corner_radius(8.0);
                                if ui.add(button).clicked() {
                                    self.tab = tab;
                                }
                            }
                        });
                    });
            });
    }

    fn footer(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("app_footer")
            .frame(
                Frame::new()
                    .fill(Color32::WHITE)
                    .stroke(Stroke::new(1.0_f32, BORDER))
                    .inner_margin(Margin::symmetric(18, 6)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let tab = self.tab;
                    let status = self.status.get(&tab).cloned().unwrap_or_default();
                    ui.label(RichText::new(status).size(11.0).color(MUTED));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(DEVELOPER).size(11.0).strong().color(NAVY_2));
                    });
                });
            });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.page_dialog.is_some() {
            let mut accept = false;
            let mut cancel = false;
            {
                let dialog = self.page_dialog.as_mut().unwrap();
                egui::Modal::new(egui::Id::new("page_dialog"))
                    .frame(
                        Frame::new()
                            .fill(CARD)
                            .corner_radius(CornerRadius::same(12))
                            .inner_margin(Margin::same(18)),
                    )
                    .show(ctx, |ui| {
                        ui.set_width(440.0);
                        ui.label(RichText::new("Sayfa Seçimi").size(17.0).strong().color(INK));
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("Toplam {} sayfa", dialog.total))
                                    .size(12.0)
                                    .color(MUTED),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.small_button("Tümü").clicked() {
                                    for s in &mut dialog.selected {
                                        *s = true;
                                    }
                                }
                                if ui.small_button("Temizle").clicked() {
                                    for s in &mut dialog.selected {
                                        *s = false;
                                    }
                                }
                            });
                        });
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Aralık:").size(12.0).color(MUTED));
                            ui.add(
                                egui::TextEdit::singleline(&mut dialog.ranges)
                                    .hint_text("1-5, 8, 10-12")
                                    .desired_width(220.0),
                            );
                            if ui.button("Uygula").clicked() {
                                match crate::util::parse_ranges(&dialog.ranges, dialog.total) {
                                    Ok(pages) => {
                                        for s in &mut dialog.selected {
                                            *s = false;
                                        }
                                        for p in pages {
                                            if let Some(s) = dialog.selected.get_mut(p as usize - 1)
                                            {
                                                *s = true;
                                            }
                                        }
                                        dialog.error = None;
                                    }
                                    Err(e) => dialog.error = Some(e),
                                }
                            }
                        });
                        ui.add_space(4.0);
                        egui::ScrollArea::vertical()
                            .max_height(220.0)
                            .show(ui, |ui| {
                                let per_row = 8;
                                for chunk in 0..(dialog.total as usize).div_ceil(per_row) {
                                    ui.horizontal(|ui| {
                                        for i in 0..per_row {
                                            let idx = chunk * per_row + i;
                                            if idx >= dialog.selected.len() {
                                                break;
                                            }
                                            let checked = &mut dialog.selected[idx];
                                            ui.add_sized(
                                                Vec2::new(48.0, 24.0),
                                                egui::Checkbox::new(
                                                    checked,
                                                    format!("{}", idx + 1),
                                                ),
                                            );
                                        }
                                    });
                                }
                            });
                        if let Some(e) = &dialog.error {
                            ui.label(RichText::new(e).size(12.0).color(RED));
                        }
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if action_button(ui, "Tamam", GREEN).clicked() {
                                accept = true;
                            }
                            if ghost_button(ui, "Vazgeç").clicked() {
                                cancel = true;
                            }
                        });
                    });
            }
            if cancel {
                self.page_dialog = None;
            } else if accept {
                let (selected, target) = {
                    let dialog = self.page_dialog.as_ref().unwrap();
                    let selected: Vec<u32> = dialog
                        .selected
                        .iter()
                        .enumerate()
                        .filter_map(|(i, s)| if *s { Some(i as u32 + 1) } else { None })
                        .collect();
                    (selected, dialog.target)
                };
                if selected.is_empty() {
                    if let Some(dialog) = &mut self.page_dialog {
                        dialog.error = Some("En az bir sayfa seçmelisiniz.".to_string());
                    }
                } else {
                    match target {
                        PageTarget::Merge(i) => {
                            if let Some(file) = self.merge.files.get_mut(i) {
                                file.selected = selected;
                            }
                        }
                        PageTarget::Split => {
                            self.split.ranges = crate::util::format_ranges(&selected);
                        }
                        PageTarget::Export => {
                            self.export.selected = selected.clone();
                            self.export.ranges = crate::util::format_ranges(&selected);
                        }
                        PageTarget::Print => {
                            self.print.selected = selected.clone();
                            self.print.ranges = crate::util::format_ranges(&selected);
                        }
                    }
                    self.page_dialog = None;
                }
            }
        }

        if let Some(message) = &self.message {
            let mut close = false;
            egui::Modal::new(egui::Id::new("message_dialog"))
                .frame(
                    Frame::new()
                        .fill(CARD)
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(Margin::same(18)),
                )
                .show(ctx, |ui| {
                    ui.set_width(430.0);
                    let color = if message.error { RED } else { GREEN };
                    ui.label(
                        RichText::new(&message.title)
                            .size(17.0)
                            .strong()
                            .color(color),
                    );
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .max_height(330.0)
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(&message.text)
                                    .size(12.5)
                                    .color(INK)
                                    .family(FontFamily::Proportional),
                            );
                        });
                    ui.add_space(8.0);
                    if action_button(ui, "Tamam", color).clicked() {
                        close = true;
                    }
                });
            if close {
                self.message = None;
            }
        }

        if let Some(confirm) = &self.confirm {
            let mut yes = false;
            let mut no = false;
            egui::Modal::new(egui::Id::new("confirm_dialog"))
                .frame(
                    Frame::new()
                        .fill(CARD)
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(Margin::same(18)),
                )
                .show(ctx, |ui| {
                    ui.set_width(400.0);
                    ui.label(
                        RichText::new(&confirm.title)
                            .size(16.0)
                            .strong()
                            .color(NAVY_2),
                    );
                    ui.add_space(6.0);
                    ui.label(RichText::new(&confirm.text).size(12.5).color(INK));
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if action_button(ui, "Evet", RED).clicked() {
                            yes = true;
                        }
                        if ghost_button(ui, "Vazgeç").clicked() {
                            no = true;
                        }
                    });
                });
            if yes {
                if let Some(confirm) = self.confirm.take() {
                    self.run_confirm(confirm.action);
                }
            } else if no {
                self.confirm = None;
            }
        }
    }
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mut loaded = false;
    let candidates: [(&str, &[&str]); 3] = [
        (
            "ui_sans",
            &["segoeui.ttf", "arial.ttf", "calibri.ttf", "DejaVuSans.ttf"],
        ),
        (
            "ui_bold",
            &[
                "segoeuib.ttf",
                "arialbd.ttf",
                "calibrib.ttf",
                "DejaVuSans-Bold.ttf",
            ],
        ),
        ("ui_serif", &["times.ttf", "georgia.ttf", "DejaVuSerif.ttf"]),
    ];
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(windir) = std::env::var("WINDIR") {
        dirs.push(PathBuf::from(windir).join("Fonts"));
    }
    dirs.push(PathBuf::from("/usr/share/fonts/truetype/dejavu"));
    dirs.push(PathBuf::from("/Library/Fonts"));
    for (name, files) in candidates {
        for dir in &dirs {
            let mut done = false;
            for file in files {
                if let Ok(bytes) = std::fs::read(dir.join(file)) {
                    fonts.font_data.insert(
                        name.to_string(),
                        Arc::new(egui::FontData::from_owned(bytes)),
                    );
                    match name {
                        "ui_sans" => {
                            fonts
                                .families
                                .entry(FontFamily::Proportional)
                                .or_default()
                                .insert(0, name.to_string());
                        }
                        "ui_bold" => {
                            let proportional =
                                fonts.families.entry(FontFamily::Proportional).or_default();
                            if !proportional.iter().any(|f| f == name) {
                                proportional.push(name.to_string());
                            }
                            fonts
                                .families
                                .entry(FontFamily::Name("bold".into()))
                                .or_default()
                                .push(name.to_string());
                        }
                        _ => {
                            fonts
                                .families
                                .entry(FontFamily::Name("serif".into()))
                                .or_default()
                                .push(name.to_string());
                        }
                    }
                    loaded = true;
                    done = true;
                    break;
                }
            }
            if done {
                break;
            }
        }
    }
    if loaded {
        ctx.set_fonts(fonts);
    }
}

impl eframe::App for PdfToolboxApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_jobs(ctx);

        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if !dropped.is_empty() {
            self.handle_dropped(ctx, dropped);
        }

        self.header(ctx);
        self.tab_bar(ctx);
        self.footer(ctx);

        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(BG)
                    .inner_margin(Margin::symmetric(16, 14)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .id_salt("main_scroll")
                    .drag_to_scroll(false)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match self.tab {
                            Tab::Merge => self.ui_merge(ui),
                            Tab::Split => self.ui_split(ui, ctx),
                            Tab::Convert => self.ui_convert(ui),
                            Tab::Docx => self.ui_docx(ui),
                            Tab::Edit => self.ui_edit(ui, ctx),
                            Tab::Udf => self.ui_udf(ui),
                            Tab::PageNo => self.ui_pageno(ui),
                            Tab::Ocr => self.ui_ocr(ui),
                            Tab::Export => self.ui_export(ui),
                            Tab::Print => self.ui_print(ui),
                        }
                    });
            });

        self.dialogs(ctx);
        self.edit_dialogs(ctx);
    }
}

/// Önizleme dokusunun üst sınırı: aşırı büyük GPU dokularını ve belleği engeller.
pub fn preview_scale_limit(pw: f32, ph: f32) -> f32 {
    const MAX_EDGE: f32 = 7000.0;
    const MAX_PIXELS: f32 = 28_000_000.0;
    let w = pw.max(1.0);
    let h = ph.max(1.0);
    let edge = MAX_EDGE / w.max(h);
    let pixels = (MAX_PIXELS / (w * h)).sqrt();
    edge.min(pixels).clamp(0.5, 8.0)
}

pub fn pick_files(title: &str, filter: &str, exts: &[&str]) -> Vec<PathBuf> {
    let mut dialog = rfd::FileDialog::new().set_title(title);
    if !exts.is_empty() {
        dialog = dialog.add_filter(filter, exts);
    }
    dialog.pick_files().unwrap_or_default()
}

pub fn pick_file(title: &str, filter: &str, exts: &[&str]) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new().set_title(title);
    if !exts.is_empty() {
        dialog = dialog.add_filter(filter, exts);
    }
    dialog.pick_file()
}

pub fn pick_folder(title: &str) -> Option<PathBuf> {
    rfd::FileDialog::new().set_title(title).pick_folder()
}

pub fn dir_of(path: &Path) -> PathBuf {
    path.parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn output_dir(chosen: &Option<PathBuf>, source: Option<&Path>) -> PathBuf {
    if let Some(d) = chosen {
        return d.clone();
    }
    source.map(dir_of).unwrap_or_else(|| PathBuf::from("."))
}
