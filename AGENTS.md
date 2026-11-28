# Verso 项目说明

仓库是文档驱动的 macOS 原生电子书阅读器（类似预览 / IINA），支持 EPUB、AZW3 / MOBI（及 AZW / PRC，无 DRM）、CBZ 与 FB2 / FBZ / FB2.ZIP：不做书架/书库文件管理，直接打开磁盘原路径的电子书阅读，仅记录最近阅读历史（文件路径、阅读进度、章节与高亮）。

`cargo run` 启动 GPUI Kit 最近阅读窗口；传入电子书路径或从 Finder 打开时直接弹出 WKWebView 阅读窗口，书页由 `foliate-js/` 渲染。正式入口在 `src/main.rs`，无 Electron 或 Node.js 运行时。

## 结构

- `src/main.rs`：应用入口、`AppState` 窗口协调、macOS 打开文件钩子（`on_open_urls`/`on_reopen`）与菜单快捷键。
- `src/history_view.rs`：最近阅读窗口（`HistoryView`）UI 与交互。
- `src/reader_window.rs`：多窗口阅读器（`ReaderWindow`）与 WebView IPC 消息处理（含 `foliate-js` 元数据/封面回传）。
- `src/server.rs`：loopback 本地资源 HTTP 服务与内嵌 `foliate-js/` 资源。
- `src/settings.rs`：阅读排版与主题设置模型（`ReaderSettings`）及校验。
- `src/reader.html`：目录、翻页、选字、高亮与阅读设置的网页端交互。
- `src/book.rs`：多格式识别（EPUB / MOBI / AZW3 / CBZ / FB2 / FBZ）与元数据/封面解析调度。
- `src/epub.rs`：EPUB OPF 标题、作者与封面解析，实际排版由 `foliate-js` 负责。
- `src/store.rs`：精简 SQLite 历史库（`history.db`），按内容 SHA-256 与规范路径双匹配；移出历史仅隐藏并清封面缓存，绝不删除磁盘原文件。
- `foliate-js/`：vendored 电子书排版引擎，随应用编译进可执行文件。使用 `scripts/update-foliate-js.sh [ref]` 同步上游，版本记录在 `scripts/foliate-js-upstream.txt`。
- `scripts/package-macos.sh`：构建并打包 macOS `.app` 压缩包（含 `packaging/macos/Info.plist` 中的电子书文档关联）。

## 开发与验证

- `cargo test` 运行元数据解析、历史存储（含文件移动哈希匹配）、URL 解码与资源嵌入测试。
- `cargo check` 检查构建。
- 使用 `VERSO_DATA_DIR=/private/tmp/verso-test` 指向临时目录进行运行验收，避免触碰日常阅读历史。
- `VERSO_DATA_DIR=/private/tmp/verso-test cargo run -- /path/to/book.epub --self-test` 可运行页内目录/选字/高亮自测。
- `scripts/package-macos.sh` 创建可脱离源码目录运行的应用包。
