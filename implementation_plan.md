# Codex Box Runtime 重构计划

## 本轮目标

把 `Codex Box 运行时` 从工具按钮堆叠页，收敛成用户能判断和操作的主流程：

1. 当前能不能用：展示 Codex 下拉接入是否完整。
2. 为什么不能用：把本地代理、Codex config、模型目录、真实上游、环境变量缺口放在同一条链路里。
3. 下一步点哪里：启动本地连接、生成 BYOK 激活 diff、预览 `/v1/models`、恢复原配置。

## 边界

- 样式沿用现有 Mac Dashboard / frosted glass 方向。
- 功能逻辑参考 OpenCodex 的链路：模型目录 -> provider 配置 -> 本地 OpenAI-compatible 代理 -> Codex 下拉。
- 不复制 OpenCodex UI、源码或长文案。
- 不抓取官方 token，不绕过登录，不 patch Codex Desktop 内部文件。
- 本轮优先重构信息架构和操作路径，不扩大到后台协议实现。

## 改动文件

- `src/pages/WorkspacePages.tsx`
- `src/locales/zh.json`
- `src/locales/en.json`

## 菜单收敛追加

当前导航只保留客户能理解的 BYOK 主线入口：

1. 总览
2. 模型列表
3. 连接 Codex
4. 设置

`API 服务` / `Provider Routes` 属于实现层，不作为客户主菜单出现。用户在“模型列表”里填写模型名、接口地址和 API Key 即可；后台可以继续维护 `providers.json` 作为兼容和高级配置。

旧的 `Profiles` / `Providers` / `Diagnostics` 不再出现在菜单、快捷键和总览入口里；诊断能力后续应合并进 `连接 Codex` 的链路检查，而不是作为独立主菜单分散用户注意力。
