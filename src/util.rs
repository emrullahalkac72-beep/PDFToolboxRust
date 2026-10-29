use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Harici bir programın çıktısı. `timed_out` true ise süreç zaman aşımı
/// nedeniyle sonlandırılmıştır.
pub struct CapturedOutput {
    pub status: std::process::ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub timed_out: bool,
}

/// Komutu çalıştırır ve en fazla `timeout` süre bekler. Çıktı geçici
/// dosyalara yazılır; böylece alt süreçlerin boru (pipe) tutması nedeniyle
/// kilitlenme yaşanmaz. Zaman aşımında süreç ve alt süreç ağacı öldürülür.
pub fn run_captured(
    cmd: &mut std::process::Command,
    timeout: std::time::Duration,
) -> std::io::Result<CapturedOutput> {
    use std::process::Stdio;
    use std::time::Instant;

    let out_dir = tempfile::tempdir()?;
    let out_path = out_dir.path().join("stdout.txt");
    let err_path = out_dir.path().join("stderr.txt");
    cmd.stdin(Stdio::null())
        .stdout(Stdio::from(std::fs::File::create(&out_path)?))
        .stderr(Stdio::from(std::fs::File::create(&err_path)?));
    let mut child = cmd.spawn()?;
    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() >= timeout {
                    timed_out = true;
                    kill_process_tree(child.id());
                    let _ = child.kill();
                    break child.wait()?;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => {
                kill_process_tree(child.id());
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
        }
    };
    Ok(CapturedOutput {
        status,
        stdout: std::fs::read(&out_path).unwrap_or_default(),
        stderr: std::fs::read(&err_path).unwrap_or_default(),
        timed_out,
    })
}

fn kill_process_tree(pid: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
    }
}

/// "1-5, 8, 10-12" biçimindeki seçimi okur.
pub fn parse_ranges(input: &str, total: u32) -> Result<Vec<u32>, String> {
    let mut set = BTreeSet::new();
    for part in input.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((a, b)) = part.split_once('-') {
            let start: u32 = a
                .trim()
                .parse()
                .map_err(|_| format!("Geçersiz aralık: {part}"))?;
            let end: u32 = b
                .trim()
                .parse()
                .map_err(|_| format!("Geçersiz aralık: {part}"))?;
            if start == 0 || end == 0 || start > end {
                return Err(format!("Geçersiz aralık: {part}"));
            }
            for p in start..=end {
                if p <= total {
                    set.insert(p);
                }
            }
        } else {
            let p: u32 = part
                .parse()
                .map_err(|_| format!("Geçersiz sayfa: {part}"))?;
            if p == 0 {
                return Err(format!("Geçersiz sayfa: {part}"));
            }
            if p <= total {
                set.insert(p);
            }
        }
    }
    if set.is_empty() {
        return Err("Geçerli sayfa bulunamadı.".to_string());
    }
    Ok(set.into_iter().collect())
}

/// [1,2,3,5,8,9] -> "1-3, 5, 8-9"
pub fn format_ranges(pages: &[u32]) -> String {
    if pages.is_empty() {
        return "Hiçbiri".to_string();
    }
    let mut sorted = pages.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut out = Vec::new();
    let mut start = sorted[0];
    let mut end = sorted[0];
    for &p in sorted.iter().skip(1) {
        if p == end + 1 {
            end = p;
        } else {
            out.push(if start == end {
                start.to_string()
            } else {
                format!("{start}-{end}")
            });
            start = p;
            end = p;
        }
    }
    out.push(if start == end {
        start.to_string()
    } else {
        format!("{start}-{end}")
    });
    out.join(", ")
}

pub fn short_ranges(pages: &[u32], max: usize) -> String {
    let s = format_ranges(pages);
    if s.chars().count() <= max {
        s
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(3)).collect();
        t.push_str("...");
        t
    }
}

pub fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("dosya")
        .to_string()
}

pub fn stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("dosya")
        .to_string()
}

pub fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

pub fn unique_output(dir: &Path, base: &str, ext: &str) -> PathBuf {
    let ext = ext.trim_start_matches('.');
    let first = dir.join(format!("{base}.{ext}"));
    if !first.exists() {
        return first;
    }
    for n in 2..10000 {
        let p = dir.join(format!("{base}_{n}.{ext}"));
        if !p.exists() {
            return p;
        }
    }
    dir.join(format!("{base}_{}.", ext))
}

pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

#[allow(dead_code)]
pub fn html_escape(s: &str) -> String {
    xml_escape(s)
}

pub fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[allow(dead_code)]
pub fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// PDF metinlerinde yaygın olarak kullanılan PDFDocEncoding / UTF-16 çözümü.
pub fn decode_pdf_string(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let mut u16s = Vec::with_capacity(bytes.len() / 2);
        let mut i = 2;
        while i + 1 < bytes.len() {
            u16s.push(u16::from_be_bytes([bytes[i], bytes[i + 1]]));
            i += 2;
        }
        return String::from_utf16_lossy(&u16s);
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let mut u16s = Vec::with_capacity(bytes.len() / 2);
        let mut i = 2;
        while i + 1 < bytes.len() {
            u16s.push(u16::from_le_bytes([bytes[i], bytes[i + 1]]));
            i += 2;
        }
        return String::from_utf16_lossy(&u16s);
    }
    // PDFDocEncoding ile UTF-8 çoğu Türkçe belgede aynıdır. Önce UTF-8 dene.
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        out.push(match b {
            0x80 => '•',
            0x91 => '‘',
            0x92 => '’',
            0x93 => '“',
            0x94 => '”',
            0x95 => '–',
            0x96 => '—',
            _ => b as char,
        });
    }
    out
}
