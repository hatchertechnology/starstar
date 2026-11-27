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
            let start = len.saturating_sub(TAIL_LIMIT);
            let boundary = if start == 0 {
                true
            } else {
                file.seek(SeekFrom::Start(start - 1))?;
                let mut previous = [0];
                file.read_exact(&mut previous)?;
                previous[0] == b'\n'
            };
            file.seek(SeekFrom::Start(start))?;
            (&mut file).take(TAIL_LIMIT).read_to_end(&mut tail)?;
            let mut extra = [0];
            let at_eof = file.read(&mut extra)? == 0;
            trim_tail(&mut tail, boundary, at_eof);
            if len > head.len() as u64 {
                if let Some(pos) = head.iter().rposition(|&b| b == b'\n') {
                    head.truncate(pos + 1);
                } else {
                    head.clear();
                }
            }
            Ok(parse_ascii(&head, &tail))
        }
    }
}

fn trim_tail(tail: &mut Vec<u8>, boundary: bool, at_eof: bool) {
    if !boundary {
        if let Some(pos) = tail.iter().position(|&b| b == b'\n') {
            tail.drain(..=pos);
        } else {
            tail.clear();
        }
    }
    if !at_eof && !tail.ends_with(b"\n") {
        if let Some(pos) = tail.iter().rposition(|&b| b == b'\n') {
            tail.truncate(pos + 1);
        } else {
            tail.clear();
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
                | "print_settings_id"
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
        if !seen.insert(key.to_owned()) {
            continue;
        }
        let value = value.trim();
        macro_rules! set {
            ($field:ident, $v:expr) => {{
                let value = $v;
                if value.is_some() {
                    found = true;
                }
                if meta.$field.is_none() {
                    meta.$field = value;
                }
            }};
        }
        match key.trim() {
            "printer_model" => set!(printer_model, (!value.is_empty()).then(|| value.to_owned())),
            "print_settings_id" => set!(
                print_settings_id,
                (!value.is_empty()).then(|| value.to_owned())
            ),
            "filament_type" => set!(filament_type, (!value.is_empty()).then(|| value.to_owned())),
            "nozzle_diameter" => set!(nozzle_diameter, parse_finite(value)),
            "layer_height" => set!(layer_height, parse_finite(value)),
            "temperature" => set!(nozzle_temp, parse_finite(value)),
            "bed_temperature" => set!(bed_temp, parse_finite(value)),
            "filament used [g]" => set!(filament_used_g, parse_finite(value)),
            "filament used [mm]" => set!(filament_used_mm, parse_finite(value)),
            "estimated printing time (normal mode)" => {
                set!(estimated_print_time, parse_time(value))
            }
            _ => {}
        }
    }
    found
}

fn parse_finite(value: &str) -> Option<f64> {
    value.parse::<f64>().ok().filter(|v| v.is_finite())
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
    png: Vec<(u64, Vec<u8>)>,
    qoi: Vec<(u64, Vec<u8>)>,
    retained: usize,
}
impl Previews {
    fn add(&mut self, bytes: &[u8], format: ImageFormat) {
        // Read only fixed headers here: constructing a PNG decoder can inflate
        // ancillary chunks before decoder limits have been applied.
        let dimensions = match format {
            ImageFormat::Png
                if bytes.len() >= 33
                    && bytes.starts_with(b"\x89PNG\r\n\x1a\n")
                    && bytes.get(8..16) == Some(b"\0\0\0\rIHDR") =>
            {
                bytes.get(16..24)
            }
            ImageFormat::Qoi if bytes.starts_with(b"qoif") => bytes.get(4..12),
            _ => None,
        };
        let Some(dims) = dimensions else { return };
        let width = u32::from_be_bytes(dims[..4].try_into().unwrap());
        let height = u32::from_be_bytes(dims[4..].try_into().unwrap());
        let area = u64::from(width) * u64::from(height);
        if width == 0 || height == 0 || width > 1024 || height > 1024 || area > 1_048_576 {
            return;
        }
        if bytes.len() > 16 * 1024 * 1024 - self.retained {
            return;
        }
        self.retained += bytes.len();
        let target = if format == ImageFormat::Png {
            &mut self.png
        } else {
            &mut self.qoi
        };
        target.push((area, bytes.to_vec()));
    }
    fn finish(mut self) -> Option<Vec<u8>> {
        for (format, candidates) in [
            (ImageFormat::Png, &mut self.png),
            (ImageFormat::Qoi, &mut self.qoi),
        ] {
            candidates.sort_by_key(|a| std::cmp::Reverse(a.0));
            // Give each format a bounded allowance so invalid PNGs cannot starve QOI.
            let mut decode_budget = 64 * 1024 * 1024usize;
            for (_, bytes) in candidates {
                // Charge the full per-decoder allowance even on failed decodes.
                let Some(remaining) = decode_budget.checked_sub(16 * 1024 * 1024) else {
                    break;
                };
                decode_budget = remaining;
                let mut reader = image::ImageReader::with_format(Cursor::new(&*bytes), format);
                let mut limits = image::Limits::default();
                limits.max_image_width = Some(1024);
                limits.max_image_height = Some(1024);
                limits.max_alloc = Some(16 * 1024 * 1024);
                reader.limits(limits);
                let Ok(img) = reader.decode() else {
                    continue;
                };
                if format == ImageFormat::Png {
                    return Some(std::mem::take(bytes));
                }
                let mut output = Cursor::new(Vec::new());
                if img.write_to(&mut output, ImageFormat::Png).is_ok() {
                    return Some(output.into_inner());
                }
            }
        }
        None
    }
}

fn parse_ascii(head: &[u8], tail: &[u8]) -> Parsed {
    let mut result = Parsed::default();
    let mut previews = Previews::default();
    let text = String::from_utf8_lossy(head);
    let mut seen = HashSet::new();
    result.parsed = update_meta(
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
    let mut budget = 16 * 1024 * 1024usize;
    let mut thumbnail_budget = 16 * 1024 * 1024usize;
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
        if !matches!(kind, 0 | 2 | 3 | 4 | 5) {
            continue;
        }
        if kind == 5 && !matches!(u16_at(params, 0)?, 0 | 2) {
            continue;
        }
        if matches!(kind, 0 | 2 | 3 | 4) && matches!(compression, 0 | 1) {
            budget = budget.checked_sub(uncompressed)?;
        }
        if kind == 5 && matches!(compression, 0 | 1) {
            let Some(remaining) = thumbnail_budget.checked_sub(uncompressed) else {
                continue;
            };
            thumbnail_budget = remaining;
        }
        let decoded = match compression {
            0 => content.to_vec(),
            1 => {
                let mut output = Vec::new();
                if ZlibDecoder::new(content)
                    .take(uncompressed as u64 + 1)
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
    fn reads_print_settings_id_from_bgcode() {
        let p = parse(CUBE, LibraryKind::Bgcode);
        assert_eq!(
            p.meta.print_settings_id.as_deref(),
            Some("0.20mm SPEED @MK4IS 0.4")
        );
    }
    #[test]
    fn reads_print_settings_id_from_ascii() {
        let p = parse(
            b"; print_settings_id = 0.15mm QUALITY @MINI\n",
            LibraryKind::Gcode,
        );
        assert_eq!(
            p.meta.print_settings_id.as_deref(),
            Some("0.15mm QUALITY @MINI")
        );
        let empty = parse(b"; print_settings_id = \n", LibraryKind::Gcode);
        assert_eq!(empty.meta.print_settings_id, None);
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
    fn block(kind: u16, compression: u16, declared: usize, content: &[u8], format: u16) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&kind.to_le_bytes());
        b.extend_from_slice(&compression.to_le_bytes());
        b.extend_from_slice(&(declared as u32).to_le_bytes());
        if compression != 0 {
            b.extend_from_slice(&(content.len() as u32).to_le_bytes());
        }
        b.extend_from_slice(&format.to_le_bytes());
        if kind == 5 {
            b.extend_from_slice(&[0; 4]);
        }
        b.extend_from_slice(content);
        b
    }
    fn bg(blocks: &[Vec<u8>]) -> Vec<u8> {
        let mut b = b"GCDE\x01\0\0\0\0\0".to_vec();
        for item in blocks {
            b.extend_from_slice(item);
        }
        b.extend_from_slice(&block(1, 0, 0, &[], 0));
        b
    }
    fn compressed(data: &[u8]) -> Vec<u8> {
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }
    #[test]
    fn metadata_budget_and_size_mismatch_reject_file() {
        let payload = vec![b'x'; 1024 * 1024];
        let zipped = compressed(&payload);
        let mut repeated = vec![block(0, 0, 17, b"printer_model=MK4", 0)];
        repeated.extend(vec![block(0, 1, payload.len(), &zipped, 0); 17]);
        assert!(!parse(&bg(&repeated), LibraryKind::Bgcode).parsed);
        let bad = block(0, 1, 3, &compressed(b"printer_model=MK4"), 0);
        assert!(
            !parse(
                &bg(&[block(0, 0, 17, b"printer_model=MK4", 0), bad]),
                LibraryKind::Bgcode
            )
            .parsed
        );
    }
    #[test]
    fn oversized_image_headers_are_skipped() {
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 2)
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        let valid = png.into_inner();
        let mut huge_png = valid.clone();
        huge_png[16..20].copy_from_slice(&1025u32.to_be_bytes());
        let mut crc = Crc::new();
        crc.update(&huge_png[12..29]);
        huge_png[29..33].copy_from_slice(&crc.sum().to_be_bytes());
        let mut qoi = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 2)
            .write_to(&mut qoi, ImageFormat::Qoi)
            .unwrap();
        let valid_qoi = qoi.into_inner();
        let mut huge_qoi = valid_qoi.clone();
        huge_qoi[4..8].copy_from_slice(&1025u32.to_be_bytes());
        for (oversized, small, format) in [(huge_png, valid, 0), (huge_qoi, valid_qoi, 2)] {
            let p = parse(
                &bg(&[
                    block(5, 0, oversized.len(), &oversized, format),
                    block(5, 0, small.len(), &small, format),
                ]),
                LibraryKind::Bgcode,
            );
            assert_eq!(dimensions(&p.preview_png.unwrap()), (2, 2));
        }
    }
    #[test]
    fn ascii_tail_boundary_preserves_complete_lines_and_discards_partial_lines() {
        let mut exact = vec![b'x'; 100];
        exact.push(b'\n');
        exact.extend_from_slice(b"; printer_model = TAIL\n");
        exact.resize(101 + TAIL_LIMIT as usize, b'x');
        assert_eq!(
            parse(&exact, LibraryKind::Gcode)
                .meta
                .printer_model
                .as_deref(),
            Some("TAIL")
        );
        let mut partial = b"; printer_model = WRONG".to_vec();
        partial.resize(TAIL_LIMIT as usize + 10, b'x');
        partial.extend_from_slice(b"\n; filament_type = PLA\n");
        let p = parse(&partial, LibraryKind::Gcode);
        assert!(p.meta.printer_model.is_none());
        assert_eq!(p.meta.filament_type.as_deref(), Some("PLA"));
    }
    #[test]
    fn invalid_values_do_not_mark_parsed() {
        let p = parse(
            b"; nozzle_diameter = abc\n; layer_height = NaN\n; temperature = NaN\n; bed_temperature = inf\n",
            LibraryKind::Gcode,
        );
        assert!(!p.parsed);
        assert_eq!(p.meta.layer_height, None);
    }
    #[test]
    fn footer_end_requires_eof_or_newline() {
        let mut tail = b"; filament_type = PLA\n; printer_model = PAR".to_vec();
        trim_tail(&mut tail, true, false);
        let p = parse_ascii(&[], &tail);
        assert_eq!(p.meta.filament_type.as_deref(), Some("PLA"));
        assert!(p.meta.printer_model.is_none());
        let mut final_line = b"; printer_model = COMPLETE".to_vec();
        trim_tail(&mut final_line, true, true);
        assert_eq!(
            parse_ascii(&[], &final_line).meta.printer_model.as_deref(),
            Some("COMPLETE")
        );
    }
    #[test]
    fn head_boundary_rejects_partial_thumbnail_end() {
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 2)
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        let mut input = format!(
            "; thumbnail begin 2x2 0\n; {}\n",
            STANDARD.encode(png.into_inner())
        )
        .into_bytes();
        input.resize(HEAD_LIMIT as usize - 16, b' ');
        input.extend_from_slice(b"\n; thumbnail end and more\n");
        assert!(parse(&input, LibraryKind::Gcode).preview_png.is_none());
    }
    #[test]
    fn thumbnail_budgets_bound_retention_and_decodes() {
        let mut previews = Previews::default();
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 2)
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        let bytes = png.into_inner();
        let mut large = bytes.clone();
        large.resize(16 * 1024 * 1024, 0);
        previews.add(&large, ImageFormat::Png);
        previews.add(&bytes, ImageFormat::Png);
        assert_eq!(previews.png.len(), 1);
        let payload = vec![0; 1024 * 1024];
        let zipped = compressed(&payload);
        let mut blocks = vec![block(0, 0, 17, b"printer_model=MK4", 0)];
        blocks.extend(vec![block(5, 1, payload.len(), &zipped, 0); 17]);
        assert!(parse(&bg(&blocks), LibraryKind::Bgcode).parsed);
        let mut invalid = bytes.clone();
        invalid.truncate(24);
        let mut candidates = Previews::default();
        for _ in 0..4 {
            candidates.add(&invalid, ImageFormat::Png);
        }
        candidates.add(&bytes, ImageFormat::Png);
        assert_eq!(dimensions(&candidates.finish().unwrap()), (2, 2));

        // Even structurally complete PNGs can fail decoding; reserve work for QOI.
        let mut corrupt = bytes.clone();
        corrupt[29] ^= 1;
        let mut qoi = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 2)
            .write_to(&mut qoi, ImageFormat::Qoi)
            .unwrap();
        let mut candidates = Previews::default();
        for _ in 0..4 {
            candidates.add(&corrupt, ImageFormat::Png);
        }
        candidates.add(&qoi.into_inner(), ImageFormat::Qoi);
        assert_eq!(dimensions(&candidates.finish().unwrap()), (2, 2));
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
