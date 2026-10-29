use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::util;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    Auto,
    Word,
    LibreOffice,
}

impl Engine {
    pub fn title(self) -> &'static str {
        match self {
            Engine::Auto => "Otomatik",
            Engine::Word => "Microsoft Word",
            Engine::LibreOffice => "LibreOffice",
        }
    }
    pub fn all() -> [Engine; 3] {
        [Engine::Auto, Engine::Word, Engine::LibreOffice]
    }
}

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const CONVERT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(240);

/// Dönüştürücü zaman aşımı. `PDF_TOOLBOX_CONVERT_TIMEOUT` ortam değişkeni ile
/// saniye cinsinden değiştirilebilir (testler ve çok büyük dosyalar için).
fn convert_timeout() -> std::time::Duration {
    std::env::var("PDF_TOOLBOX_CONVERT_TIMEOUT")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .map(std::time::Duration::from_secs)
        .unwrap_or(CONVERT_TIMEOUT)
}

fn command(program: &Path) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

pub fn find_soffice(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        if p.is_file() {
            return Some(p.to_path_buf());
        }
    }
    if let Ok(p) = std::env::var("SOFFICE") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Ok(out) = util::run_captured(
        command(Path::new("where")).arg("soffice.exe"),
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
                    .join("LibreOffice")
                    .join("program")
                    .join("soffice.exe"),
            );
        }
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        candidates.push(
            PathBuf::from(local)
                .join("Programs")
                .join("LibreOffice")
                .join("program")
                .join("soffice.exe"),
        );
    }
    candidates.push(PathBuf::from(
        r"C:\Program Files\LibreOffice\program\soffice.exe",
    ));
    candidates.push(PathBuf::from(
        r"C:\Program Files (x86)\LibreOffice\program\soffice.exe",
    ));
    candidates.push(PathBuf::from("/usr/bin/soffice"));
    candidates.push(PathBuf::from("/usr/local/bin/soffice"));
    candidates.push(PathBuf::from(
        "/Applications/LibreOffice.app/Contents/MacOS/soffice",
    ));
    candidates.into_iter().find(|p| p.is_file())
}

#[cfg(windows)]
pub fn word_available() -> bool {
    let out = util::run_captured(
        command(Path::new("reg")).args([
            "query",
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\winword.exe",
            "/ve",
        ]),
        PROBE_TIMEOUT,
    );
    out.map(|o| o.status.success()).unwrap_or(false)
}

#[cfg(not(windows))]
pub fn word_available() -> bool {
    false
}

#[cfg(windows)]
pub fn excel_available() -> bool {
    for key in [
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\excel.exe",
        r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\App Paths\excel.exe",
    ] {
        let ok = util::run_captured(
            command(Path::new("reg")).args(["query", key, "/ve"]),
            PROBE_TIMEOUT,
        )
        .map(|o| o.status.success())
        .unwrap_or(false);
        if ok {
            return true;
        }
    }
    false
}

#[cfg(not(windows))]
pub fn excel_available() -> bool {
    false
}

fn run(cmd: &mut Command, what: &str) -> Result<()> {
    let timeout = convert_timeout();
    let out = util::run_captured(cmd, timeout).with_context(|| format!("{what} başlatılamadı"))?;
    if out.timed_out {
        return Err(anyhow!(
            "{what} zaman aşımına uğradı ({} sn).\n\n\
             Dönüştürme iptal edildi. Dosya çok büyükse veya dönüştürücü \
             arka planda takıldıysa programı yeniden deneyin.",
            timeout.as_secs()
        ));
    }
    if !out.status.success() {
        let mut msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if msg.is_empty() {
            msg = String::from_utf8_lossy(&out.stdout).trim().to_string();
        }
        if msg.is_empty() {
            msg = format!("çıkış kodu {:?}", out.status.code());
        }
        return Err(anyhow!("{what} başarısız: {msg}"));
    }
    Ok(())
}

