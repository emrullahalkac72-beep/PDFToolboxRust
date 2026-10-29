use anyhow::{anyhow, Result};
use printpdf::ParsedFont;
use std::path::PathBuf;

#[derive(Clone)]
pub struct FontSet {
    pub sans: ParsedFont,
    pub sans_bold: ParsedFont,
    pub serif: ParsedFont,
    pub serif_bold: ParsedFont,
}

impl FontSet {
    pub fn load() -> Result<Self> {
        let sans = load_first(&[
            "arial.ttf",
            "segoeui.ttf",
            "calibri.ttf",
            "DejaVuSans.ttf",
            "LiberationSans-Regular.ttf",
        ])?;
        let sans_bold = load_first(&[
            "arialbd.ttf",
            "segoeuib.ttf",
            "calibrib.ttf",
            "DejaVuSans-Bold.ttf",
            "LiberationSans-Bold.ttf",
        ])
        .unwrap_or_else(|_| sans.clone());
        let serif = load_first(&[
            "times.ttf",
            "georgia.ttf",
            "DejaVuSerif.ttf",
            "LiberationSerif-Regular.ttf",
        ])
        .unwrap_or_else(|_| sans.clone());
        let serif_bold = load_first(&[
            "timesbd.ttf",
            "georgiabd.ttf",
            "DejaVuSerif-Bold.ttf",
            "LiberationSerif-Bold.ttf",
        ])
        .unwrap_or_else(|_| serif.clone());
        Ok(Self {
            sans,
            sans_bold,
            serif,
            serif_bold,
        })
    }

    #[allow(dead_code)]
    pub fn pick(&self, serif: bool, bold: bool) -> &ParsedFont {
        match (serif, bold) {
            (false, false) => &self.sans,
            (false, true) => &self.sans_bold,
            (true, false) => &self.serif,
            (true, true) => &self.serif_bold,
        }
    }
}

fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(windir) = std::env::var("WINDIR") {
        dirs.push(PathBuf::from(windir).join("Fonts"));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        dirs.push(
            PathBuf::from(local)
                .join("Microsoft")
                .join("Windows")
                .join("Fonts"),
        );
    }
    if let Ok(home) = std::env::var("USERPROFILE") {
        dirs.push(PathBuf::from(home).join(".fonts"));
    }
    dirs.push(PathBuf::from("/usr/share/fonts/truetype/dejavu"));
    dirs.push(PathBuf::from("/usr/share/fonts/truetype/liberation"));
    dirs.push(PathBuf::from("/Library/Fonts"));
    dirs.push(PathBuf::from("/System/Library/Fonts/Supplemental"));
    dirs
}

fn load_first(names: &[&str]) -> Result<ParsedFont> {
    for dir in font_dirs() {
        for name in names {
            let path = dir.join(name);
            if let Ok(bytes) = std::fs::read(&path) {
                let mut warnings = Vec::new();
                if let Some(font) = ParsedFont::from_bytes(&bytes, 0, &mut warnings) {
                    return Ok(font);
                }
            }
        }
    }
    Err(anyhow!(
        "Uygun TrueType yazı tipi bulunamadı (Arial / Times New Roman)."
    ))
}
