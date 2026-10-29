#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod app_convert;
mod app_edit;
mod app_merge;
mod app_tools;
mod convert;
mod export;
mod fonts;
mod icon;
mod jobs;
mod model;
mod ocr;
mod ooxml;
mod overlay;
mod pdfcore;
mod print;
mod textobj;
mod udf;
mod util;

#[cfg(test)]
mod smoke;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1120.0, 620.0])
        .with_min_inner_size([900.0, 600.0])
        .with_title("PDF Toolbox");
    if let Some(icon) = icon::window_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "PDF Toolbox",
        options,
        Box::new(|cc| Ok(Box::new(app::PdfToolboxApp::new(cc)))),
    )
}
