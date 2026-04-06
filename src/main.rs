// Rust File Archiver with GUI
// Run with: cargo run --release
//
// Features:
// - Drag-and-drop file/folder selection
// - Custom .rpak archive format with zstd compression
// - Optional encryption with ChaCha20-Poly1305
// - Progress tracking and integrity checking with BLAKE3
// - Modern dark-themed GUI using iced

mod archiver;
mod gui;

fn main() -> iced::Result {
    gui::run()
}
