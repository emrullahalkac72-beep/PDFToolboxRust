use anyhow::{anyhow, Context, Result};
use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::vello_cpu::peniko::ImageAlphaType;
use hayro::{render, PixmapSettings, RenderCache, RenderSettings};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use std::collections::HashMap;
use std::path::Path;

use crate::model::{EditItem, RenderedPage};

#[derive(Clone, Debug)]
pub struct PageGeom {
    pub width: f32,
    pub height: f32,
    #[allow(dead_code)]
    pub rotation: i32,
    #[allow(dead_code)]
    pub media: [f32; 4],
}

pub fn load_document(bytes: &[u8]) -> Result<Document> {
    Document::load_mem(bytes).context("PDF dosyası açılamadı")
}

pub fn page_count(bytes: &[u8]) -> Result<u32> {
    Ok(load_document(bytes)?.get_pages().len() as u32)
}

pub fn page_geometries(bytes: &[u8]) -> Result<Vec<PageGeom>> {
    let doc = load_document(bytes)?;
    let pages = doc.get_pages();
    let mut out = Vec::with_capacity(pages.len());
    for (_, id) in pages {
        out.push(page_geometry(&doc, id));
    }
    Ok(out)
}

pub fn page_geometry(doc: &Document, page_id: ObjectId) -> PageGeom {
    let media = media_box(doc, page_id);
    let rotation = inherited_i64(doc, page_id, b"Rotate").unwrap_or(0) as i32;
    PageGeom {
        width: (media[2] - media[0]).abs(),
        height: (media[3] - media[1]).abs(),
        rotation: rotation.rem_euclid(360),
        media,
    }
}

pub fn media_box(doc: &Document, page_id: ObjectId) -> [f32; 4] {
    if let Some(Object::Array(arr)) = inherited_object(doc, page_id, b"MediaBox") {
        if arr.len() >= 4 {
            return [
                object_f32(&arr[0]).unwrap_or(0.0),
                object_f32(&arr[1]).unwrap_or(0.0),
                object_f32(&arr[2]).unwrap_or(595.28),
                object_f32(&arr[3]).unwrap_or(841.89),
            ];
        }
    }
    [0.0, 0.0, 595.28, 841.89]
}

#[allow(dead_code)]
pub fn crop_box(doc: &Document, page_id: ObjectId) -> [f32; 4] {
    if let Some(Object::Array(arr)) = inherited_object(doc, page_id, b"CropBox") {
        if arr.len() >= 4 {
            return [
                object_f32(&arr[0]).unwrap_or(0.0),
                object_f32(&arr[1]).unwrap_or(0.0),
                object_f32(&arr[2]).unwrap_or(595.28),
                object_f32(&arr[3]).unwrap_or(841.89),
            ];
        }
    }
    media_box(doc, page_id)
}

fn object_f32(obj: &Object) -> Option<f32> {
    match obj {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(r) => Some(*r),
        _ => None,
    }
}

fn inherited_object(doc: &Document, page_id: ObjectId, key: &[u8]) -> Option<Object> {
    let mut current = Some(page_id);
    let mut guard = 0;
    while let Some(id) = current {
        guard += 1;
        if guard > 64 {
            break;
        }
        let dict = doc.get_dictionary(id).ok()?;
        if let Ok(obj) = dict.get(key) {
            return Some(match obj {
                Object::Reference(r) => doc.get_object(*r).cloned().unwrap_or(Object::Null),
                other => other.clone(),
            });
        }
        current = dict.get(b"Parent").and_then(Object::as_reference).ok();
    }
    None
}

fn inherited_i64(doc: &Document, page_id: ObjectId, key: &[u8]) -> Option<i64> {
    inherited_object(doc, page_id, key).and_then(|o| match o {
        Object::Integer(i) => Some(i),
        Object::Real(r) => Some(r as i64),
        _ => None,
    })
}

#[allow(dead_code)]
pub fn extract_pages_document(bytes: &[u8], pages: &[u32]) -> Result<Document> {
    merge_page_sets(&[(bytes.to_vec(), pages.to_vec())])
}

