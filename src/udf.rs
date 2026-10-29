use anyhow::{anyhow, Context, Result};
use base64::Engine as _;
use printpdf::{
    Mm, Op, PdfDocument, PdfFontHandle, PdfPage, PdfSaveOptions, Point, Pt, Rgb, TextItem,
    XObjectTransform,
};
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

const A4_W: f32 = 595.28;
const A4_H: f32 = 841.89;

#[derive(Clone, Default)]
struct Node {
    name: String,
    attrs: HashMap<String, String>,
    text: String,
    children: Vec<Node>,
}

impl Node {
    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs.get(key).map(|s| s.as_str())
    }
    fn fattr(&self, key: &str, default: f32) -> f32 {
        self.attr(key)
            .and_then(|v| v.trim().replace(',', ".").parse().ok())
            .unwrap_or(default)
    }
    fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }
    fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }
    fn descendants_named<'a>(&'a self, name: &'a str, out: &mut Vec<&'a Node>) {
        for c in &self.children {
            if c.name == name {
                out.push(c);
            }
            c.descendants_named(name, out);
        }
    }
}

fn local_name(raw: &[u8]) -> String {
    let s = String::from_utf8_lossy(raw);
    s.rsplit(':').next().unwrap_or(&s).to_ascii_lowercase()
}

fn parse_xml(bytes: &[u8]) -> Result<Node> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut stack: Vec<Node> = Vec::new();
    let mut root: Option<Node> = None;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let mut node = Node {
                    name: local_name(e.name().as_ref()),
                    ..Default::default()
                };
                for a in e.attributes().flatten() {
                    let key = local_name(a.key.as_ref());
                    let val = a
                        .unescape_value()
                        .map(|v| v.to_string())
                        .unwrap_or_else(|_| String::from_utf8_lossy(&a.value).to_string());
                    node.attrs.insert(key, val);
                }
                stack.push(node);
            }
            Ok(Event::Empty(e)) => {
                let mut node = Node {
                    name: local_name(e.name().as_ref()),
                    ..Default::default()
                };
                for a in e.attributes().flatten() {
                    let key = local_name(a.key.as_ref());
                    let val = a
                        .unescape_value()
                        .map(|v| v.to_string())
                        .unwrap_or_else(|_| String::from_utf8_lossy(&a.value).to_string());
                    node.attrs.insert(key, val);
                }
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else if root.is_none() {
                    root = Some(node);
                }
            }
            Ok(Event::Text(t)) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&t.unescape().unwrap_or_default());
                }
            }
            Ok(Event::CData(t)) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&String::from_utf8_lossy(&t));
                }
            }
            Ok(Event::End(_)) => {
                if let Some(node) = stack.pop() {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(node);
                    } else if root.is_none() {
                        root = Some(node);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(anyhow!("UDF XML ayrıştırılamadı: {e}")),
            _ => {}
        }
        buf.clear();
    }
    root.ok_or_else(|| anyhow!("UDF içeriği boş."))
}

fn read_udf_xml(src: &Path) -> Result<Vec<u8>> {
    let bytes =
        std::fs::read(src).with_context(|| format!("Dosya okunamadı: {}", src.display()))?;
    if bytes.starts_with(b"PK") {
        let cursor = std::io::Cursor::new(bytes);
        let mut zip = zip::ZipArchive::new(cursor).context("UDF arşivi açılamadı")?;
        let names: Vec<String> = zip.file_names().map(|s| s.to_string()).collect();
        let name = names
            .iter()
            .find(|n| n.to_ascii_lowercase().ends_with("content.xml"))
            .cloned()
            .ok_or_else(|| anyhow!("Geçersiz UDF: içinde content.xml bulunamadı."))?;
        let mut file = zip.by_name(&name)?;
        let mut out = Vec::new();
        file.read_to_end(&mut out)?;
        Ok(out)
    } else {
        Ok(bytes)
    }
}

#[derive(Clone, Debug)]
struct Style {
    size: f32,
    bold: bool,
    serif: bool,
    italic: bool,
    underline: bool,
    color: [u8; 3],
}

