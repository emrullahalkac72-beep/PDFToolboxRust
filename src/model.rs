use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tab {
    Merge,
    Split,
    Convert,
    Docx,
    Edit,
    Udf,
    PageNo,
    Ocr,
    Export,
    Print,
}

impl Tab {
    pub const ALL: [Tab; 10] = [
        Tab::Merge,
        Tab::Split,
        Tab::Convert,
        Tab::Docx,
        Tab::Edit,
        Tab::Udf,
        Tab::PageNo,
        Tab::Ocr,
        Tab::Export,
        Tab::Print,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Merge => "PDF Birleştir",
            Tab::Split => "PDF Böl",
            Tab::Convert => "Dosya → PDF",
            Tab::Docx => "DOCX → PDF",
            Tab::Edit => "PDF Düzenle",
            Tab::Udf => "UDF → PDF",
            Tab::PageNo => "Sayfa Numarası",
            Tab::Ocr => "Taranmış PDF",
            Tab::Export => "PDF → Diğer",
            Tab::Print => "PDF Yazdır",
        }
    }
}

#[derive(Clone, Debug)]
pub struct PdfFileEntry {
    pub path: PathBuf,
    pub name: String,
    pub total_pages: u32,
    pub selected: Vec<u32>,
}

impl PdfFileEntry {
    pub fn new(path: PathBuf, total_pages: u32) -> Self {
        Self {
            name: crate::util::file_name(&path),
            path,
            total_pages,
            selected: (1..=total_pages).collect(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct FileRow {
    pub path: PathBuf,
    pub name: String,
    pub ext: String,
    pub status: String,
}

impl FileRow {
    pub fn new(path: PathBuf) -> Self {
        let ext = crate::util::extension(&path).to_ascii_uppercase();
        Self {
            name: crate::util::file_name(&path),
            path,
            ext,
            status: "Bekliyor".to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatermarkMode {
    Diagonal,
    Horizontal,
    Repeated,
}

impl WatermarkMode {
    pub fn title(self) -> &'static str {
        match self {
            WatermarkMode::Diagonal => "Çapraz",
            WatermarkMode::Horizontal => "Yatay",
            WatermarkMode::Repeated => "Tekrarlı",
        }
    }
    pub fn all() -> [WatermarkMode; 3] {
        [
            WatermarkMode::Diagonal,
            WatermarkMode::Horizontal,
            WatermarkMode::Repeated,
        ]
    }
}

#[derive(Clone, Debug)]
pub enum EditAction {
    Text {
        x: f32,
        y: f32,
        text: String,
        size: f32,
        color: String,
        serif: bool,
        bold: bool,
    },
    Rectangle {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        fill: bool,
        color: String,
        lw: f32,
    },
    Circle {
        cx: f32,
        cy: f32,
        r: f32,
        fill: bool,
        color: String,
        lw: f32,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        color: String,
        width: f32,
    },
    Image {
        path: PathBuf,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    Watermark {
        text: String,
        mode: WatermarkMode,
        size: f32,
        alpha: f32,
        color: String,
    },
    Rotate {
        angle: i32,
    },
    DeletePage,
    TextEdit {
        #[allow(dead_code)]
        idx: usize,
        bounds: (f32, f32, f32, f32),
        old: String,
        new: String,
        size: f32,
        color: String,
        bg: String,
        origin: (f32, f32),
        serif: bool,
        bold: bool,
    },
}

#[derive(Clone, Debug)]
pub struct EditItem {
    /// 0 = tüm sayfalar
    pub page: u32,
    pub action: EditAction,
}

impl EditItem {
    pub fn label(&self) -> String {
        let kind = match &self.action {
            EditAction::Text { text, .. } => format!("Metin: \"{}\"", short(text, 28)),
            EditAction::Rectangle { .. } => "Dikdörtgen".to_string(),
            EditAction::Circle { .. } => "Daire".to_string(),
            EditAction::Line { .. } => "Çizgi".to_string(),
            EditAction::Image { path, .. } => {
                format!("Resim: {}", crate::util::file_name(path))
            }
            EditAction::Watermark { text, .. } => {
                format!("Filigran: \"{}\"", short(text, 24))
            }
            EditAction::Rotate { angle } => format!("Döndür: {angle}°"),
            EditAction::DeletePage => "Sayfa sil".to_string(),
            EditAction::TextEdit { old, new, .. } => {
                format!("\"{}\" → \"{}\"", short(old, 20), short(new, 20))
            }
        };
        let page = if self.page == 0 {
            "Tüm sayfalar".to_string()
        } else {
            format!("Sayfa {}", self.page)
        };
        format!("{kind}  •  {page}")
    }
}

fn short(s: &str, n: usize) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() <= n {
        s
    } else {
        let mut t: String = s.chars().take(n).collect();
        t.push('…');
        t
    }
}

#[derive(Clone, Debug)]
pub struct TextObject {
    pub idx: usize,
    pub text: String,
    pub origin: (f32, f32),
    pub size: f32,
    #[allow(dead_code)]
    pub font: String,
    pub bold: bool,
    pub serif: bool,
    pub color: String,
    pub bounds: (f32, f32, f32, f32),
}

#[derive(Clone)]
pub struct RenderedPage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OcrMode {
    Searchable,
    Text,
}

pub fn palette_color(name: &str) -> [u8; 3] {
    match name {
        "black" => [0, 0, 0],
        "white" => [255, 255, 255],
        "red" => [255, 0, 0],
        "blue" => [0, 0, 255],
        "green" => [0, 128, 0],
        "gray" => [128, 128, 128],
        "orange" => [243, 156, 18],
        "purple" => [142, 68, 173],
        "brown" => [121, 85, 72],
        "navy" => [31, 58, 147],
        "teal" => [0, 137, 123],
        "gold" => [212, 172, 13],
        other => parse_hex_color(other).unwrap_or([0, 0, 0]),
    }
}

pub fn parse_hex_color(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().trim_start_matches('#');
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some([r, g, b])
    } else {
        None
    }
}

#[allow(dead_code)]
pub fn hex_color(name: &str) -> String {
    let c = palette_color(name);
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

pub const PALETTE: [(&str, &str); 12] = [
    ("black", "Siyah"),
    ("white", "Beyaz"),
    ("red", "Kırmızı"),
    ("blue", "Mavi"),
    ("green", "Yeşil"),
    ("gray", "Gri"),
    ("orange", "Turuncu"),
    ("purple", "Mor"),
    ("brown", "Kahve"),
    ("navy", "Lacivert"),
    ("teal", "Petrol"),
    ("gold", "Altın"),
];
