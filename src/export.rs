use anyhow::{anyhow, Context, Result};
use image::{ImageFormat, RgbImage, RgbaImage};
use std::io::Cursor;
use std::path::Path;

use crate::util;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Png,
    Jpg,
    Docx,
    Doc,
    Xlsx,
    Txt,
    Csv,
    Pptx,
}

impl ExportFormat {
    pub fn title(self) -> &'static str {
        match self {
            ExportFormat::Png => "PNG",
            ExportFormat::Jpg => "JPG",
            ExportFormat::Docx => "DOCX",
            ExportFormat::Doc => "DOC",
            ExportFormat::Xlsx => "XLSX",
            ExportFormat::Txt => "TXT",
            ExportFormat::Csv => "CSV",
            ExportFormat::Pptx => "PPTX",
        }
    }
    pub fn all() -> [ExportFormat; 8] {
        [
            ExportFormat::Png,
            ExportFormat::Jpg,
            ExportFormat::Docx,
            ExportFormat::Doc,
            ExportFormat::Xlsx,
            ExportFormat::Txt,
            ExportFormat::Csv,
            ExportFormat::Pptx,
        ]
    }
    pub fn ext(self) -> &'static str {
        match self {
            ExportFormat::Png => "png",
            ExportFormat::Jpg => "jpg",
            ExportFormat::Docx => "docx",
            ExportFormat::Doc => "doc",
            ExportFormat::Xlsx => "xlsx",
            ExportFormat::Txt => "txt",
            ExportFormat::Csv => "csv",
            ExportFormat::Pptx => "pptx",
        }
    }
}

pub fn export(
    pdf_path: &Path,
    bytes: &[u8],
    pages: &[u32],
    out_dir: &Path,
    fmt: ExportFormat,
    dpi: u32,
    merge_images: bool,
    soffice: Option<&Path>,
    mut progress: impl FnMut(u32, u32),
) -> Result<u32> {
    if pages.is_empty() {
        return Err(anyhow!("Sayfa seçilmedi."));
    }
    let mut pages = pages.to_vec();
    pages.sort_unstable();
    let base = util::stem(pdf_path);
    match fmt {
        ExportFormat::Png | ExportFormat::Jpg => export_images(
            bytes,
            &pages,
            out_dir,
            &base,
            fmt,
            dpi,
            merge_images,
            &mut progress,
        ),
        ExportFormat::Txt => {
            let text = pages_text(bytes, &pages, &mut progress)?;
            let path = out_dir.join(format!("{base}.txt"));
            std::fs::write(path, text.join("\n\n"))?;
            Ok(pages.len() as u32)
        }
        ExportFormat::Csv => {
            let texts = pages_text(bytes, &pages, &mut progress)?;
            let path = out_dir.join(format!("{base}.csv"));
            let mut out = String::new();
            for text in texts {
                for line in text.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    out.push_str(&util::csv_field(line));
                    out.push_str("\r\n");
                }
            }
            std::fs::write(path, out)?;
            Ok(pages.len() as u32)
        }
        ExportFormat::Xlsx => {
            let texts = pages_text(bytes, &pages, &mut progress)?;
            let sheets: Vec<(String, Vec<String>)> = pages
                .iter()
                .zip(texts.iter())
                .map(|(p, t)| {
                    (
                        format!("Sayfa {p}"),
                        t.lines().map(|l| l.to_string()).collect(),
                    )
                })
                .collect();
            let path = out_dir.join(format!("{base}.xlsx"));
            crate::ooxml::write_xlsx(&sheets, &path)?;
            Ok(pages.len() as u32)
        }
        ExportFormat::Docx => {
            let texts = pages_text(bytes, &pages, &mut progress)?;
            let path = out_dir.join(format!("{base}.docx"));
            crate::ooxml::write_docx(&texts.join("\n\n"), &path)?;
            Ok(pages.len() as u32)
        }
        ExportFormat::Pptx => {
            let mut images: Vec<Vec<u8>> = Vec::with_capacity(pages.len());
            let total = pages.len() as u32;
            for (i, p) in pages.iter().enumerate() {
                progress(i as u32 + 1, total);
                let rendered = crate::pdfcore::render_page(bytes, (*p - 1) as usize, 2.0)?;
                let img = RgbaImage::from_raw(rendered.width, rendered.height, rendered.rgba)
                    .ok_or_else(|| anyhow!("Sayfa görüntüsü oluşturulamadı."))?;
                let mut buf = Vec::new();
                image::DynamicImage::ImageRgba8(img)
                    .write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)?;
                images.push(buf);
            }
            let path = out_dir.join(format!("{base}.pptx"));
            crate::ooxml::write_pptx(&images, &path)?;
            Ok(pages.len() as u32)
        }
        ExportFormat::Doc => {
            let tmp = tempfile::tempdir()?;
            let selected = crate::pdfcore::extract_pages_to_bytes(bytes, &pages)?;
            let pdf_path = tmp.path().join(format!("{base}.pdf"));
            std::fs::write(&pdf_path, selected)?;
            for (i, _) in pages.iter().enumerate() {
                progress(i as u32 + 1, pages.len() as u32);
            }
            let produced = crate::convert::pdf_to_word(soffice, &pdf_path, out_dir, "doc")?;
            let target = out_dir.join(format!("{base}.doc"));
            if produced != target {
                std::fs::copy(&produced, &target)?;
                let _ = std::fs::remove_file(&produced);
            }
            Ok(pages.len() as u32)
        }
    }
}