impl Default for Style {
    fn default() -> Self {
        Self {
            size: 12.0,
            bold: false,
            serif: false,
            italic: false,
            underline: false,
            color: [0, 0, 0],
        }
    }
}

impl PartialEq for Style {
    fn eq(&self, other: &Self) -> bool {
        self.size.to_bits() == other.size.to_bits()
            && self.bold == other.bold
            && self.serif == other.serif
            && self.italic == other.italic
            && self.underline == other.underline
            && self.color == other.color
    }
}

#[derive(Clone, Debug)]
struct Segment {
    text: String,
    style: Style,
}

fn parse_color(v: Option<&str>) -> [u8; 3] {
    let Some(v) = v else {
        return [0, 0, 0];
    };
    let s = v.trim().trim_start_matches('#');
    if s.len() >= 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&s[0..2], 16),
            u8::from_str_radix(&s[2..4], 16),
            u8::from_str_radix(&s[4..6], 16),
        ) {
            return [r, g, b];
        }
    }
    match s.to_ascii_lowercase().as_str() {
        "black" => [0, 0, 0],
        "white" => [255, 255, 255],
        "red" => [255, 0, 0],
        "blue" => [0, 0, 255],
        "green" => [0, 128, 0],
        "gray" | "grey" => [128, 128, 128],
        _ => [0, 0, 0],
    }
}

fn char_width(ch: char, size: f32) -> f32 {
    (match ch {
        'i' | 'l' | 'ı' | 'I' | '.' | ',' | '!' | '\'' | '|' => 0.30,
        'm' | 'w' | 'M' | 'W' | 'Ş' | 'Ğ' => 0.86,
        ' ' => 0.28,
        _ => 0.56,
    }) * size
}

fn text_width(s: &str, size: f32) -> f32 {
    s.chars().map(|c| char_width(c, size)).sum()
}

fn wrap_chars(chars: &[(char, Style)], max_width: f32) -> Vec<Vec<(char, Style)>> {
    let mut lines: Vec<Vec<(char, Style)>> = Vec::new();
    let mut line: Vec<(char, Style)> = Vec::new();
    let mut width = 0.0f32;
    for (ch, style) in chars {
        if *ch == '\n' {
            lines.push(std::mem::take(&mut line));
            width = 0.0;
            continue;
        }
        if *ch == '\r' {
            continue;
        }
        let cw = char_width(*ch, style.size);
        if width + cw > max_width && !line.is_empty() {
            if let Some(pos) = line.iter().rposition(|(c, _)| *c == ' ') {
                let rest = line.split_off(pos + 1);
                while line.last().map(|(c, _)| *c == ' ').unwrap_or(false) {
                    line.pop();
                }
                lines.push(std::mem::take(&mut line));
                line = rest;
                width = line.iter().map(|(c, s)| char_width(*c, s.size)).sum();
            } else {
                lines.push(std::mem::take(&mut line));
                width = 0.0;
            }
        }
        line.push((*ch, style.clone()));
        width += cw;
    }
    lines.push(line);
    lines
}

fn to_segments(chars: Vec<(char, Style)>) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    for (ch, style) in chars {
        if let Some(last) = out.last_mut() {
            if last.style == style {
                last.text.push(ch);
                continue;
            }
        }
        out.push(Segment {
            text: ch.to_string(),
            style,
        });
    }
    out
}

struct Builder {
    width: f32,
    height: f32,
    margin_l: f32,
    margin_r: f32,
    margin_t: f32,
    margin_b: f32,
    y: f32,
    ops: Vec<Op>,
    pages: Vec<PdfPage>,
    sans: printpdf::FontId,
    sans_bold: printpdf::FontId,
    serif: printpdf::FontId,
    serif_bold: printpdf::FontId,
    image_warnings: Vec<printpdf::PdfWarnMsg>,
}

impl Builder {
    fn content_width(&self) -> f32 {
        self.width - self.margin_l - self.margin_r
    }

