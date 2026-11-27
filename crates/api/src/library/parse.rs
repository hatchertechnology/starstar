use std::collections::HashSet;
use std::fs::File;
use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::path::Path;

use base64::{Engine, engine::general_purpose::STANDARD};
use flate2::{Crc, read::ZlibDecoder};
use image::ImageFormat;
use starstar_shared::{LibraryKind, PrintMeta};

const HEAD_LIMIT: u64 = 8 * 1024 * 1024;
const TAIL_LIMIT: u64 = 512 * 1024;

#[derive(Debug, Default)]
pub struct Parsed {
    pub meta: PrintMeta,
    pub preview_png: Option<Vec<u8>>,
    pub parsed: bool,
}

/// Read bounded portions of a print file; malformed content is reported in `parsed`.
pub fn parse_file(path: &Path, kind: LibraryKind) -> io::Result<Parsed> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let mut head = Vec::new();
    (&mut file).take(HEAD_LIMIT).read_to_end(&mut head)?;
    match kind {
        LibraryKind::Bgcode => Ok(parse_bgcode(&head)),
        LibraryKind::Gcode => {
            let mut tail = Vec::new();
            let start = len.saturating_sub(TAIL_LIMIT).max(head.len() as u64);
            if start < len {
                file.seek(SeekFrom::Start(start))?;
                file.read_to_end(&mut tail)?;
                // The first line may start in the middle of a comment.
                if start > head.len() as u64 {
                    if let Some(pos) = tail.iter().position(|&b| b == b'\n') {
                        tail.drain(..=pos);
                    } else {
                        tail.clear();
                    }
                }
            }
            Ok(parse_ascii(&head, &tail))
        }
    }
}

fn update_meta(
    meta: &mut PrintMeta,
    seen: &mut HashSet<String>,
    text: &str,
    comments_only: bool,
) -> bool {
    let mut found = false;
    for line in text.lines() {
        let line = line.trim();
        if comments_only && !line.starts_with(';') {
            continue;
        }
        let line = line.trim_start_matches(';').trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if !matches!(
            key,
            "printer_model"
                | "filament_type"
                | "nozzle_diameter"
                | "layer_height"
                | "temperature"
                | "bed_temperature"
                | "filament used [g]"
                | "filament used [mm]"
                | "estimated printing time (normal mode)"
        ) {
            continue;
        }
        found = true;
        if !seen.insert(key.to_owned()) {
            continue;
        }
        let value = value.trim();
        macro_rules! set {
            ($field:ident, $v:expr) => {{
                found = true;
                if meta.$field.is_none() {
                    meta.$field = $v;
                }
            }};
        }
        match key.trim() {
            "printer_model" => set!(printer_model, Some(value.to_owned())),
            "filament_type" => set!(filament_type, Some(value.to_owned())),
            "nozzle_diameter" => set!(nozzle_diameter, value.parse().ok()),
            "layer_height" => set!(layer_height, value.parse().ok()),
            "temperature" => set!(nozzle_temp, value.parse().ok()),
            "bed_temperature" => set!(bed_temp, value.parse().ok()),
            "filament used [g]" => set!(filament_used_g, value.parse().ok()),
            "filament used [mm]" => set!(filament_used_mm, value.parse().ok()),
            "estimated printing time (normal mode)" => {
                set!(estimated_print_time, parse_time(value))
            }
            _ => {}
        }
    }
    found
}

fn parse_time(value: &str) -> Option<u64> {
    let mut total = 0u64;
    let mut any = false;
    for part in value.split_whitespace() {
        let last = part.chars().last()?;
        let (number, unit) = part.split_at(part.len() - last.len_utf8());
        let factor = match unit {
            "d" => 86400,
            "h" => 3600,
            "m" => 60,
            "s" => 1,
            _ => return None,
        };
        total = total.checked_add(number.parse::<u64>().ok()?.checked_mul(factor)?)?;
        any = true;
    }
    any.then_some(total)
}