pub fn merge_page_sets(items: &[(Vec<u8>, Vec<u32>)]) -> Result<Document> {
    let mut out = Document::with_version("1.6");
    let pages_id = out.new_object_id();
    let catalog = dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    };
    let catalog_id = out.add_object(catalog);
    out.trailer.set("Root", Object::Reference(catalog_id));

    let mut kids: Vec<Object> = Vec::new();
    for (bytes, page_numbers) in items {
        let src = load_document(bytes)?;
        let src_pages = src.get_pages();
        let mut map: HashMap<ObjectId, ObjectId> = HashMap::new();
        for &p in page_numbers {
            let Some(&src_id) = src_pages.get(&p) else {
                continue;
            };
            let Some(dict) = src.get_dictionary(src_id).ok().cloned() else {
                continue;
            };
            let new_id = out.new_object_id();
            map.insert(src_id, new_id);

            let mut new_dict = Dictionary::new();
            for (k, v) in dict.iter() {
                if k == b"Parent" {
                    continue;
                }
                let imported = import_object(&mut out, &src, v, &mut map, 0);
                new_dict.set(k.clone(), imported.unwrap_or(Object::Null));
            }
            new_dict.set("Parent", Object::Reference(pages_id));
            out.set_object(new_id, Object::Dictionary(new_dict));
            kids.push(Object::Reference(new_id));
        }
    }

    if kids.is_empty() {
        return Err(anyhow!(
            "Seçilen sayfalarda aktarılabilir sayfa bulunamadı."
        ));
    }
    let count = kids.len() as i64;
    out.set_object(
        pages_id,
        dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(kids),
            "Count" => Object::Integer(count),
        },
    );
    out.prune_objects();
    out.compress();
    Ok(out)
}

pub fn merge_page_sets_to_bytes(items: &[(Vec<u8>, Vec<u32>)]) -> Result<Vec<u8>> {
    let mut doc = merge_page_sets(items)?;
    let mut buf = Vec::new();
    doc.save_to(&mut buf)?;
    Ok(buf)
}

pub fn extract_pages_to_bytes(bytes: &[u8], pages: &[u32]) -> Result<Vec<u8>> {
    merge_page_sets_to_bytes(&[(bytes.to_vec(), pages.to_vec())])
}

pub fn save_document(doc: &mut Document, path: &Path) -> Result<()> {
    let mut buf = Vec::new();
    doc.save_to(&mut buf)?;
    std::fs::write(path, buf)?;
    Ok(())
}

fn is_page_tree_node(obj: &Object) -> bool {
    let dict = match obj {
        Object::Dictionary(d) => d,
        Object::Stream(s) => &s.dict,
        _ => return false,
    };
    dict.get(b"Type")
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(|n| n == b"Pages")
        .unwrap_or(false)
}

fn import_object(
    dest: &mut Document,
    src: &Document,
    obj: &Object,
    map: &mut HashMap<ObjectId, ObjectId>,
    depth: usize,
) -> Result<Object> {
    if depth > 96 {
        return Ok(Object::Null);
    }
    match obj {
        Object::Reference(id) => {
            if let Some(new_id) = map.get(id) {
                return Ok(Object::Reference(*new_id));
            }
            let Some(source_obj) = src.objects.get(id) else {
                return Ok(Object::Null);
            };
            if is_page_tree_node(source_obj) {
                return Ok(Object::Null);
            }
            let new_id = dest.new_object_id();
            map.insert(*id, new_id);
            let imported = import_object(dest, src, source_obj, map, depth + 1)?;
            dest.set_object(new_id, imported);
            Ok(Object::Reference(new_id))
        }
        Object::Array(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                out.push(import_object(dest, src, item, map, depth + 1)?);
            }
            Ok(Object::Array(out))
        }
        Object::Dictionary(dict) => {
            let mut out = Dictionary::new();
            for (k, v) in dict.iter() {
                if k == b"Parent" {
                    continue;
                }
                out.set(k.clone(), import_object(dest, src, v, map, depth + 1)?);
            }
            Ok(Object::Dictionary(out))
        }
        Object::Stream(stream) => {
            let mut dict = Dictionary::new();
            for (k, v) in stream.dict.iter() {
                if k == b"Parent" {
                    continue;
                }
                dict.set(k.clone(), import_object(dest, src, v, map, depth + 1)?);
            }
            let mut new_stream = Stream::new(dict, stream.content.clone());
            new_stream.allows_compression = stream.allows_compression;
            Ok(Object::Stream(new_stream))
        }
        Object::String(s, fmt) => Ok(Object::String(s.clone(), *fmt)),
        other => Ok(other.clone()),
    }
}

