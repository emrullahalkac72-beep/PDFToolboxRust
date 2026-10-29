//! Uygulama simgesini (`assets/app.ico`) Windows yürütülebilir dosyasına gömer.
//!
//! `rc.exe` / Windows SDK gerektirmemek için `.res` kaynak dosyası doğrudan bu
//! betikte üretilir ve bağlayıcıya verilir. Böylece exe, Gezgin'de ve görev
//! çubuğunda aynı simgeyle görünür.

use std::fs;
use std::path::{Path, PathBuf};

const ICON_PATH: &str = "assets/app.ico";
const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
const LANGUAGE_EN_US: u16 = 0x0409;
const GROUP_ID: u16 = 1;

fn main() {
    println!("cargo:rerun-if-changed={ICON_PATH}");
    println!("cargo:rerun-if-changed=build.rs");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let Ok(out_dir) = std::env::var("OUT_DIR") else {
        return;
    };
    let target = PathBuf::from(out_dir).join("app_icon.res");
    match build_resource(Path::new(ICON_PATH), &target) {
        Ok(()) => println!("cargo:rustc-link-arg={}", target.display()),
        Err(e) => println!("cargo:warning=Uygulama simgesi gömülemedi: {e}"),
    }
}

struct IconEntry {
    width: u8,
    height: u8,
    colors: u8,
    reserved: u8,
    planes: u16,
    bit_count: u16,
    size: u32,
    offset: u32,
}

fn build_resource(icon: &Path, target: &Path) -> Result<(), String> {
    let bytes = fs::read(icon).map_err(|e| format!("{} okunamadı: {e}", icon.display()))?;
    let entries = parse_ico(&bytes)?;

    // RT_GROUP_ICON verisi: simge dizini (GRPICONDIR).
    let mut group = Vec::with_capacity(6 + entries.len() * 14);
    group.extend_from_slice(&0u16.to_le_bytes()); // idReserved
    group.extend_from_slice(&1u16.to_le_bytes()); // idType = simge
    group.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    for (index, entry) in entries.iter().enumerate() {
        group.push(entry.width);
        group.push(entry.height);
        group.push(entry.colors);
        group.push(entry.reserved);
        group.extend_from_slice(&entry.planes.to_le_bytes());
        group.extend_from_slice(&entry.bit_count.to_le_bytes());
        group.extend_from_slice(&entry.size.to_le_bytes());
        group.extend_from_slice(&((index + 1) as u16).to_le_bytes());
    }

    let mut body = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let start = entry.offset as usize;
        let end = start + entry.size as usize;
        let data = bytes
            .get(start..end)
            .ok_or_else(|| "ICO görüntü verisi dosya sınırını aşıyor.".to_string())?;
        push_entry(&mut body, RT_ICON, (index + 1) as u16, data);
    }
    push_entry(&mut body, RT_GROUP_ICON, GROUP_ID, &group);

    let mut out = Vec::with_capacity(body.len() + 32);
    out.extend_from_slice(&0u32.to_le_bytes()); // DataSize (rc.exe bu alanı 0 bırakır)
    out.extend_from_slice(&32u32.to_le_bytes()); // HeaderSize
    out.extend_from_slice(&0xFFFFu16.to_le_bytes()); // Type: sıra numarası
    out.extend_from_slice(&0u16.to_le_bytes()); // Type kimliği (kullanılmıyor)
    out.extend_from_slice(&0xFFFFu16.to_le_bytes()); // Name: sıra numarası
    out.extend_from_slice(&0u16.to_le_bytes()); // Name kimliği (kullanılmıyor)
    out.extend_from_slice(&0u32.to_le_bytes()); // DataVersion
    out.extend_from_slice(&0u16.to_le_bytes()); // MemoryFlags
    out.extend_from_slice(&0u16.to_le_bytes()); // LanguageId
    out.extend_from_slice(&0u32.to_le_bytes()); // Version
    out.extend_from_slice(&0u32.to_le_bytes()); // Characteristics
    out.extend_from_slice(&body);

    fs::write(target, &out).map_err(|e| format!("{} yazılamadı: {e}", target.display()))
}

fn push_entry(out: &mut Vec<u8>, kind: u16, name: u16, data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_le_bytes()); // DataSize
    out.extend_from_slice(&32u32.to_le_bytes()); // HeaderSize
    out.extend_from_slice(&0xFFFFu16.to_le_bytes()); // Type: sıra numarası
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(&0xFFFFu16.to_le_bytes()); // Name: sıra numarası
    out.extend_from_slice(&name.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // DataVersion
    out.extend_from_slice(&0x1010u16.to_le_bytes()); // MemoryFlags
    out.extend_from_slice(&LANGUAGE_EN_US.to_le_bytes()); // LanguageId
    out.extend_from_slice(&0u32.to_le_bytes()); // Version
    out.extend_from_slice(&0u32.to_le_bytes()); // Characteristics
    out.extend_from_slice(data);
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

fn parse_ico(bytes: &[u8]) -> Result<Vec<IconEntry>, String> {
    if bytes.len() < 6 {
        return Err("ICO dosyası çok kısa.".to_string());
    }
    let reserved = u16::from_le_bytes([bytes[0], bytes[1]]);
    let kind = u16::from_le_bytes([bytes[2], bytes[3]]);
    let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
    if reserved != 0 || kind != 1 {
        return Err("Geçersiz ICO başlığı.".to_string());
    }
    if count == 0 {
        return Err("ICO dosyasında simge yok.".to_string());
    }
    let mut entries = Vec::with_capacity(count);
    for index in 0..count {
        let base = 6 + index * 16;
        let entry = bytes
            .get(base..base + 16)
            .ok_or_else(|| "ICO dizini eksik.".to_string())?;
        entries.push(IconEntry {
            width: entry[0],
            height: entry[1],
            colors: entry[2],
            reserved: entry[3],
            planes: u16::from_le_bytes([entry[4], entry[5]]),
            bit_count: u16::from_le_bytes([entry[6], entry[7]]),
            size: u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]),
            offset: u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]),
        });
    }
    Ok(entries)
}
