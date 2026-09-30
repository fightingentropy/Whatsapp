pub fn build(root: &std::path::Path) {
    let source = root.join("src/native/apple.m");
    println!("cargo:rerun-if-changed={}", source.display());
    let target = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    if target == "macos" || target == "ios" {
        let mut build = cc::Build::new();
        if std::env::var_os("CARGO_FEATURE_DEMO").is_some() {
            build.define("WHATSAPP_NATIVE_PROBE", None);
        }
        build
            .file(source)
            .flag("-fobjc-arc")
            .flag("-fblocks")
            .compile("whatsapp_apple");
        for framework in [
            "Foundation",
            "AVFoundation",
            "CoreMedia",
            "CoreGraphics",
            "ImageIO",
        ] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
        if target == "macos" {
            println!("cargo:rustc-link-lib=framework=AppKit");
        }
    }
}
