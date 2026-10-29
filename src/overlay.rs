use anyhow::{anyhow, Result};
use printpdf::{
    CurTransMat, ExtendedGraphicsState, Line, LinePoint, Mm, Op, PaintMode, ParsedFont,
    PdfDocument, PdfFontHandle, PdfPage, PdfSaveOptions, Point, Polygon, PolygonRing, Pt, RawImage,
    Rect, Rgb, TextItem, WindingOrder, XObjectTransform,
};

use crate::fonts::FontSet;
use crate::model::{palette_color, EditAction, EditItem, WatermarkMode};

const PT_PER_MM: f32 = 72.0 / 25.4;

pub fn pt_to_mm(v: f32) -> f32 {
    v / PT_PER_MM
}

pub fn estimate_text_width(text: &str, size: f32) -> f32 {
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

fn color(name: &str) -> printpdf::Color {
    let [r, g, b] = palette_color(name);
    printpdf::Color::Rgb(Rgb {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        icc_profile: None,
    })
}

fn set_fill(ops: &mut Vec<Op>, name: &str) {
    ops.push(Op::SetFillColor { col: color(name) });
}

fn set_stroke(ops: &mut Vec<Op>, name: &str) {
    ops.push(Op::SetOutlineColor { col: color(name) });
}

fn text_ops(
    ops: &mut Vec<Op>,
    text: &str,
    x: f32,
    y: f32,
    size: f32,
    font_id: printpdf::FontId,
    col: &str,
) {
    if text.is_empty() {
        return;
    }
    ops.push(Op::StartTextSection);
    set_fill(ops, col);
    ops.push(Op::SetFont {
        font: PdfFontHandle::External(font_id),
        size: Pt(size),
    });
    ops.push(Op::SetLineHeight { lh: Pt(size * 1.2) });
    ops.push(Op::SetTextCursor {
        pos: Point { x: Pt(x), y: Pt(y) },
    });
    ops.push(Op::ShowText {
        items: vec![TextItem::Text(text.to_string())],
    });
    ops.push(Op::EndTextSection);
}

fn rect_ops(ops: &mut Vec<Op>, x: f32, y: f32, w: f32, h: f32, fill: bool, col: &str, lw: f32) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    set_fill(ops, col);
    set_stroke(ops, col);
    ops.push(Op::SetOutlineThickness {
        pt: Pt(lw.max(0.1)),
    });
    ops.push(Op::DrawRectangle {
        rectangle: Rect {
            x: Pt(x),
            y: Pt(y),
            width: Pt(w),
            height: Pt(h),
            mode: Some(if fill {
                PaintMode::Fill
            } else {
                PaintMode::Stroke
            }),
            winding_order: Some(WindingOrder::NonZero),
        },
    });
}

fn circle_ops(ops: &mut Vec<Op>, cx: f32, cy: f32, r: f32, fill: bool, col: &str, lw: f32) {
    if r <= 0.0 {
        return;
    }
    set_fill(ops, col);
    set_stroke(ops, col);
    ops.push(Op::SetOutlineThickness {
        pt: Pt(lw.max(0.1)),
    });
    let mut points = Vec::with_capacity(65);
    for i in 0..64 {
        let a = std::f32::consts::TAU * (i as f32) / 64.0;
        points.push(LinePoint {
            p: Point {
                x: Pt(cx + r * a.cos()),
                y: Pt(cy + r * a.sin()),
            },
            bezier: false,
        });
    }
    ops.push(Op::DrawPolygon {
        polygon: Polygon {
            rings: vec![PolygonRing { points }],
            mode: if fill {
                PaintMode::Fill
            } else {
                PaintMode::Stroke
            },
            winding_order: WindingOrder::NonZero,
        },
    });
}

