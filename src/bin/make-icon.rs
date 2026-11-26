//! Writes the app icon as a PNG: `cargo run --release --bin make-icon -- out.png [size]`.

#[path = "../icon.rs"]
mod icon;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| "facet.png".into());
    let size: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(256);
    std::fs::write(&path, icon::png(size, &icon::render(size))).expect("failed to write icon");
    println!("wrote {path} ({size}x{size})");
}