pub fn libreoffice_convert(
    soffice: &Path,
    input: &Path,
    out_dir: &Path,
    convert_to: &str,
    infilter: Option<&str>,
) -> Result<PathBuf> {
    std::fs::create_dir_all(out_dir)?;
    let profile = tempfile::tempdir().context("Geçici LibreOffice profili oluşturulamadı")?;
    let profile_uri = format!("-env:UserInstallation={}", path_to_file_uri(profile.path()));
    let mut cmd = command(soffice);
    cmd.arg("--headless")
        .arg("--norestore")
        .arg(profile_uri)
        .arg("--convert-to")
        .arg(convert_to)
        .arg("--outdir")
        .arg(out_dir);
    if let Some(f) = infilter {
        cmd.arg(format!("--infilter={f}"));
    }
    cmd.arg(input);
    run(&mut cmd, "LibreOffice dönüştürme")?;
    let ext = convert_to
        .split(':')
        .next()
        .unwrap_or(convert_to)
        .trim_start_matches('.');
    let expected = out_dir.join(format!("{}.{}", util::stem(input), ext));
    if expected.exists() {
        Ok(expected)
    } else {
        // LibreOffice bazen farklı bir ad üretir; klasördeki en yeni dosyayı dene.
        let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
        if let Ok(rd) = std::fs::read_dir(out_dir) {
            for entry in rd.flatten() {
                let p = entry.path();
                if util::extension(&p) == ext.to_ascii_lowercase() {
                    let t = entry
                        .metadata()
                        .and_then(|m| m.modified())
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                    if best.as_ref().map(|(bt, _)| t > *bt).unwrap_or(true) {
                        best = Some((t, p));
                    }
                }
            }
        }
        best.map(|(_, p)| p)
            .ok_or_else(|| anyhow!("LibreOffice çıktısı bulunamadı ({})", expected.display()))
    }
}

fn path_to_file_uri(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    if s.starts_with('/') {
        format!("file://{s}")
    } else {
        format!("file:///{s}")
    }
}

fn word_convert(input: &Path, out: &Path) -> Result<()> {
    let src = input.to_string_lossy().replace('\'', "''");
    let dst = out.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference='Stop'; \
         $w=New-Object -ComObject Word.Application; $w.Visible=$false; \
         try {{ $d=$w.Documents.Open('{src}',$false,$true); \
         $d.ExportAsFixedFormat('{dst}',17); $d.Close([ref]0) }} finally {{ $w.Quit() }}"
    );
    let mut cmd = command(Path::new("powershell.exe"));
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
    run(&mut cmd, "Microsoft Word dönüştürme")
}

fn convert_via_word(src: &Path, out: &Path) -> Result<()> {
    if !word_available() {
        return Err(anyhow!(
            "Microsoft Word bulunamadı. Word kurulu değilse LibreOffice motorunu seçin."
        ));
    }
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    word_convert(src, out)
}

/// Excel çalışma kitaplarını (XLSX / XLS / XLSM) COM otomasyonuyla PDF'e
/// aktarır. Excel kurulu değilse LibreOffice (Calc) kullanılır.
fn excel_convert(src: &Path, out: &Path) -> Result<()> {
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let src_escaped = src.to_string_lossy().replace('\'', "''");
    let out_escaped = out.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference='Stop'; \
         $x=New-Object -ComObject Excel.Application; \
         $x.Visible=$false; $x.DisplayAlerts=$false; \
         try {{ $wb=$x.Workbooks.Open('{src_escaped}',0,$true); \
         $wb.ExportAsFixedFormat(0,'{out_escaped}'); \
         $wb.Close($false) }} finally {{ $x.Quit() }}"
    );
    let mut cmd = command(Path::new("powershell.exe"));
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
    run(&mut cmd, "Microsoft Excel dönüştürme")
}

fn convert_via_excel(src: &Path, out: &Path) -> Result<()> {
    if !excel_available() {
        return Err(anyhow!(
            "Microsoft Excel bulunamadı. Excel kurulu değilse LibreOffice motorunu seçin."
        ));
    }
    excel_convert(src, out)
}

fn convert_via_libreoffice(src: &Path, out: &Path, soffice: Option<&Path>) -> Result<()> {
    let lo = soffice
        .map(|p| p.to_path_buf())
        .or_else(|| find_soffice(None))
        .ok_or_else(|| {
            anyhow!(
                "LibreOffice bulunamadı.\n\nKurulum: https://www.libreoffice.org/ \
                 veya DOCX → PDF sekmesinden soffice.exe yolunu seçin."
            )
        })?;
    let tmp = tempfile::tempdir().context("Geçici dönüştürme klasörü oluşturulamadı")?;
    let produced = libreoffice_convert(&lo, src, tmp.path(), "pdf", None)?;
    std::fs::copy(&produced, out)?;
    Ok(())
}