    fn font(&self, style: &Style) -> printpdf::FontId {
        match (style.serif, style.bold) {
            (false, false) => self.sans.clone(),
            (false, true) => self.sans_bold.clone(),
            (true, false) => self.serif.clone(),
            (true, true) => self.serif_bold.clone(),
        }
    }

    fn new_page(&mut self) {
        let ops = std::mem::take(&mut self.ops);
        self.pages
            .push(PdfPage::new(Mm(mm(self.width)), Mm(mm(self.height)), ops));
        self.y = self.height - self.margin_t;
    }

    fn ensure(&mut self, needed: f32) {
        if self.y - needed < self.margin_b {
            self.new_page();
        }
    }

    fn draw_text(
        ops: &mut Vec<Op>,
        text: &str,
        x: f32,
        y: f32,
        style: &Style,
        font: printpdf::FontId,
    ) {
        if text.is_empty() {
            return;
        }
        ops.push(Op::StartTextSection);
        ops.push(Op::SetFillColor {
            col: printpdf::Color::Rgb(Rgb {
                r: style.color[0] as f32 / 255.0,
                g: style.color[1] as f32 / 255.0,
                b: style.color[2] as f32 / 255.0,
                icc_profile: None,
            }),
        });
        ops.push(Op::SetFont {
            font: PdfFontHandle::External(font),
            size: Pt(style.size),
        });
        ops.push(Op::SetTextCursor {
            pos: Point { x: Pt(x), y: Pt(y) },
        });
        ops.push(Op::ShowText {
            items: vec![TextItem::Text(text.to_string())],
        });
        ops.push(Op::EndTextSection);
    }

