# macOS GPUI + WebView 验证

这个独立 Cargo 工程使用 GPUI Kit 的 `gpui-webview` 将 WKWebView 嵌入 GPUI 窗口。阅读页直接导入仓库中的 `foliate-js`，不依赖 Electron。

运行：

```sh
cargo run --manifest-path experiments/gpui-webview/Cargo.toml -- /path/to/book.epub
```

不传 EPUB 路径时会打开已有书库，可导入 EPUB，点击书名在新的 GPUI 窗口中阅读。也可以在阅读页中选择文件。GPUI 顶部按钮通过 `evaluate_script` 控制翻页和高亮。阅读页通过 `window.ipc.postMessage` 向 Rust 报告就绪、加载、定位、选中文本、CFI 和高亮结果；事件显示在启动终端。书库使用现有数据库，但此阶段尚未把阅读进度或新高亮写回数据库。

验证时检查：书籍能打开、章节图片与样式正常、左右翻页、选中文字后终端出现 `selection` 和 CFI、点击“GPUI 高亮选中”后文字变黄且终端出现 `highlight`、窗口缩放后布局正常、阅读区域与顶部按钮之间的键盘焦点切换正常。

这是独立实验，没有替换现有 `cargo run` 应用；本地 HTTP 服务仅绑定 `127.0.0.1`，退出程序即停止。