fn run_engines(
    src: &Path,
    out: &Path,
    attempts: &[&str],
    soffice: Option<&Path>,
) -> Result<()> {
    let mut errors: Vec<String> = Vec::new();
    for attempt in attempts {
        let result = match *attempt {
            "word" => convert_via_word(src, out),
            "excel" => convert_via_excel(src, out),
            _ => convert_via_libreoffice(src, out, soffice),
        };
        match result {
            Ok(()) => return Ok(()),
            Err(e) => {
                let detail = format!("{e:#}");
                if !errors.contains(&detail) {
                    errors.push(detail);
                }
            }
        }
    }
    if errors.len() == 1 {
        return Err(anyhow!("{}", errors.remove(0)));
    }
    if errors.is_empty() {
        return Err(anyhow!("Dönüştürme için uygun motor bulunamadı."));
    }
    Err(anyhow!(
        "Dönüştürme başarısız. Denenen yöntemler:\n\n• {}",
        errors.join("\n\n• ")
    ))
}

fn convert_document(src: &Path, out: &Path, engine: Engine, soffice: Option<&Path>) -> Result<()> {
    let ext = util::extension(src);
    let word_ok = word_available();
    let word_first = word_ok && matches!(ext.as_str(), "doc" | "docx" | "rtf" | "odt");
    let attempts: Vec<&str> = match engine {
        Engine::Word => vec!["word"],
        Engine::LibreOffice => vec!["libreoffice"],
        Engine::Auto if word_first => vec!["word", "libreoffice"],
        Engine::Auto => vec!["libreoffice"],
    };
    run_engines(src, out, &attempts, soffice)
}

/// XLSX / XLS / XLSM dosyaları için motor seçimi. Excel kuruluysa önce Excel
/// denenir; aksi halde (veya Excel başarısız olursa) LibreOffice kullanılır.
fn convert_spreadsheet(src: &Path, out: &Path, engine: Engine, soffice: Option<&Path>) -> Result<()> {
    let attempts: Vec<&str> = match engine {
        Engine::LibreOffice => vec!["libreoffice"],
        _ if excel_available() => vec!["excel", "libreoffice"],
        _ => vec!["libreoffice"],
    };
    run_engines(src, out, &attempts, soffice)
}

pub fn image_to_pdf(src: &Path, out: &Path) -> Result<()> {
    let bytes = crate::overlay::build_image_pdf(src, 150.0)
        .with_context(|| format!("Görsel PDF'e çevrilemedi: {}", src.display()))?;
    std::fs::write(out, bytes)?;
    Ok(())
}

pub fn convert_any_to_pdf(
    src: &Path,
    out: &Path,
    engine: Engine,
    soffice: Option<&Path>,
) -> Result<()> {
    let ext = util::extension(src);
    match ext.as_str() {
        "pdf" => {
            std::fs::copy(src, out)?;
            Ok(())
        }
        "doc" | "docx" | "odt" | "rtf" => convert_document(src, out, engine, soffice),
        "xls" | "xlsx" | "xlsm" => convert_spreadsheet(src, out, engine, soffice),
        "udf" => crate::udf::udf_to_pdf(src, out),
        "png" | "jpg" | "jpeg" | "bmp" | "tif" | "tiff" | "webp" => image_to_pdf(src, out),
        other => Err(anyhow!(
            "Desteklenmeyen dosya türü: .{other}\nDesteklenen: DOC, DOCX, ODT, RTF, XLS, XLSX, XLSM, UDF, PNG, JPG, JPEG, BMP, TIFF, WEBP"
        )),
    }
}

pub fn collect_files_recursive(dir: &Path, exts: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(dir: &Path, exts: &[&str], out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                walk(&p, exts, out);
            } else {
                let e = util::extension(&p);
                if exts.iter().any(|x| *x == e) {
                    out.push(p);
                }
            }
        }
    }
    walk(dir, exts, &mut out);
    out
}