    fn draw_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, lw: f32) {
        self.ops.push(Op::SetOutlineColor {
            col: printpdf::Color::Rgb(Rgb {
                r: 0.5,
                g: 0.5,
                b: 0.5,
                icc_profile: None,
            }),
        });
        self.ops.push(Op::SetOutlineThickness { pt: Pt(lw) });
        self.ops.push(Op::DrawLine {
            line: printpdf::Line {
                points: vec![
                    printpdf::LinePoint {
                        p: Point {
                            x: Pt(x1),
                            y: Pt(y1),
                        },
                        bezier: false,
                    },
                    printpdf::LinePoint {
                        p: Point {
                            x: Pt(x2),
                            y: Pt(y2),
                        },
                        bezier: false,
                    },
                ],
                is_closed: false,
            },
        });
    }

    fn draw_rect(&mut self, x: f32, y: f32, w: f32, h: f32, lw: f32) {
        self.ops.push(Op::SetOutlineColor {
            col: printpdf::Color::Rgb(Rgb {
                r: 0.55,
                g: 0.55,
                b: 0.55,
                icc_profile: None,
            }),
        });
        self.ops.push(Op::SetOutlineThickness { pt: Pt(lw) });
        self.ops.push(Op::DrawRectangle {
            rectangle: printpdf::Rect {
                x: Pt(x),
                y: Pt(y),
                width: Pt(w),
                height: Pt(h),
                mode: Some(printpdf::PaintMode::Stroke),
                winding_order: Some(printpdf::WindingOrder::NonZero),
            },
        });
    }

    fn paragraph(
        &mut self,
        runs: Vec<Segment>,
        align: u8,
        left_indent: f32,
        right_indent: f32,
        first_indent: f32,
        space_before: f32,
        space_after: f32,
    ) {
        let base_size = runs.iter().map(|r| r.style.size).fold(12.0f32, f32::max);
        let leading = base_size * 1.32;
        self.y -= space_before.max(0.0);
        self.ensure(leading);
        let max_width = (self.content_width() - left_indent - right_indent).max(40.0);

        let mut chars: Vec<(char, Style)> = Vec::new();
        if runs.is_empty() {
            // keep an empty line
            chars.push(('\n', Style::default()));
        }
        for r in &runs {
            for ch in r.text.chars() {
                chars.push((ch, r.style.clone()));
            }
        }
        let lines = wrap_chars(&chars, max_width);
        let mut first = true;
        for line in lines {
            self.ensure(leading);
            let segments = to_segments(line);
            let line_width: f32 = segments
                .iter()
                .map(|s| text_width(&s.text, s.style.size))
                .sum();
            let extra_indent = if first { first_indent } else { 0.0 };
            let mut x = self.margin_l + left_indent + extra_indent;
            if align == 1 {
                x = self.margin_l
                    + left_indent
                    + ((max_width - line_width) / 2.0).max(0.0)
                    + extra_indent;
            } else if align == 2 {
                x = self.margin_l + left_indent + (max_width - line_width).max(0.0);
            }
            let baseline = self.y - base_size * 0.92;
            let start_x = x;
            for seg in &segments {
                let w = text_width(&seg.text, seg.style.size);
                let font = self.font(&seg.style);
                Self::draw_text(&mut self.ops, &seg.text, x, baseline, &seg.style, font);
                if seg.style.underline {
                    self.draw_line(
                        x,
                        baseline - seg.style.size * 0.12,
                        x + w,
                        baseline - seg.style.size * 0.12,
                        0.6,
                    );
                }
                x += w;
            }
            if align == 3 && line_width < max_width && !segments.is_empty() {
                let _ = start_x;
            }
            self.y -= leading;
            first = false;
        }
        self.y -= space_after.max(0.0);
    }

    fn table(&mut self, rows: Vec<Vec<Vec<Segment>>>, content_width: f32) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let cols = rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
        let col_w = content_width / cols as f32;
        let pad = 3.0f32;
        let base_size = 10.5f32;
        let leading = base_size * 1.3;
        for row in rows {
            let mut wrapped: Vec<Vec<Vec<Segment>>> = Vec::new();
            let mut max_lines = 1usize;
            for c in 0..cols {
                let runs = row.get(c).cloned().unwrap_or_default();
                let chars: Vec<(char, Style)> = if runs.is_empty() {
                    Vec::new()
                } else {
                    runs.iter()
                        .flat_map(|r| r.text.chars().map(move |ch| (ch, r.style.clone())))
                        .collect()
                };
                let lines = wrap_chars(&chars, (col_w - 2.0 * pad).max(10.0));
                max_lines = max_lines.max(lines.len());
                wrapped.push(lines.into_iter().map(to_segments).collect());
            }
            let row_h = max_lines as f32 * leading + 2.0 * pad;
            self.ensure(row_h);
            let top = self.y;
            for (c, cell_lines) in wrapped.iter().enumerate() {
                let x = self.margin_l + c as f32 * col_w;
                self.draw_rect(x, top - row_h, col_w, row_h, 0.5);
                let mut baseline = top - pad - base_size * 0.92;
                for segs in cell_lines {
                    let mut tx = x + pad;
                    for seg in segs {
                        let font = self.font(&seg.style);
                        Self::draw_text(&mut self.ops, &seg.text, tx, baseline, &seg.style, font);
                        tx += text_width(&seg.text, seg.style.size);
                    }
                    baseline -= leading;
                }
            }
            self.y -= row_h;
        }
        self.y -= 6.0;
        Ok(())
    }
}

fn mm(v: f32) -> f32 {
    v / (72.0 / 25.4)
}

