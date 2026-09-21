//! Native iOS bridge over the same protocol worker and archive as the Mac app.

#[path = "../../../src/archive.rs"]
pub mod archive;
#[path = "../../../src/backend.rs"]
pub mod backend;
#[cfg(target_os = "macos")]
#[path = "../../../src/background.rs"]
pub mod background;
#[path = "../../../src/model.rs"]
pub mod model;
#[path = "../../../src/paths.rs"]
pub mod paths;
#[path = "../../../src/updates.rs"]
pub mod updates;
#[path = "../../../src/util.rs"]
pub mod util;
#[path = "../../../src/voice.rs"]
pub mod voice;

mod bridge;