fn line_ops(ops: &mut Vec<Op>, x1: f32, y1: f32, x2: f32, y2: f32, col: &str, lw: f32) {
    set_stroke(ops, col);
    ops.push(Op::SetOutlineThickness {
        pt: Pt(lw.max(0.1)),
    });
    ops.push(Op::DrawLine {
        line: Line {
            points: vec![
                LinePoint {
                    p: Point {
                        x: Pt(x1),
                        y: Pt(y1),
                    },
                    bezier: false,
                },
                LinePoint {
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

fn new_page(width: f32, height: f32, ops: Vec<Op>) -> PdfPage {
    PdfPage::new(Mm(pt_to_mm(width)), Mm(pt_to_mm(height)), ops)
}

pub fn build_overlay(
    width: f32,
    height: f32,
    edits: &[EditItem],
    fonts: &FontSet,
) -> Result<Vec<u8>> {
    let width = width.max(1.0);
    let height = height.max(1.0);
    let mut doc = PdfDocument::new("PDF Toolbox Overlay");
    let sans = doc.add_font(&fonts.sans);
    let sans_bold = doc.add_font(&fonts.sans_bold);
    let serif = doc.add_font(&fonts.serif);
    let serif_bold = doc.add_font(&fonts.serif_bold);
    let mut ops: Vec<Op> = Vec::new();
    let mut warnings = Vec::new();

    for item in edits {
        match &item.action {
            EditAction::Text {
                x,
                y,
                text,
                size,
                color,
                serif: is_serif,
                bold,
            } => {
                let fid = if *is_serif {
                    if *bold {
                        serif_bold.clone()
                    } else {
                        serif.clone()
                    }
                } else if *bold {
                    sans_bold.clone()
                } else {
                    sans.clone()
                };
                text_ops(&mut ops, text, *x, *y, *size, fid, color);
            }
            EditAction::Rectangle {
                x,
                y,
                w,
                h,
                fill,
                color,
                lw,
            } => rect_ops(&mut ops, *x, *y, *w, *h, *fill, color, *lw),
            EditAction::Circle {
                cx,
                cy,
                r,
                fill,
                color,
                lw,
            } => circle_ops(&mut ops, *cx, *cy, *r, *fill, color, *lw),
            EditAction::Line {
                x1,
                y1,
                x2,
                y2,
                color,
                width,
            } => line_ops(&mut ops, *x1, *y1, *x2, *y2, color, *width),
            EditAction::Image { path, x, y, w, h } => {
                let bytes = std::fs::read(path)?;
                if let Ok(img) = RawImage::decode_from_bytes(&bytes, &mut warnings) {
                    let nw = img.width.max(1) as f32;
                    let nh = img.height.max(1) as f32;
                    let id = doc.add_image(&img);
                    ops.push(Op::UseXobject {
                        id,
                        transform: XObjectTransform {
                            translate_x: Some(Pt(*x)),
                            translate_y: Some(Pt(*y)),
                            scale_x: Some(w / nw),
                            scale_y: Some(h / nh),
                            dpi: Some(72.0),
                            ..Default::default()
                        },
                    });
                }
            }
            EditAction::Watermark {
                text,
                mode,
                size,
                alpha,
                color,
            } => {
                let gs = doc.add_graphics_state(
                    ExtendedGraphicsState::default()
                        .with_current_fill_alpha(alpha.clamp(0.01, 1.0)),
                );
                ops.push(Op::SaveGraphicsState);
                ops.push(Op::LoadGraphicsState { gs });
                set_fill(&mut ops, color);
                ops.push(Op::SetFont {
                    font: PdfFontHandle::External(sans_bold.clone()),
                    size: Pt(*size),
                });
                ops.push(Op::SetLineHeight {
                    lh: Pt(*size * 1.2),
                });
                let tw = estimate_text_width(text, *size);
                match mode {
                    WatermarkMode::Horizontal => {
                        ops.push(Op::SetTextCursor {
                            pos: Point {
                                x: Pt((width - tw) / 2.0),
                                y: Pt(height / 2.0 - size / 3.0),
                            },
                        });
                        ops.push(Op::ShowText {
                            items: vec![TextItem::Text(text.clone())],
                        });
                    }
                    WatermarkMode::Diagonal => {
                        ops.push(Op::SetTransformationMatrix {
                            matrix: CurTransMat::Translate(Pt(width / 2.0), Pt(height / 2.0)),
                        });
                        ops.push(Op::SetTransformationMatrix {
                            matrix: CurTransMat::Rotate(45.0),
                        });
                        ops.push(Op::SetTextCursor {
                            pos: Point {
                                x: Pt(-tw / 2.0),
                                y: Pt(-size / 3.0),
                            },
                        });
                        ops.push(Op::ShowText {
                            items: vec![TextItem::Text(text.clone())],
                        });
                    }
                    WatermarkMode::Repeated => {
                        let diag = (width * width + height * height).sqrt() / 2.0 + *size * 2.0;
                        let step_x = tw + size * 2.0;
                        let step_y = size * 4.0;
                        ops.push(Op::SetTransformationMatrix {
                            matrix: CurTransMat::Translate(Pt(width / 2.0), Pt(height / 2.0)),
                        });
                        ops.push(Op::SetTransformationMatrix {
                            matrix: CurTransMat::Rotate(45.0),
                        });
                        let mut y = -diag;
                        let mut row = 0;
                        while y < diag {
                            let mut x = -diag + if row % 2 == 1 { step_x / 2.0 } else { 0.0 };
                            while x < diag {
                                ops.push(Op::SetTextCursor {
                                    pos: Point { x: Pt(x), y: Pt(y) },
                                });
                                ops.push(Op::ShowText {
                                    items: vec![TextItem::Text(text.clone())],
                                });
                                x += step_x;
                            }
                            y += step_y;
                            row += 1;
                        }
                    }
                }
                ops.push(Op::RestoreGraphicsState);
            }
            EditAction::TextEdit {
                bounds,
                new,
                size,
                color,
                bg,
                origin,
                serif: is_serif,
                bold,
                ..
            } => {
                let (l, b, r, t) = *bounds;
                rect_ops(
                    &mut ops,
                    l - 1.0,
                    b - 1.0,
                    (r - l).max(1.0) + 2.0,
                    (t - b).max(1.0) + 2.0,
                    true,
                    bg,
                    0.1,
                );
                if !new.is_empty() {
                    let fid = if *is_serif {
                        if *bold {
                            serif_bold.clone()
                        } else {
                            serif.clone()
                        }
                    } else if *bold {
                        sans_bold.clone()
                    } else {
                        sans.clone()
                    };
                    text_ops(&mut ops, new, origin.0, origin.1, *size, fid, color);
                }
            }
            EditAction::Rotate { .. } | EditAction::DeletePage => {}
        }
    }

    let page = new_page(width, height, ops);
    doc.with_pages(vec![page]);
    let bytes = doc.save(&PdfSaveOptions::default(), &mut warnings);
    Ok(bytes)
}

fn wrap_text(text: &str, max_width: f32, size: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for raw in text.lines() {
        if raw.trim().is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in raw.split_whitespace() {
            let candidate = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            if estimate_text_width(&candidate, size) > max_width && !current.is_empty() {
                lines.push(current);
                current = word.to_string();
            } else {
                current = candidate;
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

pub fn build_text_pdf(page_texts: &[String], fonts: &FontSet) -> Result<Vec<u8>> {
    let width = 595.28;
    let height = 841.89;
    let margin = 50.0;
    let size = 11.0;
    let leading = 15.0;
    let mut doc = PdfDocument::new("OCR Metin");
    let font_id = doc.add_font(&fonts.sans);
    let mut pages: Vec<PdfPage> = Vec::new();
    let mut ops: Vec<Op> = Vec::new();
    let mut y = height - margin;

    let push_page = |pages: &mut Vec<PdfPage>, ops: &mut Vec<Op>| {
        pages.push(new_page(width, height, std::mem::take(ops)));
    };

    for (pi, text) in page_texts.iter().enumerate() {
        if pi > 0 {
            push_page(&mut pages, &mut ops);
            y = height - margin;
        }
        let lines = wrap_text(text, width - 2.0 * margin, size);
        for line in lines {
            if y < margin {
                push_page(&mut pages, &mut ops);
                y = height - margin;
            }
            if !line.is_empty() {
                text_ops(&mut ops, &line, margin, y, size, font_id.clone(), "black");
            }
            y -= leading;
        }
    }
    if !ops.is_empty() || pages.is_empty() {
        push_page(&mut pages, &mut ops);
    }
    doc.with_pages(pages);
    let mut warnings = Vec::new();
    Ok(doc.save(&PdfSaveOptions::default(), &mut warnings))
}

pub fn build_image_pdf(image_path: &std::path::Path, dpi: f32) -> Result<Vec<u8>> {
    let bytes = std::fs::read(image_path)?;
    let mut doc = PdfDocument::new("Görsel");
    let mut warnings = Vec::new();
    let img = RawImage::decode_from_bytes(&bytes, &mut warnings)
        .map_err(|e| anyhow!("Görsel açılamadı: {e}"))?;
    let nw = img.width.max(1) as f32;
    let nh = img.height.max(1) as f32;
    let w_pt = nw * 72.0 / dpi;
    let h_pt = nh * 72.0 / dpi;
    let img_id = doc.add_image(&img);
    let ops = vec![Op::UseXobject {
        id: img_id,
        transform: XObjectTransform {
            translate_x: Some(Pt(0.0)),
            translate_y: Some(Pt(0.0)),
            scale_x: Some(w_pt / nw),
            scale_y: Some(h_pt / nh),
            dpi: Some(72.0),
            ..Default::default()
        },
    }];
    doc.with_pages(vec![new_page(w_pt, h_pt, ops)]);
    Ok(doc.save(&PdfSaveOptions::default(), &mut warnings))
}

pub fn page_number_text(format: &str, page: u32, start: u32, total: u32) -> String {
    format
        .replace("{n}", &(page + start - 1).to_string())
        .replace("{t}", &(total + start - 1).to_string())
}

// ParsedFont'u dışa aktarmamak için burada küçük bir yardımcı.
#[allow(dead_code)]
pub fn font_list(fonts: &FontSet) -> [ParsedFont; 4] {
    [
        fonts.sans.clone(),
        fonts.sans_bold.clone(),
        fonts.serif.clone(),
        fonts.serif_bold.clone(),
    ]
}