fn paragraph_runs(
    node: &Node,
    text: &str,
) -> (Vec<Segment>, u8, f32, f32, f32, f32, f32, Vec<Node>) {
    let align = node
        .attr("alignment")
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(0);
    let left_indent = node.fattr("leftindent", 0.0);
    let right_indent = node.fattr("rightindent", 0.0);
    let first_indent = node.fattr("firstlineindent", 0.0);
    let space_before = node.fattr("spaceabove", 0.0);
    let space_after = node.fattr("spacebelow", 0.0);
    let mut runs = Vec::new();
    let mut images = Vec::new();
    for child in &node.children {
        match child.name.as_str() {
            "content" => {
                let start = child
                    .attr("startOffset")
                    .or_else(|| child.attr("startoffset"))
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0);
                let len = child
                    .attr("length")
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0);
                let piece: String = text.chars().skip(start).take(len).collect();
                let piece = piece.replace(['\n', '\r'], "");
                if piece.is_empty() {
                    continue;
                }
                let mut style = Style {
                    size: child.fattr("size", 12.0),
                    bold: child.attr("bold").map(is_true).unwrap_or(false),
                    italic: child.attr("italic").map(is_true).unwrap_or(false),
                    underline: child.attr("underline").map(is_true).unwrap_or(false),
                    color: parse_color(
                        child
                            .attr("color")
                            .or_else(|| child.attr("forecolor"))
                            .or_else(|| node.attr("color")),
                    ),
                    ..Default::default()
                };
                if style.size <= 0.0 {
                    style.size = 12.0;
                }
                runs.push(Segment { text: piece, style });
            }
            "tab" => runs.push(Segment {
                text: "        ".to_string(),
                style: Style {
                    size: node.fattr("size", 12.0),
                    ..Default::default()
                },
            }),
            "space" => runs.push(Segment {
                text: " ".to_string(),
                style: Style {
                    size: node.fattr("size", 12.0),
                    ..Default::default()
                },
            }),
            "image" => images.push(child.clone()),
            _ => {}
        }
    }
    (
        runs,
        align,
        left_indent,
        right_indent,
        first_indent,
        space_before,
        space_after,
        images,
    )
}

fn is_true(v: &str) -> bool {
    matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes")
}

fn cell_runs(cell: &Node, text: &str) -> Vec<Segment> {
    let mut paragraphs: Vec<&Node> = Vec::new();
    if cell.name == "paragraph" {
        paragraphs.push(cell);
    }
    cell.descendants_named("paragraph", &mut paragraphs);
    let mut runs = Vec::new();
    for (i, p) in paragraphs.iter().enumerate() {
        if i > 0 {
            runs.push(Segment {
                text: "\n".to_string(),
                style: Style::default(),
            });
        }
        let (mut r, ..) = paragraph_runs(p, text);
        runs.append(&mut r);
    }
    if runs.is_empty() {
        for child in &cell.children {
            if child.name == "content" {
                let (mut r, ..) = paragraph_runs(cell, text);
                runs.append(&mut r);
                break;
            }
        }
    }
    runs
}

pub fn udf_to_pdf(src: &Path, out: &Path) -> Result<()> {
    let xml = read_udf_xml(src)?;
    let root = parse_xml(&xml)?;
    let content_text = root
        .child("content")
        .map(|c| c.text.clone())
        .unwrap_or_default();

    let page_format = root.child("properties").and_then(|p| p.child("pageformat"));
    let (ml, mr, mt, mb) = if let Some(pf) = page_format {
        (
            pf.fattr("leftmargin", 42.5),
            pf.fattr("rightmargin", 42.5),
            pf.fattr("topmargin", 42.5),
            pf.fattr("bottommargin", 42.5),
        )
    } else {
        (42.5, 42.5, 42.5, 42.5)
    };

    let fonts = crate::fonts::FontSet::load()?;
    let mut doc = PdfDocument::new("UDF");
    let sans = doc.add_font(&fonts.sans);
    let sans_bold = doc.add_font(&fonts.sans_bold);
    let serif = doc.add_font(&fonts.serif);
    let serif_bold = doc.add_font(&fonts.serif_bold);

    let mut b = Builder {
        width: A4_W,
        height: A4_H,
        margin_l: ml,
        margin_r: mr,
        margin_t: mt,
        margin_b: mb,
        y: A4_H - mt,
        ops: Vec::new(),
        pages: Vec::new(),
        sans,
        sans_bold,
        serif,
        serif_bold,
        image_warnings: Vec::new(),
    };

    let mut images_to_add: Vec<(printpdf::RawImage, printpdf::XObjectId)> = Vec::new();
    let elements = root.child("elements");
    let mut had_content = false;
    if let Some(elements) = elements {
        for el in &elements.children {
            match el.name.as_str() {
                "paragraph" => {
                    had_content = true;
                    let (runs, align, li, ri, fi, sb, sa, imgs) = paragraph_runs(el, &content_text);
                    b.paragraph(runs, align, li, ri, fi, sb, sa);
                    for img in imgs {
                        draw_udf_image(&mut b, &mut images_to_add, &img)?;
                    }
                }
                "table" => {
                    had_content = true;
                    let mut rows: Vec<Vec<Vec<Segment>>> = Vec::new();
                    for row in el.children_named("row") {
                        let mut cells = Vec::new();
                        for cell in row.children_named("cell") {
                            cells.push(cell_runs(cell, &content_text));
                        }
                        rows.push(cells);
                    }
                    b.table(rows, b.content_width())?;
                }
                "page-break" | "pagebreak" => {
                    b.new_page();
                }
                "image" => {
                    had_content = true;
                    draw_udf_image(&mut b, &mut images_to_add, el)?;
                }
                _ => {}
            }
        }
    }
    if !had_content && !content_text.trim().is_empty() {
        for line in content_text.replace('\r', "").split('\n') {
            b.paragraph(
                vec![Segment {
                    text: line.to_string(),
                    style: Style::default(),
                }],
                1,
                0.0,
                0.0,
                0.0,
                0.0,
                6.0,
            );
        }
    }
    if b.pages.is_empty() && b.ops.is_empty() {
        return Err(anyhow!("UDF içinde metin bulunamadı."));
    }
    if !b.ops.is_empty() {
        b.new_page();
    }
    let mut pages = b.pages;
    // Register queued images into the PDF document and fix up XObject references.
    for (img, id) in images_to_add {
        let real = doc.add_image(&img);
        remap_xobject(&mut pages, id, real);
    }
    doc.with_pages(pages);
    let mut warnings = Vec::new();
    let bytes = doc.save(&PdfSaveOptions::default(), &mut warnings);
    std::fs::write(out, bytes)?;
    Ok(())
}

