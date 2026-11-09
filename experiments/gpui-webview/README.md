# macOS GPUI + WebView 阅读器

这个 Cargo 工作区成员使用 GPUI Kit 的 `gpui-webview` 将 WKWebView 嵌入 GPUI 窗口。阅读页直接导入仓库中的 `foliate-js`，不依赖 Electron。仓库根目录执行 `cargo run` 会启动此版本。

运行：

```sh
cargo run --manifest-path experiments/gpui-webview/Cargo.toml -- /path/to/book.epub
```

不传 EPUB 路径时会打开已有书库，可导入 EPUB，点击书名在新的 GPUI 窗口中阅读。书库中的书会恢复 CFI 阅读位置和旧高亮，翻页及新高亮写回原有数据库。字号、字体、行距、字距、段距、内容宽度和明暗主题由 GPUI 工具栏控制，并保存在现有设置表。GPUI 按钮可打开阅读页中的目录和高亮列表；目录支持跳转，高亮支持改色和删除。直接传入路径或在阅读页中另选文件时不写数据库。GPUI 顶部按钮通过 `evaluate_script` 控制翻页、高亮和排版。阅读页通过 `window.ipc.postMessage` 向 Rust 报告就绪、加载、定位、选中文本、CFI 和高亮结果；事件显示在启动终端。

可用 `--book-id ID` 直接打开书库中的指定书籍，方便验证进度恢复。测试时设置 `ORANGE_READER_DATA_DIR` 可使用隔离数据库。

验证时检查：书籍能打开、章节图片与样式正常、左右翻页、选中文字后终端出现 `selection` 和 CFI、点击“GPUI 高亮选中”后文字变黄且终端出现 `highlight`、窗口缩放后布局正常、阅读区域与顶部按钮之间的键盘焦点切换正常。

此版本是仓库根目录 `cargo run` 的默认入口；本地 HTTP 服务仅绑定 `127.0.0.1`，阅读窗口关闭后停止。目录、改色和删除的实际点击效果仍需在 macOS 上人工确认。
