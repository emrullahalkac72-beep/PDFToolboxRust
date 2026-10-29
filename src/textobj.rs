use anyhow::{anyhow, Result};
use lopdf::content::Content;
use lopdf::{Document, Object};
use pdf_extract::MediaBox;
use std::collections::HashMap;

use crate::model::TextObject;
use crate::util::decode_pdf_string;

#[derive(Clone)]
struct State {
    font: String,
    size: f32,
    fill: String,
    leading: f32,
    tm: [f32; 6],
    tlm: [f32; 6],
}

impl Default for State {
    fn default() -> Self {
        Self {
            font: String::new(),
            size: 12.0,
            fill: "black".to_string(),
            leading: 0.0,
            tm: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            tlm: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        }
    }
}

fn num(op: &Object) -> f32 {
    match op {
        Object::Integer(i) => *i as f32,
        Object::Real(r) => *r,
        _ => 0.0,
    }
}

fn color_hex_from_rgb(r: f32, g: f32, b: f32) -> String {
    let clamp = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02X}{:02X}{:02X}", clamp(r), clamp(g), clamp(b))
}

fn state_fill_ops(state: &mut State, op: &str, operands: &[Object]) {
    match op {
        "rg" if operands.len() >= 3 => {
            state.fill =
                color_hex_from_rgb(num(&operands[0]), num(&operands[1]), num(&operands[2]));
        }
        "g" if !operands.is_empty() => {
            let v = num(&operands[0]);
            state.fill = color_hex_from_rgb(v, v, v);
        }
        "k" if operands.len() >= 4 => {
            let c = num(&operands[0]);
            let m = num(&operands[1]);
            let y = num(&operands[2]);
            let k = num(&operands[3]);
            state.fill = color_hex_from_rgb(
                (1.0 - c) * (1.0 - k),
                (1.0 - m) * (1.0 - k),
                (1.0 - y) * (1.0 - k),
            );
        }
        _ => {}
    }
}

/// Metin nesnelerini, gömülü yazı tiplerinin ToUnicode/CID eşlemelerini
/// dikkate alarak çıkarır. `pdf-extract`, alt küme yazı tiplerindeki ham
/// baytları doğru Unicode karakterlerine çevirir; ham lopdf çıkarımı ise
/// yalnızca çözümleme başarısız olursa yedek olarak kullanılır.
pub fn extract_text_objects(bytes: &[u8], page_index: usize) -> Result<Vec<TextObject>> {
    let raw = extract_raw_text_objects(bytes, page_index);
    let decoded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        extract_decoded_text_objects(bytes, page_index)
    }));
    match decoded {
        Ok(Ok(mut objects)) if !objects.is_empty() => {
            if let Ok(raw_objects) = &raw {
                for object in &mut objects {
                    copy_decoded_style(object, raw_objects);
                }
            }
            Ok(objects)
        }
        _ => raw,
    }
}

fn extract_decoded_text_objects(bytes: &[u8], page_index: usize) -> Result<Vec<TextObject>> {
    let doc = pdf_extract::Document::load_mem(bytes).map_err(|e| anyhow!("PDF açılamadı: {e}"))?;
    let mut device = DecodedTextDev::default();
    pdf_extract::output_doc_page(&doc, &mut device, (page_index + 1) as u32)
        .map_err(|e| anyhow!("Metin çıkarılamadı: {e}"))?;
    Ok(device.finish())
}

/// Çözülen metnin yazı tipi rengi/kalınlığı gibi görsel bilgilerini, aynı
/// konumdaki ham çıkarım nesnesinden kopyalar. Ham metin bozuk olabilir ama
/// burada yalnızca stil alanları kullanılır.
fn copy_decoded_style(target: &mut TextObject, raw: &[TextObject]) {
    let mut best: Option<(f32, &TextObject)> = None;
    for source in raw {
        let dx = source.origin.0 - target.origin.0;
        let dy = source.origin.1 - target.origin.1;
        let distance = (dx * dx + dy * dy).sqrt();
        if distance <= 4.0
            && best
                .as_ref()
                .map(|(best_distance, _)| distance < *best_distance)
                .unwrap_or(true)
        {
            best = Some((distance, source));
        }
    }
    if let Some((_, source)) = best {
        target.font = source.font.clone();
        target.bold = source.bold;
        target.serif = source.serif;
        target.color = source.color.clone();
    }
}

