use crate::fonts::FontSet;
use crate::model::{EditAction, EditItem, OcrMode, WatermarkMode};
use std::path::{Path, PathBuf};

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pdftoolbox_smoke_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn text_pdf(texts: &[&str]) -> Vec<u8> {
    let fonts = FontSet::load().expect("font");
    let pages: Vec<String> = texts.iter().map(|s| s.to_string()).collect();
    crate::overlay::build_text_pdf(&pages, &fonts).expect("text pdf")
}

/// Ham baytları WinAnsi karakterleriyle eşleşmeyen, yalnızca ToUnicode
/// tablosuyla çözülebilen küçük bir PDF üretir. 0x41 -> İ, 0x42 -> ş.
fn tounicode_pdf() -> Vec<u8> {
    use lopdf::{dictionary, Document, Object, Stream};

    let cmap = br#"/CIDInit /ProcSet findresource begin
12 dict begin
begincmap
/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def
/CMapName /Adobe-Identity-UCS def
/CMapType 2 def
1 begincodespacerange
<00> <FF>
endcodespacerange
2 beginbfchar
<41> <0130>
<42> <015F>
endbfchar
endcmap
CMapName currentdict /CMap defineresource pop
end
end"#;

    let mut doc = Document::with_version("1.5");
    let to_unicode_id = doc.add_object(Stream::new(dictionary! {}, cmap.to_vec()));
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
        "ToUnicode" => to_unicode_id,
    });
    let contents_id = doc.add_object(Stream::new(
        dictionary! {},
        b"BT /F1 24 Tf 72 720 Td (AB) Tj ET".to_vec(),
    ));
    let pages_id = doc.new_object_id();
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        },
        "Contents" => contents_id,
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

#[test]
fn pdf_core_pipeline() {
    let dir = tmp("core");
    let fonts = FontSet::load().expect("font");
    let one = text_pdf(&["Merhaba PDF Toolbox. Bu bir test belgesidir."]);
    let two = text_pdf(&["İkinci belge.", "Üçüncü sayfa."]);
    assert_eq!(crate::pdfcore::page_count(&one).unwrap(), 1);
    assert_eq!(crate::pdfcore::page_count(&two).unwrap(), 2);

    let rendered = crate::pdfcore::render_page(&one, 0, 1.0).unwrap();
    assert!(rendered.width > 100 && rendered.height > 100);
    assert_eq!(
        rendered.rgba.len() as u32,
        rendered.width * rendered.height * 4
    );

    let merged = crate::pdfcore::merge_page_sets_to_bytes(&[
        (one.clone(), vec![1]),
        (two.clone(), vec![1, 2]),
    ])
    .unwrap();
    assert_eq!(crate::pdfcore::page_count(&merged).unwrap(), 3);

    let extracted = crate::pdfcore::extract_pages_to_bytes(&merged, &[1, 3]).unwrap();
    assert_eq!(crate::pdfcore::page_count(&extracted).unwrap(), 2);

    let edits = vec![
        EditItem {
            page: 1,
            action: EditAction::Text {
                x: 60.0,
                y: 700.0,
                text: "Düzenleme metni".to_string(),
                size: 22.0,
                color: "red".to_string(),
                serif: false,
                bold: true,
            },
        },
        EditItem {
            page: 0,
            action: EditAction::Rectangle {
                x: 80.0,
                y: 550.0,
                w: 200.0,
                h: 80.0,
                fill: false,
                color: "blue".to_string(),
                lw: 2.0,
            },
        },
        EditItem {
            page: 1,
            action: EditAction::Circle {
                cx: 300.0,
                cy: 650.0,
                r: 40.0,
                fill: false,
                color: "green".to_string(),
                lw: 1.5,
            },
        },
        EditItem {
            page: 1,
            action: EditAction::Line {
                x1: 60.0,
                y1: 500.0,
                x2: 500.0,
                y2: 520.0,
                color: "orange".to_string(),
                width: 2.0,
            },
        },
        EditItem {
            page: 0,
            action: EditAction::Watermark {
                text: "GİZLİ".to_string(),
                mode: WatermarkMode::Diagonal,
                size: 60.0,
                alpha: 0.25,
                color: "gray".to_string(),
            },
        },
    ];
    let edited = dir.join("edited.pdf");
    crate::pdfcore::apply_edits_and_save(&one, &edited, &edits, &fonts, |_, _| {}).unwrap();
    let edited_bytes = std::fs::read(&edited).unwrap();
    assert_eq!(crate::pdfcore::page_count(&edited_bytes).unwrap(), 1);
    crate::pdfcore::render_page(&edited_bytes, 0, 1.0).unwrap();

    let rotated = dir.join("rotated.pdf");
    let rotate_edits = vec![EditItem {
        page: 1,
        action: EditAction::Rotate { angle: 90 },
    }];
    crate::pdfcore::apply_edits_and_save(&one, &rotated, &rotate_edits, &fonts, |_, _| {}).unwrap();
    let geoms = crate::pdfcore::page_geometries(&std::fs::read(&rotated).unwrap()).unwrap();
    assert_eq!(geoms[0].rotation, 90);

    let deleted = dir.join("deleted.pdf");
    let delete_edits = vec![EditItem {
        page: 1,
        action: EditAction::DeletePage,
    }];
    crate::pdfcore::apply_edits_and_save(&two, &deleted, &delete_edits, &fonts, |_, _| {}).unwrap();
    assert_eq!(
        crate::pdfcore::page_count(&std::fs::read(&deleted).unwrap()).unwrap(),
        1
    );

    let numbered = dir.join("numbered.pdf");
    let count = crate::pdfcore::add_page_numbers(
        &two,
        &numbered,
        1,
        12.0,
        "Alt Orta",
        "Sayfa {n} / {t}",
        true,
        &fonts,
        |_, _| {},
    )
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(
        crate::pdfcore::page_count(&std::fs::read(&numbered).unwrap()).unwrap(),
        2
    );

    let objects = crate::textobj::extract_text_objects(&one, 0).unwrap();
    assert!(!objects.is_empty(), "text objects should be parsed");
}

