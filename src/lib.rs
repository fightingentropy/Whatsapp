//! ZapFast internals exposed for diagnostics and tests.

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
compile_error!("ZapFast Silicon supports Apple Silicon Macs (aarch64-apple-darwin) only.");

pub mod animation;
pub mod app;
pub mod archive;
pub mod audio;
pub mod backend;
#[cfg(target_os = "macos")]
pub mod background;
#[cfg(any(test, feature = "demo"))]
pub mod demo;
pub mod emoji;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod markup;
pub mod model;
pub mod notify;
pub mod paths;
pub mod qr;
pub mod renderer;
pub mod settings;
pub mod single_instance;
pub mod system_fonts;
pub mod theme;
pub mod transcript;
#[cfg(target_os = "linux")]
pub mod tray;
#[cfg(not(target_os = "linux"))]
#[path = "tray_native.rs"]
pub mod tray;
pub mod ui;
pub mod updates;
pub mod util;
pub mod voice;
