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
// The shared worker's tests check which downloads open in a viewer.
#[cfg(test)]
#[path = "../../../src/open_policy.rs"]
pub mod open_policy;
#[path = "../../../src/paths.rs"]
pub mod paths;
#[path = "../../../src/updates.rs"]
pub mod updates;
#[path = "../../../src/util.rs"]
pub mod util;
#[path = "../../../src/voice.rs"]
pub mod voice;

#[path = "../../../src/gif_video.rs"]
pub mod gif_video;
#[path = "../../../src/video_metadata.rs"]
pub mod video_metadata;

mod audio_preview;
mod bridge;
