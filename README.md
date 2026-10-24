# OmniSlice 幽灵窗

将网页局部、系统信息和常用工具变成桌面切片。基于 Tauri 2 与 Rust，优先支持 Windows。

项目正在开发第一阶段技术原型，当前功能和验收进度以 [项目规划](PROJECT_PLAN.md) 为准。

## 核心体验

- 网页矩形裁剪与等比缩放，窗口多余区域透明。
- 每个独立窗口分别设置隐私模式、鼠标穿透、文字与背景透明度。
- 隐私窗口移出隐藏，移入恢复，隐藏时默认遮蔽预览并静音。
- 独立窗口置顶排序，窗口内通过胶囊切换切片。
- 可选标签页内存回收，默认关闭，支持始终保持运行的例外。

以上为规划能力，尚未通过验证的事项不会标记为已完成。

## 开发环境

需要 Node.js、Rust MSVC 工具链、Microsoft C++ Build Tools、Windows SDK 与 WebView2。安装要求见 [Tauri 官方文档](https://v2.tauri.app/start/prerequisites/)。

当前使用 Node.js 24、Rust 1.99.1、Tauri 2.12.5、Svelte 5 和 TypeScript，依赖由两个锁文件固定。

```powershell
npm ci --ignore-scripts
npm run tauri dev
```

首次启动创建一个隐私窗口和一个常显窗口。原型每窗承载一个网页；多标签聚合和节能回收属于后续阶段。可运行 `npm run fixtures`，再打开 `http://127.0.0.1:4174/` 验证动态文字、图表、输入与声音。

恢复入口为托盘菜单“恢复所有窗口”和 `Ctrl+Alt+Shift+O`。快捷键被其他软件占用时使用托盘。重复点击“置顶”会提升该窗口，另有“取消置顶”按钮。配置保存在应用配置目录的 `workspace.json`，包含网址，勿将其提交到仓库。

检查命令：

```powershell
npm run check
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --all --check
cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets --features tauri/custom-protocol -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked --features tauri/custom-protocol
```

原生测试入口为 `scripts/windows-smoke.mjs`，需要先构建 exe、启动 fixture 和匹配 WebView2 版本的 `tauri-driver` / `msedgedriver`。本地原生证据写入 `.workflow/native-evidence`，不会推送。每步设计、验证和遗留问题见 [实施记录](DESIGN.md)。第一阶段尚在验收，未完成项以规划及实施记录为准。

## 参与贡献

请阅读 [贡献指南](CONTRIBUTING.md)。开发通过功能分支和 Pull Request 进行，提交前运行适用检查，不提交本机配置、凭据或构建产物。

## 许可证

[MIT](LICENSE) © 2026 1000ttank。