#[test]
fn extracts_tounicode_text() {
    let pdf = tounicode_pdf();
    let objects = crate::textobj::extract_text_objects(&pdf, 0).unwrap();
    let text: String = objects.iter().map(|o| o.text.as_str()).collect();
    assert!(
        text.contains("İş"),
        "ToUnicode eşlemesi uygulanmalı, bulunan metin: {text:?}"
    );
    assert!(!text.contains("AB"), "ham baytlar kullanılmamalı: {text:?}");
}

#[test]
fn text_edit_roundtrip_preserves_unicode() {
    let pdf = tounicode_pdf();
    let objects = crate::textobj::extract_text_objects(&pdf, 0).unwrap();
    let target = objects
        .iter()
        .find(|object| object.text.contains("İş"))
        .expect("hedef metin");
    let fonts = FontSet::load().expect("font");
    let dir = tmp("text_edit_unicode");
    let out = dir.join("out.pdf");
    let edits = vec![EditItem {
        page: 1,
        action: EditAction::TextEdit {
            idx: target.idx,
            bounds: target.bounds,
            old: target.text.clone(),
            new: "Yeni Öğretim (Öz Farkındalık)".to_string(),
            size: target.size,
            color: "#000000".to_string(),
            bg: "#FFFFFF".to_string(),
            origin: target.origin,
            serif: false,
            bold: false,
        },
    }];
    crate::pdfcore::apply_edits_and_save(&pdf, &out, &edits, &fonts, |_, _| {}).unwrap();
    let saved = std::fs::read(&out).unwrap();
    let after = crate::textobj::extract_text_objects(&saved, 0).unwrap();
    let text: String = after.iter().map(|object| object.text.as_str()).collect();
    assert!(
        text.contains("Yeni Öğretim") && text.contains("Öz Farkındalık"),
        "düzenlenen metin doğru çıkmalı, bulunan: {text:?}"
    );
    let rendered = crate::pdfcore::render_page(&saved, 0, 1.0).unwrap();
    assert!(rendered.width > 100 && rendered.height > 100);
}

