# Orange Reader 项目说明

当前主应用为 Rust + GPUI，入口是 `src-native/main.rs`，使用 `cargo run` 启动。`src/`、`foliate-js/` 和 npm 配置是迁移前的 Electron 实现，仅作为参考保留。

## 结构

- `src-native/epub.rs`：读取 EPUB ZIP、OPF、spine 与 XHTML 文本。
- `src-native/store.rs`：SQLite 书库、阅读进度、标注及旧数据迁移。
- `src-native/main.rs`：GPUI 书库与阅读窗口。

## 数据兼容

原 Electron 书库数据库与复制的 EPUB 文件继续使用。原 CFI 进度通过 fraction 近似定位；有文字内容的旧标注在阅读时按段落文本匹配迁移。`ORANGE_READER_DATA_DIR` 可指定隔离数据目录。

## 当前限制

原生阅读器目前提取 XHTML 文本分页，不执行 EPUB CSS，不显示章节内图片，也不支持固定版式。分页按字符数估算，部分页面仍需滚动。开发时用 `cargo check`、`cargo test` 验证。
