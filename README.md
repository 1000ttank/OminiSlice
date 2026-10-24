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

## 参与贡献

请阅读 [贡献指南](CONTRIBUTING.md)。开发通过功能分支和 Pull Request 进行，提交前运行适用检查，不提交本机配置、凭据或构建产物。

## 许可证

[MIT](LICENSE) © 2026 1000ttank。
