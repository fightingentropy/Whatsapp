//! Attachment reads are bound to archived messages and the app's media folders.

use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

use super::{AttachmentQuery, Failure, Reply, Result, bounded};
use crate::paths::AppDirs;

const IMAGE_BYTES: u64 = 4 * 1024 * 1024;
const TEXT_BYTES: u64 = 8 * 1024 * 1024;
const PDF_BYTES: u64 = 32 * 1024 * 1024;

fn resolve(dirs: &AppDirs, stored: &Path) -> Option<PathBuf> {
    let roots = [
        dirs.media_cache_dir(),
        dirs.sticker_cache_dir(),
        dirs.saved_sticker_dir(),
    ];
    let allowed: Vec<_> = roots.iter().filter_map(|r| r.canonicalize().ok()).collect();
    let check = |path: &Path| {
        let path = path.canonicalize().ok()?;
        (path.is_file() && allowed.iter().any(|root| path.starts_with(root))).then_some(path)
    };
    if let Some(path) = check(stored) {
        return Some(path);
    }
    // Repair a path from an older profile/cache location without updating SQLite.
    let name = stored.file_name()?;
    roots.iter().find_map(|root| check(&root.join(name)))
}

pub(super) fn describe(dirs: &AppDirs, mut content: Value) -> Value {
    if let Some(object) = content.as_object_mut() {
        object.remove("waveform");
    }
    if let Some(media) = content.get_mut("media").and_then(Value::as_object_mut) {
        let stored = media.remove("path");
        let path = stored
            .as_ref()
            .and_then(Value::as_str)
            .and_then(|p| resolve(dirs, Path::new(p)));
        media.insert(
            "availability".into(),
            json!(if path.is_some() {
                "downloaded"
            } else {
                "not_downloaded"
            }),
        );
        media.insert("local_path".into(), json!(path));
    }
    content
}

pub(super) fn read(dirs: &AppDirs, message: &Value, args: &AttachmentQuery) -> Result<Reply> {
    let max_chars = bounded(args.max_chars, 12_000, 32_000)?;
    let offset = args.offset.unwrap_or(0) as usize;
    if offset > TEXT_BYTES as usize || args.page.is_some_and(|p| p == 0) {
        return Err(Failure::invalid());
    }
    let media = &message["content"]["media"];
    if !media.is_object() {
        return Err(Failure::new(
            "no_attachment",
            "This message has no readable attachment.",
        ));
    }
    let path = media["local_path"].as_str().and_then(|p| resolve(dirs, Path::new(p))).ok_or_else(|| Failure::new("attachment_unavailable", "This attachment is not downloaded in this profile. Download it in Whatsapp, then retry. Agent reads never request media or send receipts."))?;
    let file = File::open(&path).map_err(|_| unreadable())?;
    let size = file.metadata().map_err(|_| unreadable())?.len();
    let mime = media["mime"].as_str().unwrap_or("application/octet-stream");
    let mut result = json!({"chat_id": message["chat_id"], "message_id": message["id"],
        "file_name": message["content"]["file_name"], "local_path": path,
        "mime": mime, "size_bytes": size, "representation": "local_file"});
    let mut header = [0_u8; 16];
    let header_len = (&file).read(&mut header).map_err(|_| unreadable())?;
    let header = &header[..header_len];
    // PDFKit receives the validated local URL, never a posted URL or raw path
    // supplied by the caller. Header detection also handles generic MIME types.
    if header.starts_with(b"%PDF-") || mime.split(';').next() == Some("application/pdf") {
        if size > PDF_BYTES {
            result["note"] = json!(
                "PDF exceeds the 32 MiB text-extraction limit. Use the local file with a PDF tool."
            );
        } else {
            pdf_text(
                &path,
                args.page.unwrap_or(1),
                offset,
                max_chars,
                &mut result,
            )?;
        }
    } else if image::guess_format(header).is_ok() && size <= IMAGE_BYTES {
        let bytes = read_bounded(&path, IMAGE_BYTES)?;
        let format = image::guess_format(&bytes).map_err(|_| unreadable())?;
        let reader = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format);
        let (width, height) = reader.into_dimensions().map_err(|_| unreadable())?;
        if u64::from(width) * u64::from(height) <= 32_000_000 {
            result["representation"] = json!("image");
            result["width"] = json!(width);
            result["height"] = json!(height);
            result["mime"] = json!(format.to_mime_type());
            return Ok(Reply {
                value: result,
                image: Some((STANDARD.encode(bytes), format.to_mime_type().to_owned())),
            });
        }
        result["note"] = json!(
            "Image exceeds the 32 megapixel inline limit. Use the local file with an image tool."
        );
    } else if is_text(mime, &path) && size <= TEXT_BYTES {
        let bytes = read_bounded(&path, TEXT_BYTES)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| {
            Failure::new(
                "unsupported_encoding",
                "This file is not UTF-8 text. Use the local file with a document tool.",
            )
        })?;
        result["representation"] = json!("text");
        text_chunk(text, offset, max_chars, &mut result)?;
    } else {
        result["note"] = json!(
            "Use the local file with a suitable media/document tool. This reader does not transcribe audio/video, perform OCR, or decode other document formats."
        );
    }
    Ok(Reply::json(result))
}

