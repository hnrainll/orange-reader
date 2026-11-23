# Orange Reader 项目说明

仓库是 macOS Rust 应用。`cargo run` 启动 GPUI Kit 书库和 WKWebView 阅读窗口；书页由 `foliate-js/` 渲染。正式入口在 `src/main.rs`，没有 Electron 或 Node.js 运行时。

## 结构

- `src/main.rs`：书库、阅读窗口、loopback 资源服务、WebView IPC。
- `src/reader.html`：目录、翻页、选字、高亮与阅读设置的网页端交互。
- `src/epub.rs`：导入时解析 EPUB 元数据和封面，实际阅读由 foliate-js 负责。
- `src/store.rs`：兼容旧版 SQLite 书库和阅读数据。
- `scripts/package-macos.sh`：构建并打包 macOS `.app` 压缩包。

## 开发与验证

`cargo test` 运行元数据、数据存储和资源嵌入测试。`cargo check` 检查构建。使用 `ORANGE_READER_DATA_DIR` 指向临时目录进行运行验收，避免触碰现有书库。`scripts/package-macos.sh` 创建可脱离源码目录运行的应用包。

旧 Electron 与 GPUI 文字原型已从源码中清理；历史 `native_progress` 和 `native_annotations` 表仍保留在用户数据库中以防数据丢失。