pub fn append_overlay(base: &mut Document, page_id: ObjectId, overlay_bytes: &[u8]) -> Result<()> {
    let overlay = Document::load_mem(overlay_bytes)?;
    let overlay_page = *overlay
        .get_pages()
        .get(&1)
        .ok_or_else(|| anyhow!("Overlay PDF'te sayfa yok."))?;
    let overlay_dict = overlay.get_dictionary(overlay_page)?;
    let content = overlay.get_page_content(overlay_page);
    let resources = overlay_dict
        .get(b"Resources")
        .cloned()
        .unwrap_or(Object::Dictionary(Dictionary::new()));
    let obox = media_box(&overlay, overlay_page);
    let ow = (obox[2] - obox[0]).abs();
    let oh = (obox[3] - obox[1]).abs();

    let mut map = HashMap::new();
    let imported_resources = import_object(base, &overlay, &resources, &mut map, 0)?;

    let mut form_dict = Dictionary::new();
    form_dict.set("Type", "XObject");
    form_dict.set("Subtype", "Form");
    form_dict.set("FormType", Object::Integer(1));
    form_dict.set(
        "BBox",
        Object::Array(vec![
            Object::Real(0.0),
            Object::Real(0.0),
            Object::Real(ow),
            Object::Real(oh),
        ]),
    );
    form_dict.set("Resources", imported_resources);
    let form_stream = Stream::new(form_dict, content);
    let form_id = base.add_object(form_stream);
    let xname = format!("PTBox{}", form_id.0);
    base.add_xobject(page_id, xname.as_bytes().to_vec(), form_id)?;

    let media = media_box(base, page_id);
    let cmd = format!(
        "q 1 0 0 1 {} {} cm /{} Do Q",
        fmt_num(media[0]),
        fmt_num(media[1]),
        xname
    );
    base.add_page_contents(page_id, cmd.into_bytes())?;
    Ok(())
}

fn fmt_num(v: f32) -> String {
    if (v - v.round()).abs() < 0.0001 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.3}")
    }
}

pub fn rotate_page(doc: &mut Document, page_id: ObjectId, angle: i32) {
    let current = inherited_i64(doc, page_id, b"Rotate").unwrap_or(0) as i32;
    let new_angle = (current + angle).rem_euclid(360);
    if let Ok(dict) = doc.get_dictionary_mut(page_id) {
        if new_angle == 0 {
            dict.remove(b"Rotate");
        } else {
            dict.set("Rotate", Object::Integer(new_angle as i64));
        }
    }
}

pub fn render_page(bytes: &[u8], page_index: usize, scale: f32) -> Result<RenderedPage> {
    let pdf = Pdf::new(bytes.to_vec()).map_err(|e| anyhow!("PDF açılamadı: {e:?}"))?;
    let pages = pdf.pages();
    let page = pages.get(page_index).ok_or_else(|| {
        anyhow!(
            "Sayfa {} bulunamadı (toplam {}).",
            page_index + 1,
            pages.len()
        )
    })?;
    let cache = RenderCache::new();
    let interpreter = InterpreterSettings::default();
    let settings = RenderSettings::default();
    let pixmap_settings = PixmapSettings {
        x_scale: scale,
        y_scale: scale,
        bg_color: WHITE,
    };
    let pixmap = render(page, &cache, &interpreter, &settings, &pixmap_settings);
    let width = pixmap.width() as u32;
    let height = pixmap.height() as u32;
    let rgba = pixmap.take_rgba8(ImageAlphaType::Alpha);
    Ok(RenderedPage {
        width,
        height,
        rgba,
    })
}