fn unreadable() -> Failure {
    Failure::new(
        "attachment_unreadable",
        "Could not read this downloaded attachment. It may have been removed or be damaged.",
    )
}

fn read_bounded(path: &Path, max: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| unreadable())?
        .take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| unreadable())?;
    if bytes.len() as u64 > max {
        return Err(Failure::new(
            "attachment_too_large",
            "This file exceeds the inline read limit. Use its local path with a document/media tool.",
        ));
    }
    Ok(bytes)
}

fn is_text(mime: &str, path: &Path) -> bool {
    mime.starts_with("text/")
        || matches!(
            mime.split(';').next().unwrap_or(mime),
            "application/json" | "application/xml" | "application/yaml"
        )
        || path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
            matches!(
                s.to_ascii_lowercase().as_str(),
                "txt" | "md" | "csv" | "tsv" | "json" | "xml" | "yaml" | "yml" | "ics" | "vcf"
            )
        })
}

fn text_chunk(text: &str, offset: usize, max_chars: usize, result: &mut Value) -> Result<()> {
    let total = text.chars().count();
    if offset > total {
        return Err(Failure::invalid());
    }
    let chunk: String = text.chars().skip(offset).take(max_chars).collect();
    let end = offset + chunk.chars().count();
    result["text"] = json!(chunk);
    result["offset"] = json!(offset);
    result["total_chars"] = json!(total);
    result["next_offset"] = json!((end < total).then_some(end));
    Ok(())
}

#[cfg(target_os = "macos")]
fn pdf_text(
    path: &Path,
    page: u32,
    offset: usize,
    max_chars: usize,
    result: &mut Value,
) -> Result<()> {
    use objc2::{AllocAnyThread, rc::autoreleasepool};
    use objc2_foundation::{NSString, NSURL};
    use objc2_pdf_kit::PDFDocument;

    autoreleasepool(|_| {
        let path = path.to_str().ok_or_else(unreadable)?;
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        let document = unsafe { PDFDocument::initWithURL(PDFDocument::alloc(), &url) }
            .ok_or_else(unreadable)?;
        if unsafe { document.isLocked() } {
            return Err(Failure::new(
                "pdf_locked",
                "This PDF requires a password; text was not extracted.",
            ));
        }
        let pages = unsafe { document.pageCount() };
        if page == 0 || page as usize > pages {
            return Err(Failure::invalid());
        }
        let pdf_page = unsafe { document.pageAtIndex(page as usize - 1) }.ok_or_else(unreadable)?;
        let text = unsafe { pdf_page.string() }
            .map(|s| s.to_string())
            .unwrap_or_default();
        if text.len() > TEXT_BYTES as usize {
            return Err(Failure::new(
                "pdf_page_too_large",
                "This PDF page exceeds the 8 MiB text limit. Use the local file with a PDF tool.",
            ));
        }
        result["representation"] = json!("pdf_text");
        result["page"] = json!(page);
        result["page_count"] = json!(pages);
        result["next_page"] = json!(if (page as usize) < pages {
            page.checked_add(1)
        } else {
            None
        });
        result["needs_ocr"] = json!(text.trim().is_empty());
        text_chunk(&text, offset, max_chars, result)
    })
}

#[cfg(not(target_os = "macos"))]
fn pdf_text(_: &Path, _: u32, _: usize, _: usize, result: &mut Value) -> Result<()> {
    result["note"] =
        json!("PDF text extraction requires the macOS build. Use the local file with a PDF tool.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_count_characters_not_bytes() {
        let mut result = json!({});
        text_chunk("A🍋éZ", 1, 2, &mut result).unwrap();
        assert_eq!(result["text"], "🍋é");
        assert_eq!(result["total_chars"], 4);
        assert_eq!(result["next_offset"], 3);
        assert!(text_chunk("x", 2, 1, &mut result).is_err());
    }
}