#[test]
fn export_formats() {
    let dir = tmp("export");
    let pdf = text_pdf(&[
        "PDF Toolbox export test. Sayfa bir.",
        "İkinci sayfa metni burada.",
    ]);
    let src = dir.join("kaynak.pdf");
    std::fs::write(&src, &pdf).unwrap();
    let out = dir.join("cikti");
    std::fs::create_dir_all(&out).unwrap();

    let cases = [
        crate::export::ExportFormat::Png,
        crate::export::ExportFormat::Jpg,
        crate::export::ExportFormat::Txt,
        crate::export::ExportFormat::Csv,
        crate::export::ExportFormat::Xlsx,
        crate::export::ExportFormat::Docx,
        crate::export::ExportFormat::Pptx,
    ];
    for fmt in cases {
        let count =
            crate::export::export(&src, &pdf, &[1, 2], &out, fmt, 96, false, None, |_, _| {})
                .unwrap_or_else(|e| panic!("{fmt:?} failed: {e:#}"));
        assert_eq!(count, 2);
    }
    assert!(out.join("kaynak_p1.png").exists());
    assert!(out.join("kaynak_p1.jpg").exists());
    assert!(out.join("kaynak.txt").exists());
    assert!(out.join("kaynak.csv").exists());
    assert!(out.join("kaynak.xlsx").exists());
    assert!(out.join("kaynak.docx").exists());
    assert!(out.join("kaynak.pptx").exists());

    for name in ["kaynak.xlsx", "kaynak.docx", "kaynak.pptx"] {
        let f = std::fs::File::open(out.join(name)).unwrap();
        let zip = zip::ZipArchive::new(f).unwrap();
        assert!(zip.len() > 2, "{name} should be a valid zip");
    }

    let merged = out.join("kaynak_birlesik.png");
    crate::export::export(
        &src,
        &pdf,
        &[1, 2],
        &out,
        crate::export::ExportFormat::Png,
        72,
        true,
        None,
        |_, _| {},
    )
    .unwrap();
    assert!(merged.exists());
}

#[test]
fn udf_conversion() {
    let dir = tmp("udf");
    let body = "Birinci paragraf: Türkçe karakterler ğüşiöç.\nİkinci paragraf.";
    let first = "Birinci paragraf: Türkçe karakterler ğüşiöç.";
    let second = "İkinci paragraf.";
    let first_len = first.chars().count();
    let second_start = first_len + 1;
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<template>
  <content>{body}</content>
  <properties><pageformat leftmargin="42.5" rightmargin="42.5" topmargin="42.5" bottommargin="42.5"/></properties>
  <elements>
    <paragraph><content startOffset="0" length="{first_len}" size="12"/></paragraph>
    <paragraph><content startOffset="{second_start}" length="{}" size="12"/></paragraph>
  </elements>
</template>"#,
        second.chars().count()
    );
    let src = dir.join("test.udf");
    std::fs::write(&src, &xml).unwrap();
    let out = dir.join("test.pdf");
    crate::udf::udf_to_pdf(&src, &out).unwrap();
    let bytes = std::fs::read(&out).unwrap();
    assert!(crate::pdfcore::page_count(&bytes).unwrap() >= 1);
    crate::pdfcore::render_page(&bytes, 0, 1.0).unwrap();

    let zipped = dir.join("zipped.udf");
    {
        use std::io::Write;
        let file = std::fs::File::create(&zipped).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("content.xml", opts).unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
        zip.finish().unwrap();
    }
    let out2 = dir.join("zipped.pdf");
    crate::udf::udf_to_pdf(&zipped, &out2).unwrap();
    assert!(out2.exists());
}

