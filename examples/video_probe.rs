//! Compare VideoToolbox hardware decoding with the existing OpenH264 preview.
//! `cargo run --release --features demo --example video_probe -- fixture.mp4`

fn main() -> anyhow::Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("provide a synthetic MP4 fixture"))?;
    let report = zapfast::animation::diagnostics::compare(std::path::Path::new(&path), 5)?;
    if let Some(directory) = std::env::args_os().nth(2) {
        zapfast::animation::diagnostics::save_first_frames(
            std::path::Path::new(&path),
            std::path::Path::new(&directory),
        )?;
    }
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
