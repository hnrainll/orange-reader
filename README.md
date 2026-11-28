# Verso

轻量级 macOS 原生电子书阅读器，支持 **EPUB、AZW3 / MOBI（无 DRM）、CBZ、FB2 / FBZ / FB2.ZIP**。界面使用 GPUI Kit，书页由嵌入的 WKWebView 和 foliate-js 排版。阅读器及其网页资源一起编译进单个可执行文件，运行时无需依赖源码目录。

Verso **不做书架或文件拷贝管理**——你的电子书留在原本的文件夹里，像使用 macOS 预览或 IINA 一样自然打开阅读；应用仅记录**最近阅读历史**（看过哪本、文件在什么位置、看到了哪里）以及对应的高亮与阅读偏好。

## 开发运行

需要稳定版 Rust、Xcode 与 Metal Toolchain。

```sh
cargo run
```

- 无参数启动：打开「最近阅读」窗口，可点击「打开电子书」（`⌘O`）或直接把电子书拖入窗口开始阅读。
- 直接打开书籍：`cargo run -- /path/to/book.epub`（同样支持 `.azw3`/`.mobi`/`.cbz`/`.fb2`/`.fbz`/`.fb2.zip`）直接弹出阅读窗口，并自动记录历史与进度（支持同时打开多本书窗口）。
- 打包安装后：支持在 Finder 中双击电子书或通过右键「打开方式 → Verso」打开。
- 隔离测试数据：`VERSO_DATA_DIR=/private/tmp/verso-test cargo run` 可使用临时目录，不影响日常阅读记录。

## 功能与数据

- **最近阅读历史**：按最近阅读时间排序，显示书名、作者、磁盘文件路径、上次阅读章节与百分比进度；支持点击继续阅读、在 Finder 中显示位置、移出历史（仅从列表隐藏并清理封面缓存，保留进度与高亮，**绝不删除原文件**）。
- **路径 + 内容哈希双匹配**：即使把 `.epub` 移动到别的目录或重命名，再次打开时会自动通过 SHA-256 识别为同一本书，自动更新路径并续上原有 CFI 进度与高亮；若历史中的文件已移动或删除，会给出提示并支持「重新定位」。
- **阅读体验**：支持目录跳转、方向键/空格翻页、点击正文显隐工具栏、选字浮层、多色高亮与删除；阅读位置以 EPUB CFI 保存；字号、字体、行距、页宽、上下边距、字距、段距、单/双页布局与 4 套主题自动保存。
- **数据位置**：默认保存在系统配置目录的 `verso/history.db`（及 `covers/` 封面缓存），不拷贝 `.epub` 文件。

## macOS 打包

```sh
make package
# 或直接打包并安装到 /Applications：
make install
```

脚本生成 `dist/Verso-2.0.0-macos.zip`。解压后可把 `Verso.app` 放进“应用程序”目录。脚本使用 ad hoc 签名完成本机验证；公开分发时仍需要开发者证书签名和 Apple 公证。

## 验证

```sh
make check
make test
```

对任意本地 EPUB，可用 `VERSO_DATA_DIR=/private/tmp/verso-test cargo run -- /path/to/book.epub --self-test` 执行页内交互自测；终端出现 `self-test-complete` 表示目录跳转、选字工具、高亮创建/改色及删除均已运行。

## 代码结构

- `src/main.rs`：应用入口、窗口协调、macOS 打开文件事件与菜单快捷键。
- `src/history_view.rs`：最近阅读窗口 UI 与交互。
- `src/reader_window.rs`：多窗口阅读器与 WebView IPC 消息处理。
- `src/server.rs`：loopback 本地资源 HTTP 服务与内嵌 `foliate-js/` 资源。
- `src/settings.rs`：阅读排版与主题设置模型及校验。
- `src/reader.html`：WKWebView 中的 foliate-js 阅读交互。
- `src/book.rs`：多格式识别（EPUB / AZW3 / MOBI / CBZ / FB2 / FBZ）与元数据/封面解析调度。
- `src/epub.rs`：EPUB OPF 元数据和封面解析。
- `src/store.rs`：SQLite 阅读历史、进度、高亮与设置（`history.db`）。
- `foliate-js/`：EPUB 排版代码，随应用编译进可执行文件。升级可使用 `scripts/update-foliate-js.sh [ref]`，当前同步的上游版本记录在 `scripts/foliate-js-upstream.txt`。
- `scripts/package-macos.sh`：创建 macOS 应用包。
