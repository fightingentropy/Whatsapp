//! Rebuild the opaque, edge-to-edge iPhone icon from the shared handset/chat mark.
//! Run `cargo run --locked --example render_ios_icon` after editing the artwork.

fn main() -> anyhow::Result<()> {
    let mark = include_str!("../assets/brand/whatsapp-mark.svg")
        .replace(
            "width=\"64\" height=\"64\"",
            "x=\"64\" y=\"56\" width=\"896\" height=\"896\"",
        )
        .replace("#ffffff", "#46d68a");
    // iOS supplies the corner mask. Inset rounded rectangles and transparency
    // belong to the Mac asset and would leave a black border on the home screen.
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
  <defs>
    <linearGradient id="face" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#292e31"/>
      <stop offset="1" stop-color="#131719"/>
    </linearGradient>
  </defs>
  <rect width="1024" height="1024" fill="url(#face)"/>
  {mark}
</svg>
"##
    );
    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default())?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(1024, 1024)
        .ok_or_else(|| anyhow::anyhow!("Could not allocate the icon"))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    anyhow::ensure!(
        pixmap.pixels().iter().all(|pixel| pixel.alpha() == 255),
        "The iPhone icon must be opaque across the entire square"
    );
    // Encode RGB, without an alpha channel, for the iOS asset catalog.
    let rgb = pixmap
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|pixel| pixel[..3].iter().copied())
        .collect();
    let image = image::RgbImage::from_raw(1024, 1024, rgb)
        .ok_or_else(|| anyhow::anyhow!("Could not encode the icon"))?;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    std::fs::write(root.join("assets/brand/whatsapp-ios-icon.svg"), svg)?;
    image.save(root.join("ios/Whatsapp/Assets.xcassets/AppIcon.appiconset/Icon.png"))?;
    println!("Updated Whatsapp's iPhone SVG and opaque PNG icon");
    Ok(())
}