fn draw_udf_image(
    b: &mut Builder,
    queue: &mut Vec<(printpdf::RawImage, printpdf::XObjectId)>,
    node: &Node,
) -> Result<()> {
    let data = node
        .attr("imagedata")
        .or_else(|| node.attr("data"))
        .unwrap_or("");
    if data.trim().is_empty() {
        return Ok(());
    }
    let cleaned: String = data.chars().filter(|c| !c.is_whitespace()).collect();
    let raw = base64::engine::general_purpose::STANDARD
        .decode(cleaned)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(data.trim()))?;
    let decoded = image::load_from_memory(&raw)?;
    let (gw, gh) = (decoded.width() as f32, decoded.height() as f32);
    let mut w = node.fattr("width", gw);
    let mut h = node.fattr("height", gh);
    if w <= 1.0 {
        w = gw;
    }
    if h <= 1.0 {
        h = gh;
    }
    let fit = (b.content_width() / w).min(650.0 / h).min(1.0);
    w *= fit;
    h *= fit;
    b.ensure(h + 6.0);
    let rawi = printpdf::RawImage::decode_from_bytes(&raw, &mut b.image_warnings)
        .map_err(|e| anyhow!("UDF görseli açılamadı: {e}"))?;
    let nw = rawi.width.max(1) as f32;
    let nh = rawi.height.max(1) as f32;
    let placeholder = printpdf::XObjectId(format!("PTIMG{}", queue.len()));
    queue.push((rawi, placeholder.clone()));
    let x = b.margin_l;
    let y = b.y - h;
    b.ops.push(Op::UseXobject {
        id: placeholder,
        transform: XObjectTransform {
            translate_x: Some(Pt(x)),
            translate_y: Some(Pt(y)),
            scale_x: Some(w / nw),
            scale_y: Some(h / nh),
            dpi: Some(72.0),
            ..Default::default()
        },
    });
    b.y -= h + 6.0;
    Ok(())
}

fn remap_xobject(pages: &mut [PdfPage], old: printpdf::XObjectId, new: printpdf::XObjectId) {
    for page in pages.iter_mut() {
        for op in page.ops.iter_mut() {
            if let Op::UseXobject { id, .. } = op {
                if *id == old {
                    *id = new.clone();
                }
            }
        }
    }
}
