//! Decides whether a clicked file is handed to its default app.
//!
//! A received attachment keeps the extension its sender chose, and opening a
//! file asks macOS to do whatever that extension means. For most types that is
//! "show it in a viewer". For others it is "run it": a `.terminal` profile
//! starts its command, a `.jar` starts Java, an `.html` page runs script with
//! file access. Downloads are not opened that way. Only types on a short list
//! of viewer formats are; everything else is shown in Finder.

use std::path::Path;

const PICTURES: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "heic", "heif", "bmp", "tif", "tiff", "avif",
];
const VIDEO: &[&str] = &[
    "mp4", "m4v", "mov", "3gp", "3g2", "mkv", "webm", "avi", "mpg", "mpeg",
];
const AUDIO: &[&str] = &[
    "mp3", "m4a", "aac", "ogg", "oga", "opus", "wav", "flac", "amr", "aif", "aiff", "caf",
];
const DOCUMENTS: &[&str] = &[
    "pdf", "txt", "md", "rtf", "csv", "tsv", "log", "json", "epub", "vcf", "ics",
];
const OFFICE: &[&str] = &[
    "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "pages", "numbers", "key",
];

/// Extensions that open in a viewer or editor rather than running anything.
/// Archives are left out too: unpacking one puts its contents, whatever they
/// are, next to it without any warning.
const VIEWER_EXTENSIONS: [&[&str]; 5] = [PICTURES, VIDEO, AUDIO, DOCUMENTS, OFFICE];

/// How a path the user clicked is handed to the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenTarget {
    /// Open in the default app. Used for folders and viewer formats.
    DefaultApp,
    /// Select the file in Finder and leave the decision to the user.
    Reveal,
}

/// Whether the file's extension is one that only opens a viewer.
pub fn opens_in_viewer(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            VIEWER_EXTENSIONS
                .iter()
                .flat_map(|group| group.iter())
                .any(|known| known.eq_ignore_ascii_case(extension))
        })
}

/// Folders open in Finder and viewer formats in their app. Any other file is
/// only revealed.
pub fn open_target(path: &Path) -> OpenTarget {
    if path.is_dir() || opens_in_viewer(path) {
        OpenTarget::DefaultApp
    } else {
        OpenTarget::Reveal
    }
}

/// Selects the file in a Finder window without opening it.
pub fn reveal(path: &Path) -> std::io::Result<()> {
    let mut child = std::process::Command::new("/usr/bin/open")
        .arg("-R")
        .arg(path)
        .spawn()?;
    // Reap the short-lived helper off the UI thread.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewer_formats_open_in_their_default_app() {
        for name in [
            "holiday.JPG",
            "clip.mp4",
            "voice.ogg",
            "ABC-tax_return.pdf",
            "notes.txt",
            "budget.xlsx",
            "whatsapp.log",
        ] {
            let path = Path::new("/cache").join(name);
            assert!(opens_in_viewer(&path), "{name}");
            assert_eq!(open_target(&path), OpenTarget::DefaultApp, "{name}");
        }
    }

    #[test]
    fn files_that_can_run_or_are_unknown_are_only_revealed() {
        for name in [
            "invoice.command",
            "invoice.terminal",
            "invoice.workflow",
            "invoice.app",
            "invoice.pkg",
            "invoice.dmg",
            "invoice.jar",
            "invoice.sh",
            "invoice.scpt",
            "invoice.webloc",
            "invoice.inetloc",
            "invoice.html",
            "invoice.svg",
            "invoice.zip",
            "invoice.pdf.command",
            "invoice.x-unknown",
            "invoice",
        ] {
            let path = Path::new("/cache").join(name);
            assert!(!opens_in_viewer(&path), "{name}");
            assert_eq!(open_target(&path), OpenTarget::Reveal, "{name}");
        }
    }

    #[test]
    fn folders_open_in_finder() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(open_target(root.path()), OpenTarget::DefaultApp);
    }
}