fn pages_text(
    bytes: &[u8],
    pages: &[u32],
    progress: &mut impl FnMut(u32, u32),
) -> Result<Vec<String>> {
    let doc = crate::pdfcore::load_document(bytes)?;
    let total = pages.len() as u32;
    let mut out = Vec::with_capacity(pages.len());
    for (i, p) in pages.iter().enumerate() {
        let text = doc.extract_text(&[*p]).unwrap_or_default();
        out.push(text);
        progress(i as u32 + 1, total);
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
pub fn export_images(
    bytes: &[u8],
    pages: &[u32],
    out_dir: &Path,
    base: &str,
    fmt: ExportFormat,
    dpi: u32,
    merge_images: bool,
    progress: &mut impl FnMut(u32, u32),
) -> Result<u32> {
    let total = pages.len() as u32;
    let scale = dpi as f32 / 72.0;
    let mut rendered: Vec<RgbaImage> = Vec::with_capacity(pages.len());
    for (i, p) in pages.iter().enumerate() {
        let page = crate::pdfcore::render_page(bytes, (*p - 1) as usize, scale)?;
        let img = RgbaImage::from_raw(page.width, page.height, page.rgba)
            .ok_or_else(|| anyhow!("Sayfa görüntüsü oluşturulamadı."))?;
        rendered.push(img);
        progress(i as u32 + 1, total);
    }

    if merge_images && rendered.len() > 1 {
        let width = rendered.iter().map(|i| i.width()).max().unwrap_or(1);
        let height: u64 = rendered.iter().map(|i| i.height() as u64).sum();
        if height > 65_500 && fmt == ExportFormat::Jpg {
            return Err(anyhow!(
                "Birleşik görsel JPG sınırını (65500 piksel) aşıyor. DPI'ı düşürün, daha az sayfa seçin veya PNG kullanın."
            ));
        }
        if width as u64 * height > 400_000_000 {
            return Err(anyhow!(
                "Birleşik görsel çok büyük. DPI'ı düşürün veya daha az sayfa seçin."
            ));
        }
        let mut canvas =
            RgbaImage::from_pixel(width, height as u32, image::Rgba([255, 255, 255, 255]));
        let mut y = 0u32;
        for img in &rendered {
            let x = (width - img.width()) / 2;
            image::imageops::overlay(&mut canvas, img, x as i64, y as i64);
            y += img.height();
        }
        let path = out_dir.join(format!("{base}_birlesik.{}", fmt.ext()));
        save_image(&canvas, &path, fmt)?;
        progress(total, total);
        return Ok(total);
    }

    for (i, (p, img)) in pages.iter().zip(rendered.iter()).enumerate() {
        let path = out_dir.join(format!("{base}_p{p}.{}", fmt.ext()));
        save_image(img, &path, fmt)?;
        progress((i + 1) as u32, total);
    }
    Ok(total)
}

fn save_image(img: &RgbaImage, path: &Path, fmt: ExportFormat) -> Result<()> {
    match fmt {
        ExportFormat::Jpg => {
            let rgb: RgbImage = image::ImageBuffer::from_fn(img.width(), img.height(), |x, y| {
                let p = img.get_pixel(x, y);
                let a = p.0[3] as u32;
                let blend = |c: u8| -> u8 { ((c as u32 * a + 255 * (255 - a)) / 255) as u8 };
                image::Rgb([blend(p.0[0]), blend(p.0[1]), blend(p.0[2])])
            });
            let file = std::fs::File::create(path)
                .with_context(|| format!("Dosya oluşturulamadı: {}", path.display()))?;
            let mut writer = std::io::BufWriter::new(file);
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, 95);
            encoder.encode_image(&rgb)?;
            Ok(())
        }
        _ => img
            .save(path)
            .with_context(|| format!("Görsel kaydedilemedi: {}", path.display())),
    }
}

pub fn image_dims(path: &Path) -> Result<(u32, u32)> {
    let bytes = std::fs::read(path)?;
    let img = image::load_from_memory(&bytes)?;
    Ok((img.width(), img.height()))
}