/// PDF → DOC / DOCX dönüşümü.
///
/// Önce LibreOffice'in Writer PDF içe aktarıcısı denenir. LibreOffice yoksa
/// veya dönüştürme başarısız olursa PDF metni çıkarılıp DOCX (veya Word'ün
/// açabildiği RTF tabanlı DOC) olarak yazılır.
pub fn pdf_to_word(
    soffice: Option<&Path>,
    pdf: &Path,
    out_dir: &Path,
    ext: &str,
) -> Result<PathBuf> {
    let ext = ext.trim_start_matches('.').to_ascii_lowercase();
    std::fs::create_dir_all(out_dir)?;
    let target = out_dir.join(format!("{}.{}", util::stem(pdf), ext));
    let mut first_error: Option<String> = None;
    let lo = soffice
        .map(|p| p.to_path_buf())
        .or_else(|| find_soffice(None));
    if let Some(lo) = lo {
        let filter = if ext == "doc" {
            "doc:MS Word 97"
        } else {
            "docx:MS Word 2007 XML"
        };
        let result = tempfile::tempdir()
            .context("Geçici dönüştürme klasörü oluşturulamadı")
            .and_then(|tmp| {
                let produced =
                    libreoffice_convert(&lo, pdf, tmp.path(), filter, Some("writer_pdf_import"))?;
                std::fs::copy(&produced, &target)?;
                Ok(())
            });
        match result {
            Ok(()) => return Ok(target),
            Err(e) => first_error = Some(format!("{e:#}")),
        }
    }
    match pdf_text_to_word(pdf, &target, &ext) {
        Ok(()) => Ok(target),
        Err(fallback_err) => match first_error {
            Some(lo_err) => Err(anyhow!(
                "PDF → {} dönüştürme başarısız.\n\n\
                 • LibreOffice: {lo_err}\n• Metin çıkarma: {fallback_err:#}",
                ext.to_uppercase()
            )),
            None => Err(fallback_err),
        },
    }
}

fn pdf_text_to_word(pdf: &Path, out: &Path, ext: &str) -> Result<()> {
    let bytes = std::fs::read(pdf).with_context(|| format!("PDF okunamadı: {}", pdf.display()))?;
    let text =
        crate::pdfcore::pdf_text(&bytes).map_err(|e| anyhow!("PDF metni çıkarılamadı: {e}"))?;
    if text.trim().is_empty() {
        return Err(anyhow!(
            "PDF içinde seçilebilir metin bulunamadı. Taranmış belgeler için önce OCR çalıştırın."
        ));
    }
    if ext == "docx" {
        crate::ooxml::write_docx(&text, out)?;
        return Ok(());
    }
    let rtf_path = out.with_extension("rtf");
    std::fs::write(&rtf_path, text_to_rtf(&text))?;
    if word_available() {
        let script = format!(
            "$ErrorActionPreference='Stop'; \
             $w=New-Object -ComObject Word.Application; $w.Visible=$false; \
             try {{ $d=$w.Documents.Open('{}',$false,$false); \
             $d.SaveAs([ref]'{}',[ref]0); $d.Close([ref]0) }} finally {{ $w.Quit() }}",
            rtf_path.to_string_lossy().replace('\'', "''"),
            out.to_string_lossy().replace('\'', "''")
        );
        let mut cmd = command(Path::new("powershell.exe"));
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
        if run(&mut cmd, "Microsoft Word DOC dönüştürme").is_ok()
            && std::fs::metadata(out).map(|m| m.len() > 0).unwrap_or(false)
        {
            let _ = std::fs::remove_file(&rtf_path);
            return Ok(());
        }
    }
    // Word yoksa RTF içeriği .doc uzantısıyla yazılır; Word ve LibreOffice
    // bu dosyayı sorunsuz açabilir. Kopyalama, var olan hedef dosyanın üzerine yazar.
    std::fs::copy(&rtf_path, out)?;
    let _ = std::fs::remove_file(&rtf_path);
    Ok(())
}

fn text_to_rtf(text: &str) -> String {
    let mut rtf =
        String::from("{\\rtf1\\ansi\\ansicpg1252\\deff0{\\fonttbl{\\f0 Calibri;}}\\f0\\fs22 ");
    for ch in text.chars() {
        match ch {
            '\\' => rtf.push_str("\\\\"),
            '{' => rtf.push_str("\\{"),
            '}' => rtf.push_str("\\}"),
            '\n' => rtf.push_str("\\par\n"),
            '\r' => {}
            '\t' => rtf.push_str("\\tab "),
            c if (c as u32) < 0x80 => rtf.push(c),
            c => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    let signed = *unit as i32;
                    let signed = if signed > 32767 {
                        signed - 65536
                    } else {
                        signed
                    };
                    rtf.push_str(&format!("\\u{signed}?"));
                }
            }
        }
    }
    rtf.push('}');
    rtf
}
