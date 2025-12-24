# Shelf

## Project Overview

"Shelf" is a modern, GTK4-based desktop application written in Rust, designed to help users manage and browse their PDF document collections efficiently. It provides a clean graphical interface to scan specified directories, extract essential metadata from PDF files, and display them in an organized grid view.

## Features

*   **PDF Scanning & Metadata Extraction:** Automatically scans configured directories and extracts key information like title, author, subject, keywords, page count, and file size from PDF documents.
*   **Intuitive Grid View:** Presents PDF documents in an easy-to-navigate grid layout.
*   **Responsive Preview Pane:** A resizable and togglable sidebar displays detailed metadata for the currently selected PDF.
*   **Fuzzy Search:** Quickly find documents by filename, title, or author using intelligent fuzzy matching.
*   **Configurable External Viewer:** Open PDF files with your preferred external PDF viewer (defaults to `zathura`).
*   **Performance:** Utilizes parallel processing with `rayon` for fast PDF scanning and `rusqlite` for efficient metadata caching.
*   **User Configuration:** Customizable settings stored in a TOML file.

## Getting Started

### Prerequisites

To build and run Shelf, you need:

*   **Rust:** Install Rust and Cargo using `rustup`: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
*   **GTK4 Development Libraries:** Ensure you have GTK4 development packages installed on your system. The installation steps vary by distribution:
    *   **Debian/Ubuntu:** `sudo apt install libgtk-4-dev`
    *   **Fedora:** `sudo dnf install gtk4-devel`
    *   **Arch Linux:** `sudo pacman -S gtk4`
    *   **macOS (via Homebrew):** `brew install gtk4`

### Building

Navigate to the project's root directory and use Cargo to build:

```bash
cargo build
```

For a release-optimized build:

```bash
cargo build --release
```

### Running

After building, you can run the application:

```bash
cargo run
```

For the release version:

```bash
cargo run --release
```

## Configuration

Shelf stores its configuration in `~/.shelf/config.toml`. You can specify directories to scan for PDFs and your preferred PDF viewer command (e.g., `zathura %` where `%` is a placeholder for the PDF path).

Example `config.toml`:

```toml
scan_dirs = [
    "/home/youruser/Documents/Books",
    "/home/youruser/Downloads/PDFs"
]
pdf_viewer_command = "zathura %"
# or "evince %"
# or "xdg-open %"
```

## Contributing

Contributions are welcome! If you find a bug or have a feature request, please open an issue on the project's repository.

## License

This project is licensed under UNLICENSE.