pub fn apply_edits_and_save(
    src: &[u8],
    out: &Path,
    edits: &[EditItem],
    fonts: &crate::fonts::FontSet,
    mut progress: impl FnMut(u32, u32),
) -> Result<()> {
    let mut doc = load_document(src)?;
    let page_ids: Vec<(u32, ObjectId)> = doc.get_pages().into_iter().collect();
    let total = page_ids.len() as u32;
    let mut delete_pages = Vec::new();
    let mut rotate_angles: HashMap<u32, i32> = HashMap::new();
    for e in edits {
        match e.action {
            crate::model::EditAction::DeletePage => delete_pages.push(e.page),
            crate::model::EditAction::Rotate { angle } => {
                *rotate_angles.entry(e.page).or_insert(0) += angle;
            }
            _ => {}
        }
    }
    for (p, id) in page_ids {
        if delete_pages.contains(&p) {
            progress(p, total);
            continue;
        }
        let page_edits: Vec<EditItem> = edits
            .iter()
            .filter(|e| {
                !matches!(
                    e.action,
                    crate::model::EditAction::DeletePage | crate::model::EditAction::Rotate { .. }
                )
            })
            .filter(|e| e.page == 0 || e.page == p)
            .cloned()
            .collect();
        if !page_edits.is_empty() {
            let geo = page_geometry(&doc, id);
            let overlay = crate::overlay::build_overlay(geo.width, geo.height, &page_edits, fonts)?;
            append_overlay(&mut doc, id, &overlay)?;
        }
        let mut angle = 0;
        if let Some(a) = rotate_angles.get(&0) {
            angle += a;
        }
        if let Some(a) = rotate_angles.get(&p) {
            angle += a;
        }
        if angle != 0 {
            rotate_page(&mut doc, id, angle);
        }
        progress(p, total);
    }
    if delete_pages.len() >= total as usize {
        return Err(anyhow!("Tüm sayfalar silinmiş, kaydedilecek sayfa yok."));
    }
    if !delete_pages.is_empty() {
        doc.delete_pages(&delete_pages);
    }
    doc.prune_objects();
    doc.compress();
    save_document(&mut doc, out)
}

pub fn add_page_numbers(
    src: &[u8],
    out: &Path,
    start: u32,
    font_size: f32,
    position: &str,
    format: &str,
    skip_first: bool,
    fonts: &crate::fonts::FontSet,
    mut progress: impl FnMut(u32, u32),
) -> Result<u32> {
    let bytes = src.to_vec();
    let mut doc = load_document(&bytes)?;
    let page_ids: Vec<(u32, ObjectId)> = doc.get_pages().into_iter().collect();
    let total = page_ids.len() as u32;
    let last_number = start + total.saturating_sub(1);
    let mut written = 0u32;
    for (p, id) in page_ids {
        if skip_first && p == 1 {
            progress(p, total);
            continue;
        }
        let geo = page_geometry(&doc, id);
        let text = format
            .replace("{n}", &(p + start - 1).to_string())
            .replace("{t}", &last_number.to_string());
        let est_w = crate::overlay::estimate_text_width(&text, font_size);
        let margin = 28.0f32;
        let (vertical, horizontal) = position.split_once(' ').unwrap_or(("Alt", "Orta"));
        let x = match horizontal {
            "Sağ" => (geo.width - margin - est_w).max(margin),
            "Sol" => margin,
            _ => (geo.width - est_w) / 2.0,
        };
        let y = if vertical == "Üst" {
            geo.height - margin - font_size * 0.8
        } else {
            margin
        };
        let edit = EditItem {
            page: p,
            action: crate::model::EditAction::Text {
                x,
                y,
                text,
                size: font_size,
                color: "black".to_string(),
                serif: false,
                bold: false,
            },
        };
        let overlay = crate::overlay::build_overlay(geo.width, geo.height, &[edit], fonts)?;
        append_overlay(&mut doc, id, &overlay)?;
        written += 1;
        progress(p, total);
    }
    doc.prune_objects();
    doc.compress();
    save_document(&mut doc, out)?;
    Ok(written)
}

pub fn pdf_text(bytes: &[u8]) -> Result<String> {
    let doc = load_document(bytes)?;
    let pages: Vec<u32> = doc.get_pages().keys().copied().collect();
    let mut out = String::new();
    for p in pages {
        if let Ok(t) = doc.extract_text(&[p]) {
            out.push_str(&t);
            out.push('\n');
        }
    }
    Ok(out)
}