#[derive(Default)]
struct DecodedTextDev {
    objects: Vec<TextObject>,
    line: Option<LineText>,
    origin: (f64, f64),
}

struct LineText {
    text: String,
    start_x: f64,
    end_x: f64,
    baseline: f64,
    size: f64,
    last_end: f64,
    visible: bool,
}

impl LineText {
    fn new(x: f64, y: f64, size: f64, advance: f64) -> Self {
        Self {
            text: String::new(),
            start_x: x,
            end_x: x,
            baseline: y,
            size,
            last_end: x + advance,
            visible: false,
        }
    }
}

impl DecodedTextDev {
    fn flush_line(&mut self) {
        let Some(line) = self.line.take() else {
            return;
        };
        let text = line.text.trim().to_string();
        if text.is_empty() {
            return;
        }
        let size = line.size.max(1.0) as f32;
        let left = line.start_x.min(line.end_x) as f32;
        let right = line.end_x.max(line.start_x + line.size * 0.2) as f32;
        let baseline = line.baseline as f32;
        self.objects.push(TextObject {
            idx: 0,
            text,
            origin: (line.start_x as f32, baseline),
            size,
            font: String::new(),
            bold: false,
            serif: false,
            color: "black".to_string(),
            bounds: (left, baseline - size * 0.30, right, baseline + size * 0.85),
        });
    }

    fn finish(mut self) -> Vec<TextObject> {
        self.flush_line();
        for (idx, object) in self.objects.iter_mut().enumerate() {
            object.idx = idx;
        }
        self.objects
    }
}

impl pdf_extract::OutputDev for DecodedTextDev {
    fn begin_page(
        &mut self,
        _page_num: u32,
        media_box: &MediaBox,
        _art_box: Option<(f64, f64, f64, f64)>,
    ) -> std::result::Result<(), pdf_extract::OutputError> {
        self.objects.clear();
        self.line = None;
        self.origin = (media_box.llx, media_box.lly);
        Ok(())
    }

    fn end_page(&mut self) -> std::result::Result<(), pdf_extract::OutputError> {
        self.flush_line();
        Ok(())
    }

    fn output_character(
        &mut self,
        trm: &pdf_extract::Transform,
        width: f64,
        spacing: f64,
        font_size: f64,
        character: &str,
    ) -> std::result::Result<(), pdf_extract::OutputError> {
        let mut text = String::new();
        for ch in character.chars() {
            text.push(if ch == '\u{00A0}' || ch == '\t' {
                ' '
            } else {
                ch
            });
        }
        if text.is_empty() {
            return Ok(());
        }
        if text.chars().any(|ch| ch == '\n' || ch == '\r') {
            self.flush_line();
            return Ok(());
        }

        let x = trm.m31 - self.origin.0;
        let y = trm.m32 - self.origin.1;
        let horizontal_scale = (trm.m11 * trm.m11 + trm.m12 * trm.m12).sqrt();
        let vertical_scale = (trm.m21 * trm.m21 + trm.m22 * trm.m22).sqrt();
        let size = if font_size.is_finite() && font_size > 0.0 {
            (font_size * vertical_scale).abs().max(1.0)
        } else {
            10.0
        };
        let advance = (width * font_size + spacing) * horizontal_scale;

        let start_new = match &self.line {
            None => true,
            Some(line) => {
                let same_baseline = (y - line.baseline).abs() <= (line.size * 0.55).max(1.0);
                let same_size = (size - line.size).abs() <= (line.size * 0.35).max(1.0);
                let continues = x >= line.start_x - (line.size * 0.6).max(1.0)
                    && x <= line.last_end + (line.size * 2.6).max(2.0);
                !(same_baseline && same_size && continues)
            }
        };
        if start_new {
            self.flush_line();
        }
        if self.line.is_none() {
            self.line = Some(LineText::new(x, y, size, advance));
        }

        if let Some(line) = self.line.as_mut() {
            if line.visible
                && x > line.last_end + (line.size * 0.20).max(0.5)
                && !line.text.ends_with(' ')
            {
                line.text.push(' ');
            }
            for ch in text.chars() {
                if ch == '\u{FFFD}' || (ch.is_control() && ch != ' ') {
                    continue;
                }
                if ch == ' ' {
                    if line.visible && !line.text.ends_with(' ') {
                        line.text.push(' ');
                    }
                    continue;
                }
                if !line.visible {
                    line.start_x = x;
                    line.visible = true;
                }
                line.text.push(ch);
                line.end_x = x + advance;
            }
            line.last_end = x + advance;
        }
        Ok(())
    }