#[derive(Default)]
struct Previews {
    png: Option<(u64, Vec<u8>)>,
    qoi: Option<(u64, Vec<u8>)>,
}
impl Previews {
    fn add(&mut self, bytes: &[u8], format: ImageFormat) {
        let Ok(img) = image::load_from_memory_with_format(bytes, format) else {
            return;
        };
        let area = u64::from(img.width()) * u64::from(img.height());
        let target = if format == ImageFormat::Png {
            &mut self.png
        } else {
            &mut self.qoi
        };
        if target.as_ref().is_none_or(|(old, _)| area > *old) {
            if format == ImageFormat::Png {
                *target = Some((area, bytes.to_vec()));
            } else {
                let mut output = Cursor::new(Vec::new());
                if img.write_to(&mut output, ImageFormat::Png).is_ok() {
                    *target = Some((area, output.into_inner()));
                }
            }
        }
    }
    fn finish(self) -> Option<Vec<u8>> {
        self.png.or(self.qoi).map(|(_, bytes)| bytes)
    }
}

fn parse_ascii(head: &[u8], tail: &[u8]) -> Parsed {
    let mut result = Parsed::default();
    let mut previews = Previews::default();
    let text = String::from_utf8_lossy(head);
    let mut seen = HashSet::new();
    result.parsed = update_meta(&mut result.meta, &mut seen, &text, true);
    result.parsed |= update_meta(
        &mut result.meta,
        &mut seen,
        &String::from_utf8_lossy(tail),
        true,
    );
    let mut encoded: Option<String> = None;
    for line in text.lines() {
        let line = line.trim().strip_prefix(';').unwrap_or("").trim();
        if line.starts_with("thumbnail begin ") {
            encoded = Some(String::new());
        } else if line.starts_with("thumbnail end") {
            if let Some(data) = encoded.take()
                && let Ok(bytes) = STANDARD.decode(data)
            {
                previews.add(&bytes, ImageFormat::Png);
            }
        } else if let Some(data) = &mut encoded {
            data.push_str(line);
        }
    }
    result.preview_png = previews.finish();
    result.parsed |= result.preview_png.is_some();
    result
}

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?))
}
fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

