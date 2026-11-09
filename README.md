# Orange Reader

Native EPUB reader built with Rust and [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui). `cargo run` starts the first native prototype. A separate [GPUI Kit + WebView migration build](experiments/gpui-webview/README.md) uses the existing `foliate-js/` for EPUB layout. The Electron implementation remains in `src/` and the npm configuration while migration continues.

## Run

Install stable Rust and the platform build tools. On macOS, GPUI also needs Xcode and its Metal Toolchain (`xcodebuild -downloadComponent MetalToolchain`).

```sh
cargo run
```

If Rustup has no default toolchain, run `rustup default stable` first. For an isolated library during testing:

```sh
ORANGE_READER_DATA_DIR=/tmp/orange-reader-test cargo run
```

## Native implementation

- GPUI book library with cover images, reading progress, file picker and drag-and-drop EPUB import.
- A separate native GPUI reading window per opening, with chapter navigation and text pagination.
- Native text selection, colored highlights, a highlight list and deletion.
- Persisted font size, font family, line height, paragraph spacing, content width, and light/dark controls.
- SQLite storage via `rusqlite`; the existing Electron library database and copied books are reused.
- Existing CFI reading positions resume approximately from their saved fraction. Existing highlights with text are migrated when their text can be matched to a paragraph.

The native EPUB renderer currently extracts text from XHTML. It does not yet reproduce EPUB CSS, inline images, fixed-layout pages, or exact CFI positions. Pagination uses character estimates, so a page may need scrolling. Text selection is limited to one paragraph at a time, and a highlight colors its paragraph. The original Electron reader is still available through `npm run dev` during this transition.

## GPUI Kit + WebView migration build

```sh
cargo run --manifest-path experiments/gpui-webview/Cargo.toml
```

This build opens the existing library in GPUI Kit and reads EPUB content with `foliate-js` in a macOS WKWebView. It imports books, displays covers and progress, opens books, and persists CFI reading positions and highlights in the existing SQLite tables. Its GPUI toolbar controls font size, font family, line spacing, letter spacing, paragraph spacing, content width, and theme. The toolbar also opens the contents and highlight panels in the WebView; these panels can navigate chapters, recolor highlights, and delete them. It is still a migration build: the main `cargo run` entry point still starts the earlier native text prototype, and the remaining visual interactions need manual macOS checks.

## Code layout

- `src-native/main.rs`: GPUI book library, reading windows and interaction.
- `src-native/epub.rs`: EPUB ZIP, package and chapter parsing.
- `src-native/store.rs`: SQLite data access and legacy data compatibility.
- `foliate-js/`: EPUB rendering code used by the GPUI Kit migration build and the previous Electron implementation.
- `src/`: previous Electron implementation.

## Check

```sh
cargo check
cargo test
```
