//! Rebuilds the committed macOS icon from the shared handset/chat mark.
//! Run `cargo run --locked --example render_icon` after editing the artwork.

fn main() -> anyhow::Result<()> {
    let mark = include_str!("../assets/brand/whatsapp-mark.svg")
        .replace(
            "width=\"64\" height=\"64\"",
            "x=\"242\" y=\"236\" width=\"540\" height=\"540\"",
        )
        .replace("#ffffff", "#46d68a");
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
  <defs>
    <linearGradient id="face" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#292e31"/>
      <stop offset="1" stop-color="#131719"/>
    </linearGradient>
    <filter id="shadow" x="-15%" y="-15%" width="130%" height="140%">
      <feGaussianBlur stdDeviation="14"/>
    </filter>
  </defs>
  <rect x="100" y="118" width="824" height="824" rx="188" fill="#000000" opacity=".22" filter="url(#shadow)"/>
  <rect x="100" y="100" width="824" height="824" rx="188" fill="url(#face)"/>
  <rect x="102" y="102" width="820" height="820" rx="186" fill="none" stroke="#ffffff" stroke-opacity=".08" stroke-width="2"/>
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
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    std::fs::write(root.join("packaging/macos/icon-1024.svg"), svg)?;
    pixmap.save_png(root.join("packaging/macos/icon-1024.png"))?;
    println!("Updated Whatsapp's macOS SVG and PNG icons");
    Ok(())
}