#[test]
#[ignore = "LibreOffice kurulumuna ve çalışan bir profile bağlı"]
fn libreoffice_docx_conversion() {
    let Some(soffice) = crate::convert::find_soffice(None) else {
        eprintln!("LibreOffice bulunamadı, test atlandı.");
        return;
    };
    let dir = tmp("docx");
    let docx = dir.join("belge.docx");
    crate::ooxml::write_docx(
        "LibreOffice dönüştürme testi.\nTürkçe karakterler: ğüşiöç İIı.",
        &docx,
    )
    .unwrap();
    let out = dir.join("belge.pdf");
    crate::convert::convert_any_to_pdf(
        &docx,
        &out,
        crate::convert::Engine::LibreOffice,
        Some(&soffice),
    )
    .unwrap();
    let bytes = std::fs::read(&out).unwrap();
    assert!(crate::pdfcore::page_count(&bytes).unwrap() >= 1);

    let png = dir.join("belge.png");
    crate::export::export_images(
        &bytes,
        &[1],
        &dir,
        "belge",
        crate::export::ExportFormat::Png,
        96,
        false,
        &mut |_, _| {},
    )
    .unwrap();
    assert!(dir.join("belge_p1.png").exists());
    let _ = png;

    let img_pdf = dir.join("resim.pdf");
    crate::convert::image_to_pdf(&dir.join("belge_p1.png"), &img_pdf).unwrap();
    assert!(crate::pdfcore::page_count(&std::fs::read(&img_pdf).unwrap()).unwrap() == 1);
}

#[test]
fn word_docx_conversion() {
    if !crate::convert::word_available() {
        eprintln!("Microsoft Word bulunamadı, test atlandı.");
        return;
    }
    let dir = tmp("word");
    let docx = dir.join("belge.docx");
    crate::ooxml::write_docx(
        "Word COM dönüştürme testi.\nTürkçe karakterler: ğüşiöç İIı.",
        &docx,
    )
    .unwrap();
    let out = dir.join("belge.pdf");
    crate::convert::convert_any_to_pdf(&docx, &out, crate::convert::Engine::Word, None).unwrap();
    let bytes = std::fs::read(&out).unwrap();
    assert!(crate::pdfcore::page_count(&bytes).unwrap() >= 1);
}

#[test]
fn command_timeout_is_enforced() {
    #[cfg(windows)]
    {
        let start = std::time::Instant::now();
        let mut cmd = std::process::Command::new("cmd");
        cmd.args(["/C", "ping -n 30 127.0.0.1 > nul"]);
        let output = crate::util::run_captured(&mut cmd, std::time::Duration::from_secs(1))
            .expect("komut çalıştırılamadı");
        assert!(output.timed_out, "komut zaman aşımına uğramalıydı");
        assert!(
            start.elapsed() < std::time::Duration::from_secs(15),
            "zaman aşımı çok uzun sürdü"
        );
    }
}

#[test]
fn pdf_to_word_text_fallback() {
    let dir = tmp("pdf_to_word");
    let pdf = text_pdf(&["PDF metninden Word belgesi. Türkçe: ğüşiöç."]);
    let src = dir.join("kaynak.pdf");
    std::fs::write(&src, &pdf).unwrap();
    let out_dir = dir.join("cikti");
    std::fs::create_dir_all(&out_dir).unwrap();
    let missing = PathBuf::from(r"C:\nonexistent\soffice.exe");
    let produced = crate::convert::pdf_to_word(Some(&missing), &src, &out_dir, "docx").unwrap();
    assert!(produced.exists());
    let file = std::fs::File::open(&produced).unwrap();
    let zip = zip::ZipArchive::new(file).unwrap();
    assert!(zip.len() >= 3);
}

#[test]
fn export_doc_fallback() {
    let dir = tmp("export_doc");
    let pdf = text_pdf(&["PDF metninden DOC çıktısı. Türkçe: ğüşiöç."]);
    let src = dir.join("kaynak.pdf");
    std::fs::write(&src, &pdf).unwrap();
    let out = dir.join("cikti");
    std::fs::create_dir_all(&out).unwrap();
    let missing = PathBuf::from(r"C:\nonexistent\soffice.exe");
    let count = crate::export::export(
        &src,
        &pdf,
        &[1],
        &out,
        crate::export::ExportFormat::Doc,
        96,
        false,
        Some(&missing),
        |_, _| {},
    )
    .unwrap();
    assert_eq!(count, 1);
    let doc = out.join("kaynak.doc");
    assert!(doc.exists());
    assert!(std::fs::metadata(&doc).unwrap().len() > 100);
}