    fn begin_word(&mut self) -> std::result::Result<(), pdf_extract::OutputError> {
        Ok(())
    }

    fn end_word(&mut self) -> std::result::Result<(), pdf_extract::OutputError> {
        Ok(())
    }

    fn end_line(&mut self) -> std::result::Result<(), pdf_extract::OutputError> {
        self.flush_line();
        Ok(())
    }
}

fn extract_raw_text_objects(bytes: &[u8], page_index: usize) -> Result<Vec<TextObject>> {
    let doc = Document::load_mem(bytes)?;
    let pages = doc.get_pages();
    let Some(&page_id) = pages.get(&((page_index + 1) as u32)) else {
        return Ok(Vec::new());
    };
    let content_bytes = doc.get_page_content(page_id);
    let content = Content::decode(&content_bytes)?;
    let mut state = State::default();
    let mut stack: Vec<State> = Vec::new();
    let mut objects = Vec::new();
    let mut idx = 0usize;

    for op in content.operations {
        let operator = op.operator.as_str();
        let operands = &op.operands;
        match operator {
            "q" => stack.push(state.clone()),
            "Q" => {
                if let Some(prev) = stack.pop() {
                    state = prev;
                }
            }
            "BT" => {
                state.tm = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
                state.tlm = state.tm;
            }
            "Tf" => {
                if let Some(name) = operands.first().and_then(|o| o.as_name().ok()) {
                    state.font = String::from_utf8_lossy(name).to_string();
                }
                if let Some(size) = operands.get(1) {
                    state.size = num(size).max(1.0);
                }
            }
            "TL" => {
                if let Some(v) = operands.first() {
                    state.leading = num(v);
                }
            }
            "Tm" if operands.len() >= 6 => {
                state.tm = [
                    num(&operands[0]),
                    num(&operands[1]),
                    num(&operands[2]),
                    num(&operands[3]),
                    num(&operands[4]),
                    num(&operands[5]),
                ];
                state.tlm = state.tm;
            }
            "Td" | "TD" if operands.len() >= 2 => {
                let tx = num(&operands[0]);
                let ty = num(&operands[1]);
                state.tlm[4] += tx;
                state.tlm[5] += ty;
                state.tm = state.tlm;
                if operator == "TD" {
                    state.leading = -ty;
                }
            }
            "T*" => {
                state.tlm[5] -= state.leading;
                state.tm = state.tlm;
            }
            "Tj" => {
                if let Some(obj) = operands.first() {
                    if let Ok(s) = obj.as_str() {
                        let text = decode_pdf_string(s);
                        push_text(&mut objects, &mut state, &text, &mut idx);
                    }
                }
            }
            "'" => {
                state.tlm[5] -= state.leading;
                state.tm = state.tlm;
                if let Some(obj) = operands.first() {
                    if let Ok(s) = obj.as_str() {
                        let text = decode_pdf_string(s);
                        push_text(&mut objects, &mut state, &text, &mut idx);
                    }
                }
            }
            "\"" => {
                if operands.len() >= 3 {
                    state.tlm[5] -= state.leading;
                    state.tm = state.tlm;
                    if let Ok(s) = operands[2].as_str() {
                        let text = decode_pdf_string(s);
                        push_text(&mut objects, &mut state, &text, &mut idx);
                    }
                }
            }
            "TJ" => {
                if let Some(Object::Array(items)) = operands.first() {
                    let mut text = String::new();
                    for item in items {
                        if let Ok(s) = item.as_str() {
                            text.push_str(&decode_pdf_string(s));
                        } else if let Ok(n) = item.as_float() {
                            if n < -180.0 {
                                text.push(' ');
                            }
                        }
                    }
                    push_text(&mut objects, &mut state, &text, &mut idx);
                }
            }
            _ => {}
        }
        state_fill_ops(&mut state, operator, operands);
    }
    Ok(objects)
}

