use anyhow::{anyhow, Context, Result};
use image::{GrayImage, RgbaImage};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::model::OcrMode;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const OCR_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);

fn command(program: &Path) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

pub fn find_tesseract(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        if p.is_file() {
            return Some(p.to_path_buf());
        }
    }
    if let Ok(p) = std::env::var("TESSERACT") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Ok(out) = crate::util::run_captured(
        command(Path::new("where")).arg("tesseract.exe"),
        PROBE_TIMEOUT,
    ) {
        if out.status.success() {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let p = PathBuf::from(line.trim());
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
        if let Ok(root) = std::env::var(var) {
            candidates.push(
                PathBuf::from(root)
                    .join("Tesseract-OCR")
                    .join("tesseract.exe"),
            );
        }
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        candidates.push(
            PathBuf::from(local)
                .join("Programs")
                .join("Tesseract-OCR")
                .join("tesseract.exe"),
        );
    }
    candidates.push(PathBuf::from(
        r"C:\Program Files\Tesseract-OCR\tesseract.exe",
    ));
    candidates.push(PathBuf::from("/usr/bin/tesseract"));
    candidates.push(PathBuf::from("/usr/local/bin/tesseract"));
    candidates.push(PathBuf::from("/opt/homebrew/bin/tesseract"));
    candidates.into_iter().find(|p| p.is_file())
}

fn list_languages(tess: &Path) -> Result<Vec<String>> {
    let out = crate::util::run_captured(command(tess).arg("--list-langs"), PROBE_TIMEOUT)
        .context("Tesseract çalıştırılamadı")?;
    if out.timed_out {
        return Err(anyhow!("Tesseract dil listesi zaman aşımına uğradı."));
    }
    if !out.status.success() {
        return Err(anyhow!(
            "Tesseract dil listesi alınamadı: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.to_ascii_lowercase().starts_with("list of"))
        .collect())
}

pub fn check_language(tess: &Path, lang: &str) -> Result<()> {
    let available = match list_languages(tess) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let missing: Vec<&str> = lang
        .split('+')
        .filter(|l| !l.is_empty() && !available.iter().any(|a| a == l))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(
            "Tesseract dil paketi eksik: {}\n\nTesseract kurulumunda ilgili dili seçin veya \
             {}.traineddata dosyasını tessdata klasörüne kopyalayın.",
            missing.join(", "),
            missing.join(".traineddata, ")
        ))
    }
}

fn run_capture(cmd: &mut Command, what: &str) -> Result<(Vec<u8>, Vec<u8>)> {
    let out = crate::util::run_captured(cmd, OCR_TIMEOUT)
        .with_context(|| format!("{what} başlatılamadı"))?;
    if out.timed_out {
        return Err(anyhow!(
            "{what} zaman aşımına uğradı ({} sn). Sayfa çok büyük veya çözünürlük çok yüksek olabilir.",
            OCR_TIMEOUT.as_secs()
        ));
    }
    Ok((out.stdout, out.stderr))
}

