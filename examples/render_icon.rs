//! Renders packaging/icon/linewright.svg into an .iconset folder (all macOS icon sizes).
//! Used by packaging/make-icon.sh; run with:
//!     cargo run --example render_icon -- <output.iconset>

use resvg::{tiny_skia, usvg};

fn main() {
    let out = std::env::args().nth(1).expect("usage: render_icon <output.iconset>");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let svg = std::fs::read(root.join("packaging/icon/linewright.svg")).expect("read svg");
    let tree = usvg::Tree::from_data(&svg, &usvg::Options::default()).expect("parse svg");
    std::fs::create_dir_all(&out).expect("create iconset dir");
    let sizes: &[(&str, u32)] = &[
        ("icon_16x16", 16),
        ("icon_16x16@2x", 32),
        ("icon_32x32", 32),
        ("icon_32x32@2x", 64),
        ("icon_128x128", 128),
        ("icon_128x128@2x", 256),
        ("icon_256x256", 256),
        ("icon_256x256@2x", 512),
        ("icon_512x512", 512),
        ("icon_512x512@2x", 1024),
    ];
    for (name, px) in sizes {
        let mut pixmap = tiny_skia::Pixmap::new(*px, *px).expect("pixmap");
        let scale = *px as f32 / tree.size().width();
        resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
        pixmap.save_png(format!("{out}/{name}.png")).expect("write png");
    }
    println!("rendered {} sizes into {out}", sizes.len());
}