fn push_text(objects: &mut Vec<TextObject>, state: &mut State, text: &str, idx: &mut usize) {
    if text.trim().is_empty() {
        if !text.is_empty() {
            state.tm[4] += estimate_width(text, state.size);
        }
        return;
    }
    let size = state.size;
    let w = estimate_width(text, size);
    let x = state.tm[4];
    let y = state.tm[5];
    let bounds = (x, y - size * 0.28, x + w, y + size * 0.82);
    let fname = state.font.to_ascii_lowercase();
    let bold = fname.contains("bold") || fname.contains("black") || fname.contains("heavy");
    let serif = fname.contains("times")
        || fname.contains("serif")
        || fname.contains("georgia")
        || fname.contains("garamond")
        || fname.contains("book");
    objects.push(TextObject {
        idx: *idx,
        text: text.to_string(),
        origin: (x, y),
        size,
        font: state.font.clone(),
        bold,
        serif,
        color: state.fill.clone(),
        bounds,
    });
    *idx += 1;
    state.tm[4] += w;
}

fn estimate_width(text: &str, size: f32) -> f32 {
    let mut w = 0.0;
    for ch in text.chars() {
        w += match ch {
            'i' | 'l' | 'ı' | 'I' | '.' | ',' | '!' | '\'' => 0.30,
            'm' | 'w' | 'M' | 'W' | 'Ş' | 'Ğ' => 0.86,
            ' ' => 0.28,
            _ => 0.56,
        } * size;
    }
    w
}

pub fn find_text_at(objects: &[TextObject], x: f32, y: f32) -> Option<TextObject> {
    let mut best: Option<(f32, TextObject)> = None;
    for obj in objects {
        let (l, b, r, t) = obj.bounds;
        if x >= l - 2.0 && x <= r + 2.0 && y >= b - 2.0 && y <= t + 2.0 {
            let area = (r - l).max(1.0) * (t - b).max(1.0);
            if best.as_ref().map(|(a, _)| area < *a).unwrap_or(true) {
                best = Some((area, obj.clone()));
            }
        }
    }
    best.map(|(_, o)| o)
}

pub fn sample_background(
    rgba: &[u8],
    px_w: u32,
    px_h: u32,
    page_w: f32,
    page_h: f32,
    bounds: (f32, f32, f32, f32),
) -> String {
    if px_w == 0 || px_h == 0 || rgba.len() < 4 {
        return "#FFFFFF".to_string();
    }
    let (l, _b, _r, t) = bounds;
    let mut x = ((l / page_w.max(1.0)) * px_w as f32 - 3.0).round() as i32;
    let mut y = (((page_h - t) / page_h.max(1.0)) * px_h as f32 - 3.0).round() as i32;
    x = x.clamp(0, px_w as i32 - 1);
    y = y.clamp(0, px_h as i32 - 1);
    let i = ((y as u32 * px_w + x as u32) * 4) as usize;
    if i + 3 < rgba.len() {
        format!("#{:02X}{:02X}{:02X}", rgba[i], rgba[i + 1], rgba[i + 2])
    } else {
        "#FFFFFF".to_string()
    }
}

#[allow(dead_code)]
pub fn text_objects_by_page(bytes: &[u8]) -> Result<HashMap<u32, Vec<TextObject>>> {
    let doc = Document::load_mem(bytes)?;
    let mut map = HashMap::new();
    for page in doc.get_pages().keys() {
        if let Ok(objs) = extract_text_objects(bytes, (*page - 1) as usize) {
            map.insert(*page, objs);
        }
    }
    Ok(map)
}