pub fn ocr_pdf(
    src: &Path,
    out: &Path,
    mode: OcrMode,
    lang: &str,
    dpi: u32,
    tesseract: Option<&Path>,
    mut progress: impl FnMut(u32, u32),
) -> Result<u32> {
    let tess = find_tesseract(tesseract).ok_or_else(|| {
        anyhow!(
            "Tesseract-OCR bulunamadı.\n\nWindows kurulumu:\n\
             https://github.com/UB-Mannheim/tesseract/wiki\n\
             Kurulumda 'Turkish' dil paketini de işaretleyin.\n\
             Kurulu ise 'Tesseract Yolu' düğmesiyle tesseract.exe dosyasını seçin."
        )
    })?;
    check_language(&tess, lang)?;

    let bytes = std::fs::read(src).with_context(|| format!("PDF okunamadı: {}", src.display()))?;
    let geometries = crate::pdfcore::page_geometries(&bytes)
        .map_err(|e| anyhow!("PDF açılamadı (şifreli veya bozuk olabilir): {e}"))?;
    let total = geometries.len() as u32;
    if total == 0 {
        return Err(anyhow!("PDF içinde sayfa bulunamadı."));
    }
    let tmp = tempfile::tempdir().context("Geçici klasör oluşturulamadı")?;

    if mode == OcrMode::Searchable {
        let mut page_pdfs: Vec<Vec<u8>> = Vec::with_capacity(total as usize);
        for (i, geo) in geometries.iter().enumerate() {
            let index = i as u32;
            let max_edge = geo.width.max(geo.height).max(1.0);
            let mut scale = dpi as f32 / 72.0;
            if max_edge * scale > 7000.0 {
                scale = 7000.0 / max_edge;
            }
            let effective_dpi = (scale * 72.0).round().clamp(70.0, 2400.0);
            let rendered = crate::pdfcore::render_page(&bytes, i, scale)?;
            let mut rgba = RgbaImage::from_raw(rendered.width, rendered.height, rendered.rgba)
                .ok_or_else(|| anyhow!("Sayfa görüntüsü oluşturulamadı."))?;
            let mut gray = grayscale_contrast(&rgba);
            orient(&tess, &mut rgba, &mut gray);

            let color_png = tmp.path().join(format!("page_{index}_color.png"));
            rgba.save(&color_png)?;
            let gray_png = tmp.path().join(format!("page_{index}_gray.png"));
            gray.save(&gray_png)?;
            let out_base = tmp.path().join(format!("page_{index}"));
            let mut cmd = command(&tess);
            cmd.arg(&color_png)
                .arg(&out_base)
                .arg("-l")
                .arg(lang)
                .arg("--dpi")
                .arg(format!("{effective_dpi:.0}"))
                .arg("--psm")
                .arg("3")
                .arg("pdf");
            let (stdout, stderr) = run_capture(&mut cmd, "Tesseract OCR")?;
            let pdf_path = out_base.with_extension("pdf");
            if pdf_path.is_file() {
                page_pdfs.push(std::fs::read(&pdf_path)?);
            } else {
                // OCR başarısız olsa da sayfayı görüntü olarak koru.
                let text = format!(
                    "{}{}",
                    String::from_utf8_lossy(&stdout),
                    String::from_utf8_lossy(&stderr)
                );
                if text.to_ascii_lowercase().contains("failed") {
                    eprintln!("OCR uyarısı (sayfa {}): {}", i + 1, text.trim());
                }
                let image_pdf = crate::overlay::build_image_pdf(&color_png, effective_dpi)?;
                page_pdfs.push(image_pdf);
            }
            progress(index + 1, total);
        }
        let items: Vec<(Vec<u8>, Vec<u32>)> = page_pdfs.into_iter().map(|b| (b, vec![1])).collect();
        let merged = crate::pdfcore::merge_page_sets_to_bytes(&items)?;
        std::fs::write(out, merged)?;
    } else {
        let mut texts: Vec<String> = Vec::with_capacity(total as usize);
        for (i, geo) in geometries.iter().enumerate() {
            let index = i as u32;
            let max_edge = geo.width.max(geo.height).max(1.0);
            let mut scale = dpi as f32 / 72.0;
            if max_edge * scale > 7000.0 {
                scale = 7000.0 / max_edge;
            }
            let effective_dpi = (scale * 72.0).round().clamp(70.0, 2400.0);
            let rendered = crate::pdfcore::render_page(&bytes, i, scale)?;
            let mut rgba = RgbaImage::from_raw(rendered.width, rendered.height, rendered.rgba)
                .ok_or_else(|| anyhow!("Sayfa görüntüsü oluşturulamadı."))?;
            let mut gray = grayscale_contrast(&rgba);
            orient(&tess, &mut rgba, &mut gray);
            let gray_png = tmp.path().join(format!("text_{index}.png"));
            gray.save(&gray_png)?;

            let mut best = String::new();
            let mut first_error: Option<String> = None;
            for psm in [3u32, 6, 4, 11] {
                let mut cmd = command(&tess);
                cmd.arg(&gray_png)
                    .arg("stdout")
                    .arg("-l")
                    .arg(lang)
                    .arg("--dpi")
                    .arg(format!("{effective_dpi:.0}"))
                    .arg("--psm")
                    .arg(psm.to_string());
                match run_capture(&mut cmd, "Tesseract OCR") {
                    Ok((stdout, _)) => {
                        let text = String::from_utf8_lossy(&stdout).to_string();
                        if text.trim().len() > best.trim().len() {
                            best = text;
                        }
                        if best.trim().len() >= 20 {
                            break;
                        }
                    }
                    Err(e) => {
                        if psm == 3 {
                            first_error = Some(format!("{e:#}"));
                            break;
                        }
                    }
                }
            }
            if best.trim().is_empty() {
                if let Some(err) = first_error {
                    return Err(anyhow!("OCR hatası: {err}"));
                }
            }
            texts.push(best);
            progress(index + 1, total);
        }
        if !texts.iter().any(|t| !t.trim().is_empty()) {
            return Err(anyhow!(
                "Hiçbir sayfada metin tanınamadı.\n\n\
                 • DPI değerini 300 veya 400 yapıp tekrar deneyin.\n\
                 • Doğru dili seçtiğinizden emin olun (Türkçe paketi kurulu mu?).\n\
                 • Tarama çok silik/eğik/düşük çözünürlüklü olabilir."
            ));
        }
        let fonts = crate::fonts::FontSet::load()?;
        let pdf = crate::overlay::build_text_pdf(&texts, &fonts)?;
        std::fs::write(out, pdf)?;
    }

    Ok(total)
}

