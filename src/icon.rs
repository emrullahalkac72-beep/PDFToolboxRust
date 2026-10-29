//! Uygulama simgesi: pencere, görev çubuğu ve arayüz başlıkları.
//!
//! Simge `assets/app.ico` dosyasından derleme sırasında ikiliye gömülür; aynı
//! dosya `build.rs` tarafından exe kaynağına da eklenir.

use eframe::egui::{ColorImage, IconData};

/// Derlenmiş uygulamaya gömülen ICO verisi.
pub const ICO_BYTES: &[u8] = include_bytes!("../assets/app.ico");

fn decode() -> Option<image::DynamicImage> {
    image::load_from_memory_with_format(ICO_BYTES, image::ImageFormat::Ico).ok()
}

/// Pencere ve görev çubuğu simgesi (ICO içindeki en büyük kare).
pub fn window_icon() -> Option<IconData> {
    let rgba = decode()?.to_rgba8();
    Some(IconData {
        rgba: rgba.as_raw().clone(),
        width: rgba.width(),
        height: rgba.height(),
    })
}

/// Arayüzde çizilecek simge dokusu.
pub fn color_image(size: u32) -> Option<ColorImage> {
    let rgba = decode()?
        .resize_exact(size, size, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    Some(ColorImage::from_rgba_unmultiplied(
        [size as usize, size as usize],
        rgba.as_raw(),
    ))
}
