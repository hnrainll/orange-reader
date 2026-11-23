# Orange Reader

macOS EPUB 阅读器。应用界面使用 GPUI Kit，书页由嵌入的 WKWebView 和 foliate-js 排版。阅读器及其网页资源一起编译进可执行文件，运行时不需要 Electron、Node.js 或源码目录。

## 开发运行

需要稳定版 Rust、Xcode 与 Metal Toolchain。

```sh
cargo run
```

启动后可从书库导入 EPUB，也可把 EPUB 拖入书库。`cargo run -- /path/to/book.epub` 可直接打开单本书；`cargo run -- --book-id 1` 可打开书库中的指定书籍。

开发和测试时可用 `ORANGE_READER_DATA_DIR=/private/tmp/orange-reader-test cargo run` 指定隔离书库，避免修改日常阅读数据。

## 功能与数据

- 书库展示封面、作者和进度；支持导入、阅读与删除。
- 书页支持目录、翻页、选字、高亮、改色和删除高亮。
- 阅读位置以 EPUB CFI 保存；字号、字体、行距、字距、段距、内容宽度和明暗主题可保存。
- SQLite 书库沿用旧 Electron 版本的 `books`、`reading_progress` 和 `annotations` 表；旧 GPUI 文字原型的进度可按 fraction 恢复。
- 书库仍保留历史 `native_progress` 和 `native_annotations` 表，不会删除已有数据。旧文字原型创建的段落批注没有 CFI，当前阅读页无法精确显示它们。

## macOS 打包

```sh
scripts/package-macos.sh
```

脚本生成 `dist/Orange Reader-2.0.0-macos.zip`。解压后可把 `Orange Reader.app` 放进“应用程序”目录。脚本使用 ad hoc 签名完成本机验证；公开分发时仍需要开发者证书签名和 Apple 公证。

## 验证

```sh
cargo test
cargo check
```

打包后可将 `.app` 解压到源码目录之外，使用隔离书库检查打开、目录跳转、进度恢复、高亮操作和设置恢复。
对隔离书库中已有的书籍，可用 `cargo run -- --book-id 1 --self-test` 执行页内交互自测；终端出现 `self-test-complete` 表示目录跳转、选字工具、高亮创建或改色及删除均已运行。自测会修改该书的阅读位置，因此只应在隔离书库中运行。

## 代码结构

- `src/main.rs`：GPUI 书库与阅读窗口、本地资源服务和阅读数据桥接。
- `src/reader.html`：WKWebView 中的 foliate-js 阅读交互。
- `src/epub.rs`：导入时读取 EPUB 元数据和封面。
- `src/store.rs`：SQLite 书库、进度、高亮与设置。
- `foliate-js/`：EPUB 排版代码，随应用编译进可执行文件。
- `scripts/package-macos.sh`：创建 macOS 应用包。
