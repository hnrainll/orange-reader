# Orange Reader

跨平台 Epub 阅读器，参考 Apple Books 设计风格。

![Electron](https://img.shields.io/badge/Electron-33-47848F?logo=electron)
![React](https://img.shields.io/badge/React-18-61DAFB?logo=react)
![TypeScript](https://img.shields.io/badge/TypeScript-5-3178C6?logo=typescript)

## 功能

- **书库管理** — 网格展示书籍封面、书名、作者，支持导入和删除
- **导入方式** — 文件选择对话框 / 拖拽导入，自动提取元数据和封面
- **阅读器** — 基于 foliate-js 的分页阅读，支持左右翻页
- **章节导航** — 侧边栏显示目录，点击跳转
- **进度记忆** — 自动保存阅读位置，重新打开恢复
- **阅读设置** — 字体大小调节
- **主题切换** — 明 / 暗模式
- **多窗口** — 可同时打开多本书阅读

## 技术栈

| 模块 | 技术 |
|------|------|
| 框架 | Electron + electron-vite |
| 前端 | React + TypeScript |
| 阅读引擎 | foliate-js |
| 数据存储 | better-sqlite3 |
| 样式 | CSS Modules + CSS Variables |

## 快速开始

```bash
# 安装依赖（自动编译原生模块）
npm install

# 开发模式
npm run dev

# 构建
npm run build
```

## 项目结构

```
orange-reader/
├── src/
│   ├── main/              # Electron 主进程
│   │   ├── index.ts       # 入口、窗口创建、协议注册
│   │   ├── database.ts    # SQLite 数据库
│   │   ├── ipc-handlers.ts# IPC 处理
│   │   └── window.ts      # 阅读器窗口管理
│   ├── preload/           # IPC 桥接
│   └── renderer/          # React 前端
│       ├── pages/         # Library / Reader 页面
│       ├── components/    # BookCard, TOCSidebar, SettingsPanel
│       ├── hooks/         # useTheme
│       └── styles/        # CSS Modules
├── foliate-js/            # epub 渲染引擎（vendored）
└── electron.vite.config.ts
```

## License

MIT
