//! PDF yazdırma.
//!
//! Sayfalar `hayro` ile yazıcının çözünürlüğünde görüntülenir ve Windows GDI
//! (`StartDoc` / `StretchDIBits`) ile doğrudan yazıcıya gönderilir. Böylece
//! yazdırma için Acrobat, Edge veya başka bir PDF okuyucu kurulu olması
//! gerekmez.

use anyhow::Result;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrinterInfo {
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Debug)]
pub struct PrintJob {
    /// Yazıcı adı. Boş bırakılırsa Windows varsayılan yazıcısı kullanılır.
    pub printer: String,
    /// 1 tabanlı sayfa numaraları (yazdırma sırası korunur).
    pub pages: Vec<u32>,
    pub copies: u32,
    /// `true` ise sayfa kâğıt alanına sığdırılır, `false` ise gerçek boyutta
    /// (72 pt = 1 inç) yazdırılır.
    pub fit: bool,
    /// Belge adı (yazdırma kuyruğunda görünür).
    pub title: String,
}

/// Sistemde kurulu yazıcıları listeler. Windows dışında boş döner.
#[cfg(windows)]
pub fn list_printers() -> Vec<PrinterInfo> {
    use crate::util;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let script = "[Console]::OutputEncoding=[Text.Encoding]::UTF8; \
                  Get-CimInstance -ClassName Win32_Printer | \
                  ForEach-Object { \"$($_.Name)`t$($_.Default)\" }";
    let mut cmd = Command::new("powershell.exe");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        script,
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = util::run_captured(&mut cmd, std::time::Duration::from_secs(20));
    let mut printers: Vec<PrinterInfo> = Vec::new();
    if let Ok(out) = out {
        if out.status.success() {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let line = line.trim_end_matches(['\r', '\n']);
                if line.trim().is_empty() {
                    continue;
                }
                let (name, default) = match line.split_once('\t') {
                    Some((n, d)) => (n.trim(), d.trim()),
                    None => (line.trim(), ""),
                };
                if name.is_empty() {
                    continue;
                }
                let is_default = default.eq_ignore_ascii_case("true")
                    || default.eq_ignore_ascii_case("1")
                    || default.eq_ignore_ascii_case("evet");
                printers.push(PrinterInfo {
                    name: name.to_string(),
                    is_default,
                });
            }
        }
    }
    if printers.is_empty() {
        // Eski sistemler için yedek: `wmic`.
        let mut cmd = Command::new("wmic");
        cmd.args(["printer", "get", "name,default", "/format:csv"]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        if let Ok(out) = util::run_captured(&mut cmd, std::time::Duration::from_secs(20)) {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let parts: Vec<&str> = line.trim().split(',').collect();
                if parts.len() < 3 {
                    continue;
                }
                let name = parts[2].trim();
                if name.is_empty() || name.eq_ignore_ascii_case("Name") {
                    continue;
                }
                printers.push(PrinterInfo {
                    name: name.to_string(),
                    is_default: parts[1].trim().eq_ignore_ascii_case("TRUE"),
                });
            }
        }
    }
    printers.sort_by(|a, b| {
        b.is_default
            .cmp(&a.is_default)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    printers.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
    printers
}

#[cfg(not(windows))]
pub fn list_printers() -> Vec<PrinterInfo> {
    Vec::new()
}

/// Sayfaları yazıcıya gönderir. Dönen değer yazdırılan sayfa sayısıdır
/// (kopyalar dahil).
#[cfg(windows)]
pub fn print_pdf(
    bytes: &[u8],
    job: &PrintJob,
    mut progress: impl FnMut(u32, u32),
) -> Result<u32> {
    win::print_document(bytes, job, &mut progress)
}

#[cfg(not(windows))]
pub fn print_pdf(
    _bytes: &[u8],
    _job: &PrintJob,
    _progress: impl FnMut(u32, u32),
) -> Result<u32> {
    Err(anyhow::anyhow!(
        "Yazdırma yalnızca Windows'ta desteklenir."
    ))
}

#[cfg(windows)]
mod win {
    use super::{PrintJob, Result};
    use anyhow::anyhow;
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;

    type Hdc = *mut c_void;

    #[repr(C)]
    struct DocInfoW {
        cb_size: i32,
        doc_name: *const u16,
        output: *const u16,
        datatype: *const u16,
        fw_type: u32,
    }

    #[repr(C)]
    struct BitmapInfoHeader {
        bi_size: u32,
        bi_width: i32,
        bi_height: i32,
        bi_planes: u16,
        bi_bit_count: u16,
        bi_compression: u32,
        bi_size_image: u32,
        bi_x_pels_per_meter: i32,
        bi_y_pels_per_meter: i32,
        bi_clr_used: u32,
        bi_clr_important: u32,
    }

    #[repr(C)]
    struct BitmapInfo {
        header: BitmapInfoHeader,
        colors: [u32; 1],
    }

    #[link(name = "gdi32")]
    extern "system" {
        fn CreateDCW(
            driver: *const u16,
            device: *const u16,
            port: *const u16,
            devmode: *const c_void,
        ) -> Hdc;
        fn DeleteDC(hdc: Hdc) -> i32;
        fn StartDocW(hdc: Hdc, doc_info: *const DocInfoW) -> i32;
        fn StartPage(hdc: Hdc) -> i32;
        fn EndPage(hdc: Hdc) -> i32;
        fn EndDoc(hdc: Hdc) -> i32;
        fn AbortDoc(hdc: Hdc) -> i32;
        fn GetDeviceCaps(hdc: Hdc, index: i32) -> i32;
        fn SetStretchBltMode(hdc: Hdc, mode: i32) -> i32;
        fn StretchDIBits(
            hdc: Hdc,
            x_dest: i32,
            y_dest: i32,
            w_dest: i32,
            h_dest: i32,
            x_src: i32,
            y_src: i32,
            w_src: i32,
            h_src: i32,
            bits: *const c_void,
            bmi: *const BitmapInfo,
            usage: u32,
            rop: u32,
        ) -> i32;
    }

    const HORZRES: i32 = 8;
    const VERTRES: i32 = 10;
    const LOGPIXELSX: i32 = 88;
    const LOGPIXELSY: i32 = 90;
    const SRCCOPY: u32 = 0x00CC_0020;
    const DIB_RGB_COLORS: u32 = 0;
    const HALFTONE: i32 = 4;
    /// Yazıcı çözünürlüğü çok yüksek olsa da sayfa bu ölçekten büyük
    /// görüntülenmez (yaklaşık 288 DPI); bellek tüketimini sınırlar.
    const MAX_RENDER_SCALE: f32 = 4.0;

    /// Çok büyük sayfalarda bellek taşmasını engelleyen görüntüleme ölçeği.
    fn render_scale_limit(page_w: f32, page_h: f32) -> f32 {
        const MAX_EDGE: f32 = 6000.0;
        const MAX_PIXELS: f32 = 24_000_000.0;
        let w = page_w.max(1.0);
        let h = page_h.max(1.0);
        (MAX_EDGE / w.max(h))
            .min((MAX_PIXELS / (w * h)).sqrt())
            .clamp(0.2, 6.0)
    }

    fn wide(value: &str) -> Vec<u16> {
        std::ffi::OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    struct DeviceContext(Hdc);

    impl Drop for DeviceContext {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    DeleteDC(self.0);
                }
            }
        }
    }

    /// Yalnızca testlerde kullanılır: yazıcı bağlamını açar ve kâğıt
    /// alanı/DPI değerlerini döner (yazdırma işi başlatmaz).
    #[cfg(test)]
    pub fn probe_device(printer: &str) -> Option<(i32, i32, i32, i32)> {
        let driver = wide("WINSPOOL");
        let device = wide(printer);
        let hdc = unsafe {
            CreateDCW(
                driver.as_ptr(),
                device.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        if hdc.is_null() {
            return None;
        }
        let dc = DeviceContext(hdc);
        let caps = unsafe {
            (
                GetDeviceCaps(hdc, HORZRES),
                GetDeviceCaps(hdc, VERTRES),
                GetDeviceCaps(hdc, LOGPIXELSX),
                GetDeviceCaps(hdc, LOGPIXELSY),
            )
        };
        drop(dc);
        Some(caps)
    }

    pub fn print_document(
        bytes: &[u8],
        job: &PrintJob,
        progress: &mut dyn FnMut(u32, u32),
    ) -> Result<u32> {
        if job.pages.is_empty() {
            return Err(anyhow!("Yazdırılacak sayfa seçilmedi."));
        }
        let driver = wide("WINSPOOL");
        let device = if job.printer.trim().is_empty() {
            None
        } else {
            Some(wide(job.printer.trim()))
        };
        let hdc = unsafe {
            CreateDCW(
                driver.as_ptr(),
                device
                    .as_ref()
                    .map(|d| d.as_ptr())
                    .unwrap_or(std::ptr::null()),
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        if hdc.is_null() {
            let name = if job.printer.trim().is_empty() {
                "varsayılan yazıcı".to_string()
            } else {
                format!("\"{}\"", job.printer)
            };
            return Err(anyhow!(
                "Yazıcı açılamadı: {name}.\n\n\
                 Yazıcının açık ve doğru kurulmuş olduğunu kontrol edin."
            ));
        }
        let dc = DeviceContext(hdc);

        let (horz, vert, dpi_x, dpi_y) = unsafe {
            (
                GetDeviceCaps(hdc, HORZRES),
                GetDeviceCaps(hdc, VERTRES),
                GetDeviceCaps(hdc, LOGPIXELSX),
                GetDeviceCaps(hdc, LOGPIXELSY),
            )
        };
        if horz <= 0 || vert <= 0 {
            return Err(anyhow!(
                "Yazıcı kâğıt alanı okunamadı. Yazıcı sürücüsünü kontrol edin."
            ));
        }
        let dpi_x = if dpi_x > 0 { dpi_x as f32 } else { 300.0 };
        let dpi_y = if dpi_y > 0 { dpi_y as f32 } else { 300.0 };
        let render_scale = (dpi_x / 72.0).clamp(1.0, MAX_RENDER_SCALE);

        let title = if job.title.trim().is_empty() {
            "PDF Toolbox".to_string()
        } else {
            job.title.clone()
        };
        let title_w = wide(&title);
        let doc_info = DocInfoW {
            cb_size: std::mem::size_of::<DocInfoW>() as i32,
            doc_name: title_w.as_ptr(),
            output: std::ptr::null(),
            datatype: std::ptr::null(),
            fw_type: 0,
        };

        unsafe {
            SetStretchBltMode(hdc, HALFTONE);
        }
        if unsafe { StartDocW(hdc, &doc_info) } <= 0 {
            return Err(anyhow!("Yazdırma işi başlatılamadı (StartDoc)."));
        }

        let copies = job.copies.max(1);
        let total = job.pages.len() as u32 * copies;
        let geometries = crate::pdfcore::page_geometries(bytes).unwrap_or_default();
        let mut printed = 0u32;
        let result = (|| -> Result<()> {
            for _copy in 0..copies {
                for &page in &job.pages {
                    let index = page.saturating_sub(1) as usize;
                    let limit = geometries
                        .get(index)
                        .map(|g| render_scale_limit(g.width, g.height))
                        .unwrap_or(MAX_RENDER_SCALE);
                    let page_scale = render_scale.min(limit).max(0.2);
                    let image = crate::pdfcore::render_page(bytes, index, page_scale)
                        .map_err(|e| anyhow!("Sayfa {page} hazırlanamadı: {e:#}"))?;
                    if image.width == 0 || image.height == 0 {
                        return Err(anyhow!("Sayfa {page} boş görüntü üretti."));
                    }
                    // 32 bit BGRA, üstten alta (negatif yükseklik) DIB.
                    let mut dib = vec![0u8; image.rgba.len()];
                    for (src, dst) in image.rgba.chunks_exact(4).zip(dib.chunks_exact_mut(4)) {
                        dst[0] = src[2];
                        dst[1] = src[1];
                        dst[2] = src[0];
                        dst[3] = 255;
                    }

                    let page_w_pt = image.width as f32 / page_scale;
                    let page_h_pt = image.height as f32 / page_scale;
                    let mut w_dest = (page_w_pt * dpi_x / 72.0).round().max(1.0) as i32;
                    let mut h_dest = (page_h_pt * dpi_y / 72.0).round().max(1.0) as i32;
                    if job.fit {
                        let sx = horz as f32 / w_dest as f32;
                        let sy = vert as f32 / h_dest as f32;
                        let scale = sx.min(sy).min(1.0);
                        w_dest = ((w_dest as f32 * scale).round() as i32).max(1);
                        h_dest = ((h_dest as f32 * scale).round() as i32).max(1);
                    }
                    let x_dest = (horz - w_dest) / 2;
                    let y_dest = (vert - h_dest) / 2;

                    let bmi = BitmapInfo {
                        header: BitmapInfoHeader {
                            bi_size: std::mem::size_of::<BitmapInfoHeader>() as u32,
                            bi_width: image.width as i32,
                            bi_height: -(image.height as i32),
                            bi_planes: 1,
                            bi_bit_count: 32,
                            bi_compression: 0,
                            bi_size_image: dib.len() as u32,
                            bi_x_pels_per_meter: 0,
                            bi_y_pels_per_meter: 0,
                            bi_clr_used: 0,
                            bi_clr_important: 0,
                        },
                        colors: [0],
                    };

                    if unsafe { StartPage(hdc) } <= 0 {
                        return Err(anyhow!("Sayfa başlatılamadı (StartPage)."));
                    }
                    let drawn = unsafe {
                        StretchDIBits(
                            hdc,
                            x_dest,
                            y_dest,
                            w_dest,
                            h_dest,
                            0,
                            0,
                            image.width as i32,
                            image.height as i32,
                            dib.as_ptr() as *const c_void,
                            &bmi,
                            DIB_RGB_COLORS,
                            SRCCOPY,
                        )
                    };
                    unsafe {
                        EndPage(hdc);
                    }
                    if drawn == 0 || drawn == -1 {
                        return Err(anyhow!(
                            "Sayfa {page} yazıcıya aktarılamadı (StretchDIBits)."
                        ));
                    }
                    printed += 1;
                    progress(printed, total);
                }
            }
            Ok(())
        })();

        match result {
            Ok(()) => {
                if unsafe { EndDoc(hdc) } <= 0 {
                    return Err(anyhow!("Yazdırma işi tamamlanamadı (EndDoc)."));
                }
                drop(dc);
                Ok(printed)
            }
            Err(e) => {
                unsafe {
                    AbortDoc(hdc);
                }
                drop(dc);
                Err(e)
            }
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn printer_device_context_reports_capabilities() {
        let printers = list_printers();
        let Some(printer) = printers.first() else {
            eprintln!("Yazıcı bulunamadı, test atlandı.");
            return;
        };
        let caps = win::probe_device(&printer.name).expect("yazıcı bağlamı açılamadı");
        eprintln!("{}: {caps:?}", printer.name);
        assert!(caps.0 > 0 && caps.1 > 0, "kâğıt alanı okunamadı: {caps:?}");
        assert!(caps.2 > 0 && caps.3 > 0, "DPI okunamadı: {caps:?}");
    }
}
