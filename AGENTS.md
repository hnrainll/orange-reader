# Verso 项目说明

仓库是文档驱动的 macOS 原生 EPUB 阅读器（类似预览 / IINA）：不做书架/书库文件管理，直接打开磁盘原路径的 EPUB 阅读，仅记录最近阅读历史（文件路径、阅读进度、章节与高亮）。

`cargo run` 启动 GPUI Kit 最近阅读窗口；传入 EPUB 路径或从 Finder 打开时直接弹出 WKWebView 阅读窗口，书页由 `foliate-js/` 渲染。正式入口在 `src/main.rs`，无 Electron 或 Node.js 运行时。

## 结构

- `src/main.rs`：最近阅读窗口（`HistoryView`）、多窗口阅读器（`ReaderWindow`）、macOS 打开文件钩子（`on_open_urls`/`on_reopen`）、loopback 资源服务与 WebView IPC。
- `src/reader.html`：目录、翻页、选字、高亮与阅读设置的网页端交互。
- `src/epub.rs`：打开时解析 EPUB 标题、作者与封面，实际排版由 `foliate-js` 负责。
- `src/store.rs`：精简 SQLite 历史库（`history.db`），按内容 SHA-256 与规范路径双匹配；移出历史仅隐藏并清封面缓存，绝不删除磁盘原文件。
- `foliate-js/`：vendored EPUB 排版引擎，随应用编译进可执行文件。使用 `scripts/update-foliate-js.sh [ref]` 同步上游，版本记录在 `scripts/foliate-js-upstream.txt`。
- `scripts/package-macos.sh`：构建并打包 macOS `.app` 压缩包（含 `packaging/macos/Info.plist` 中的 `.epub` 文档关联）。

## 开发与验证

- `cargo test` 运行元数据解析、历史存储（含文件移动哈希匹配）、URL 解码与资源嵌入测试。
- `cargo check` 检查构建。
- 使用 `VERSO_DATA_DIR=/private/tmp/verso-test` 指向临时目录进行运行验收，避免触碰日常阅读历史。
- `VERSO_DATA_DIR=/private/tmp/verso-test cargo run -- /path/to/book.epub --self-test` 可运行页内目录/选字/高亮自测。
- `scripts/package-macos.sh` 创建可脱离源码目录运行的应用包。