fn parse_bgcode(data: &[u8]) -> Parsed {
    parse_bgcode_inner(data).unwrap_or_default()
}
fn parse_bgcode_inner(data: &[u8]) -> Option<Parsed> {
    if data.get(..4)? != b"GCDE" || u32_at(data, 4)? != 1 {
        return None;
    }
    let checksum = u16_at(data, 8)?;
    if checksum > 1 {
        return None;
    }
    let mut pos = 10usize;
    let mut result = Parsed::default();
    let mut previews = Previews::default();
    let mut seen = HashSet::new();
    loop {
        let start = pos;
        let kind = u16_at(data, pos)?;
        let compression = u16_at(data, pos + 2)?;
        let uncompressed = u32_at(data, pos + 4)? as usize;
        pos += 8;
        // The first G-code block is outside the metadata/preview region.
        if kind == 1 {
            break;
        }
        let size = if compression == 0 {
            uncompressed
        } else {
            let n = u32_at(data, pos)? as usize;
            pos += 4;
            n
        };
        let params_len = if kind == 5 { 6 } else { 2 };
        let params = data.get(pos..pos.checked_add(params_len)?)?;
        pos += params_len;
        let end = pos.checked_add(size)?;
        let content = data.get(pos..end)?;
        pos = end;
        if checksum == 1 {
            let expected = u32_at(data, pos)?;
            let mut crc = Crc::new();
            crc.update(data.get(start..pos)?);
            if crc.sum() != expected {
                return None;
            }
            pos += 4;
        }
        let decoded = match compression {
            0 => content.to_vec(),
            1 => {
                if uncompressed as u64 > HEAD_LIMIT {
                    continue;
                }
                let mut output = Vec::new();
                if ZlibDecoder::new(content)
                    .take(HEAD_LIMIT + 1)
                    .read_to_end(&mut output)
                    .is_err()
                    || output.len() != uncompressed
                {
                    return None;
                }
                output
            }
            _ => continue,
        };
        match kind {
            0 | 2 | 3 | 4 => {
                result.parsed |= update_meta(
                    &mut result.meta,
                    &mut seen,
                    &String::from_utf8_lossy(&decoded),
                    false,
                );
            }
            5 => match u16_at(params, 0)? {
                0 => previews.add(&decoded, ImageFormat::Png),
                2 => previews.add(&decoded, ImageFormat::Qoi),
                _ => {}
            },
            _ => {}
        }
    }
    result.preview_png = previews.finish();
    result.parsed |= result.preview_png.is_some();
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::STANDARD};
    use std::io::{Cursor, Write};

    const CUBE: &[u8] = include_bytes!("../../../../prints/cube-12mm/cube-12mm.MK4.bgcode");

    fn parse(bytes: &[u8], kind: LibraryKind) -> Parsed {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(bytes).unwrap();
        parse_file(file.path(), kind).unwrap()
    }
    fn dimensions(bytes: &[u8]) -> (u32, u32) {
        (
            image::load_from_memory(bytes).unwrap().width(),
            image::load_from_memory(bytes).unwrap().height(),
        )
    }
    #[test]
    fn parses_real_mk4_bgcode() {
        let p = parse(CUBE, LibraryKind::Bgcode);
        assert!(p.parsed);
        assert_eq!(p.meta.printer_model.as_deref(), Some("MK4IS"));
        assert_eq!(p.meta.nozzle_diameter, Some(0.4));
        assert_eq!(p.meta.filament_type.as_deref(), Some("PLA"));
        assert_eq!(p.meta.layer_height, Some(0.2));
        assert_eq!(p.meta.estimated_print_time, Some(537));
        assert_eq!(p.meta.filament_used_g, Some(1.11));
        let png = p.preview_png.unwrap();
        assert!(png.starts_with(b"\x89PNG"));
        assert_eq!(dimensions(&png), (380, 285));
    }
    #[test]
    fn parses_ascii_gcode_comments_and_thumbnail() {
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(16, 16)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let encoded = STANDARD.encode(png.into_inner());
        let input = format!(
            "; thumbnail begin 16x16 {}\n; {}\n; thumbnail end\nG28\n; printer_model = MK4IS\n; filament_type = PETG\n; estimated printing time (normal mode) = 1h 2m 3s\n",
            encoded.len(),
            encoded
        );
        let p = parse(input.as_bytes(), LibraryKind::Gcode);
        assert!(p.parsed);
        assert_eq!(p.meta.printer_model.as_deref(), Some("MK4IS"));
        assert_eq!(p.meta.filament_type.as_deref(), Some("PETG"));
        assert_eq!(p.meta.estimated_print_time, Some(3723));
        assert_eq!(dimensions(&p.preview_png.unwrap()), (16, 16));
    }
    #[test]
    fn prefers_png_then_converts_qoi() {
        let mut bytes = CUBE[..1527].to_vec();
        bytes.extend_from_slice(&CUBE[61365..66265]);
        bytes.extend_from_slice(&CUBE[66265..]);
        let p = parse(&bytes, LibraryKind::Bgcode);
        assert!(p.parsed);
        let png = p.preview_png.unwrap();
        assert!(png.starts_with(b"\x89PNG"));
        assert_eq!(dimensions(&png), (16, 16));
    }
    #[test]
    fn corrupt_bgcode_is_unparsed() {
        let mut corrupt = CUBE.to_vec();
        corrupt[110] ^= 1;
        for bytes in [&corrupt[..], &CUBE[..100]] {
            let p = parse(bytes, LibraryKind::Bgcode);
            assert!(!p.parsed);
            assert!(p.preview_png.is_none());
        }
    }
    #[test]
    fn plain_gcode_is_unparsed() {
        assert!(!parse(b"G28\nG1 X10\n", LibraryKind::Gcode).parsed);
    }
    #[test]
    fn repeated_metadata_keeps_first_value_and_ignores_commands() {
        let p = parse(b"printer_model = BAD\n; printer_model = FIRST\n; printer_model = SECOND\n; nozzle_diameter = invalid\n; nozzle_diameter = 0.4\n", LibraryKind::Gcode);
        assert_eq!(p.meta.printer_model.as_deref(), Some("FIRST"));
        assert_eq!(p.meta.nozzle_diameter, None);
    }
    #[test]
    fn kind_from_name() {
        assert_eq!(
            LibraryKind::from_name("A.BGCODE"),
            Some(LibraryKind::Bgcode)
        );
        assert_eq!(LibraryKind::from_name("x.gcode"), Some(LibraryKind::Gcode));
        for name in ["x.stl", "x", ".gcode.txt"] {
            assert_eq!(LibraryKind::from_name(name), None);
        }
    }
}