#[test]
fn ocr_text_mode() {
    let Some(tess) = crate::ocr::find_tesseract(None) else {
        eprintln!("Tesseract bulunamadı, test atlandı.");
        return;
    };
    let dir = tmp("ocr");
    let pdf = text_pdf(&["PDF TOOLBOX OCR TEST 12345"]);
    let rendered = crate::pdfcore::render_page(&pdf, 0, 200.0 / 72.0).unwrap();
    let img = image::RgbaImage::from_raw(rendered.width, rendered.height, rendered.rgba).unwrap();
    let png = dir.join("scan.png");
    img.save(&png).unwrap();
    let scan_pdf = dir.join("scan.pdf");
    crate::convert::image_to_pdf(&png, &scan_pdf).unwrap();

    let out = dir.join("ocr.pdf");
    let lang = if crate::ocr::check_language(&tess, "eng").is_ok() {
        "eng"
    } else {
        eprintln!("eng dil paketi yok, test atlandı.");
        return;
    };
    crate::ocr::ocr_pdf(
        &scan_pdf,
        &out,
        OcrMode::Text,
        lang,
        200,
        Some(&tess),
        |_, _| {},
    )
    .unwrap();
    let bytes = std::fs::read(&out).unwrap();
    let text = crate::pdfcore::pdf_text(&bytes).unwrap().to_uppercase();
    assert!(
        text.contains("PDF") || text.contains("TOOLBOX"),
        "OCR text: {text}"
    );
}

#[test]
fn spreadsheet_conversion_uses_office_engine() {
    let dir = tmp("xlsx");
    let xlsx = dir.join("tablo.xlsx");
    crate::ooxml::write_xlsx(
        &[(
            "Sayfa1".to_string(),
            vec!["Ad;Soyad".to_string(), "Ali;Veli".to_string()],
        )],
        &xlsx,
    )
    .unwrap();
    let out = dir.join("tablo.pdf");
    // LibreOffice yolu geçersiz: dönüştürme hata döndürmeli, panik olmamalı.
    let missing = PathBuf::from(r"C:\nonexistent\soffice.exe");
    let result = crate::convert::convert_any_to_pdf(
        &xlsx,
        &out,
        crate::convert::Engine::LibreOffice,
        Some(&missing),
    );
    let message = format!("{:#}", result.expect_err("LibreOffice olmadan başarılı olmamalı"));
    assert!(
        message.to_lowercase().contains("libreoffice"),
        "beklenmeyen hata: {message}"
    );
    assert!(!out.exists());
}

#[test]
#[ignore = "LibreOffice (Calc) kurulumuna bağlı"]
fn libreoffice_spreadsheet_conversion() {
    let Some(soffice) = crate::convert::find_soffice(None) else {
        eprintln!("LibreOffice bulunamadı, test atlandı.");
        return;
    };
    let dir = tmp("xlsx_libreoffice");
    let xlsx = dir.join("tablo.xlsx");
    crate::ooxml::write_xlsx(
        &[(
            "Sayfa1".to_string(),
            vec![
                "Ürün;Adet;Fiyat".to_string(),
                "Kalem;10;5,50".to_string(),
            ],
        )],
        &xlsx,
    )
    .unwrap();
    let out = dir.join("tablo.pdf");
    crate::convert::convert_any_to_pdf(
        &xlsx,
        &out,
        crate::convert::Engine::LibreOffice,
        Some(&soffice),
    )
    .unwrap();
    let bytes = std::fs::read(&out).unwrap();
    assert!(crate::pdfcore::page_count(&bytes).unwrap() >= 1);
}

#[test]
fn printer_listing_is_safe() {
    let printers = crate::print::list_printers();
    for printer in &printers {
        assert!(!printer.name.trim().is_empty());
    }
    // Aynı ada sahip yinelenen kayıt olmamalı.
    for (i, a) in printers.iter().enumerate() {
        for b in printers.iter().skip(i + 1) {
            assert_ne!(a.name, b.name);
        }
    }
}

fn _unused(_: &Path) {}