fn grayscale_contrast(rgba: &RgbaImage) -> GrayImage {
    let (w, h) = rgba.dimensions();
    let mut hist = [0u64; 256];
    for px in rgba.pixels() {
        let [r, g, b, _] = px.0;
        let v = ((r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000) as u8;
        hist[v as usize] += 1;
    }
    let total: u64 = hist.iter().sum();
    let cutoff = (total as f64 * 0.01) as u64;
    let mut low = 0usize;
    let mut acc = 0u64;
    for (i, &c) in hist.iter().enumerate() {
        acc += c;
        if acc > cutoff {
            low = i;
            break;
        }
    }
    let mut high = 255usize;
    acc = 0;
    for (i, &c) in hist.iter().enumerate().rev() {
        acc += c;
        if acc > cutoff {
            high = i;
            break;
        }
    }
    if high <= low {
        low = 0;
        high = 255;
    }
    let range = (high - low).max(1) as f32;
    let mut out = GrayImage::new(w, h);
    for (x, y, px) in rgba.enumerate_pixels() {
        let [r, g, b, _] = px.0;
        let v = ((r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000) as f32;
        let mapped = (((v - low as f32) / range) * 255.0).clamp(0.0, 255.0) as u8;
        out.put_pixel(x, y, image::Luma([mapped]));
    }
    out
}

fn orient(tess: &Path, color: &mut RgbaImage, gray: &mut GrayImage) {
    let tmp = match tempfile::tempdir() {
        Ok(t) => t,
        Err(_) => return,
    };
    let path = tmp.path().join("osd.png");
    if gray.save(&path).is_err() {
        return;
    }
    let mut cmd = command(tess);
    cmd.arg(&path)
        .arg("stdout")
        .arg("--psm")
        .arg("0")
        .arg("-l")
        .arg("osd");
    let Ok((stdout, _)) = run_capture(&mut cmd, "Tesseract yön tespiti") else {
        return;
    };
    let text = String::from_utf8_lossy(&stdout);
    let mut rotate = 0i32;
    let mut confidence = 0f32;
    for line in text.lines() {
        if let Some(v) = line.trim().strip_prefix("Rotate:") {
            rotate = v.trim().parse().unwrap_or(0);
        }
        if let Some(v) = line.trim().strip_prefix("Orientation confidence:") {
            confidence = v.trim().parse().unwrap_or(0.0);
        }
    }
    if rotate == 0 || confidence < 2.0 {
        return;
    }
    let (c, g) = match rotate.rem_euclid(360) {
        90 => (
            image::imageops::rotate90(color),
            image::imageops::rotate90(gray),
        ),
        180 => (
            image::imageops::rotate180(color),
            image::imageops::rotate180(gray),
        ),
        270 => (
            image::imageops::rotate270(color),
            image::imageops::rotate270(gray),
        ),
        _ => return,
    };
    *color = c;
    *gray = g;
}
