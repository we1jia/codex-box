# ADR 0008 · Codex Box 配置命名空间与第三方导入

> 状态：Accepted  
> 日期：2026-06-27  
> 关联：ADR-0005、ADR-0006、ADR-0007  

## 背景

最近围绕 MiniMax、OpenCodex、`1455` 与 `8765` 的排查暴露出两个问题：

1. Codex Box 参考了 OpenCodex 的配置逻辑，但不应该继承 OpenCodex 的运行时端口和主配置目录。
2. Codex Box 当前仍把 `~/.opencodex/` 当成主写入目录，导致 UI 文案和诊断里出现 OpenCodex 命名，用户会误以为 Codex Box 依赖或启动了 OpenCodex。

用户明确反馈：项目叫 **Codex Box**，配置也应该有自己的命名空间。OpenCodex、Codex++、CC Switch、Cockpit Tools 这类工具可以作为兼容来源，但不应该成为 Codex Box 的主目录。

## 决策

Codex Box 自 v0.3.4 起采用独立主目录：

```text
~/.codex/codex-box/
├─ providers.json
├─ custom_model_catalog.json
├─ routes.json
├─ imports/
├─ backups/
├─ logs/
├─ reports/
├─ runtime-state.json
├─ inject-map.json
└─ settings.json
```

其中：

- `~/.codex/codex-box/providers.json` 是 Codex Box 主 provider 路由文件。
- `~/.codex/codex-box/custom_model_catalog.json` 是 Codex Box 主模型目录。
- `~/.opencodex/*.json` 只作为 OpenCodex 只读扫描和导入来源，不参与 Codex Box 实时模型列表、代理路由或 API 服务链路兜底。
- `~/.codex/config.toml` 仍是 Codex App 的唯一主配置文件，Codex Box 只写最小注入。

目标 `~/.codex/config.toml` 写法：

```toml
model = "minimax"
model_provider = "codex_model_router_v2"
model_catalog_json = "/Users/<user>/.codex/codex-box/custom_model_catalog.json"

[model_providers.codex_model_router_v2]
name = "Codex API Service"
base_url = "http://127.0.0.1:1455/v1"
wire_api = "responses"
requires_openai_auth = true
experimental_bearer_token = "PROXY_MANAGED"
supports_websockets = false
```

## 两个代理的处理

Codex Box 只保留一条本地代理链路：

```text
Codex App -> http://127.0.0.1:1455/v1 -> Codex Box proxy -> backend provider
```

`8765` 只属于 OpenCodex 的历史/外部工具上下文：

- 不写入 Codex Box 生成的配置。
- 只在兼容扫描中作为“外部 OpenCodex 残留”提示。
- 如果在 `config.toml`、备份或导入来源中检测到 `127.0.0.1:8765`，导入预览应建议迁移到 `1455` 或当前实际运行端口。

`openai_base_url` 与 `[model_providers.<id>].base_url` 不再并行制造两套代理入口。Codex Box 的默认 BYOK/MultiRouter 入口使用稳定会话归属 Provider `codex_model_router_v2`，并把该 provider 的 `base_url` 写到 Codex Box 本地代理。用户仍可在历史修复工具里选择旧 `codex_local_access` 等历史桶进行归并；只有在用户明确选择内置 `openai` 兼容模式时，才写顶层 `openai_base_url`。

## 第三方导入兼容

Codex Box 应支持“扫描 -> 预览 -> 导入 -> 应用”的一键导入，而不是直接覆盖当前配置。

导入来源：

| 来源 | 扫描路径 | 处理方式 |
|---|---|---|
| Codex 原生 | `~/.codex/config.toml`、`~/.codex/*.config.toml`、`~/.codex/config.toml.bak*` | 提取 profile、model_provider、provider 表和历史 base_url |
| Codex Box | `~/.codex/codex-box/backups/*` | 作为回滚和恢复来源 |
| OpenCodex | `~/.opencodex/providers.json`、`~/.opencodex/custom_model_catalog.json` | 只读扫描，预览后导入到 Codex Box 主目录 |
| Codex++ | `CodexPlusPlus` provider、provider-sync 备份、相关 `config.toml` 快照 | 只读扫描，导入为普通 provider/profile |
| CC Switch | 最终写入 Codex 的 provider/profile 配置 | 不依赖其内部数据库格式 |
| Cockpit Tools | 用户手动选择 Data Directory 后扫描 config/account/backup 类文件 | 只读扫描，脱敏预览 |

导入流程：

```text
扫描来源
  -> 识别 provider/model/profile/backup
  -> 脱敏展示导入预览
  -> 用户选择导入项
  -> 写入 ~/.codex/codex-box/providers.json 与 custom_model_catalog.json
  -> 生成 ~/.codex/config.toml diff
  -> 用户确认后应用
```

Runtime 页面应把导入放在连接链路的自然位置：当 `~/.codex/codex-box/providers.json` 为空，或模型目录里的 `backend_provider` 在 live API 服务里找不到时，`连接 Codex` 页面显示“导入外部配置”提示。该提示只调用 `opencodex_import_preview` / `opencodex_import_apply` 做预览和确认导入，不把 `~/.opencodex` 当作代理运行时或实时兜底。

导入 OpenCodex provider 时不能原样复制明文 `api_key`。Codex Box 必须先解析来源文件，再投影为自己的 schema：明文 key 写成 `${PROVIDER_API_KEY}` 引用，并只注入到当前 Codex Box 进程环境，目标 `providers.json` 不落明文。预览 diff 必须展示最终投影后的写入内容，而不是脱敏后的原始来源文件。

## UI 设计要求

按 TasteSkill 的 redesign protocol，本次属于 **Redesign - Preserve**：

- 保留 Codex Box 的 Mac Dashboard、frosted glass、shadcn/Tailwind 技术路线。
- 降低视觉噪音，减少多层卡片嵌套。
- 普通用户默认只看到“模型来源、可选模型、启用到 Codex、导入配置”。
- 路径、hash、raw schema、`wire_api`、`inject-map` 都放入高级详情。
- 诊断文案必须说明“发生了什么 + 怎么修”，不要直接抛底层字段名。

设计读数：

- `DESIGN_VARIANCE: 5`
- `MOTION_INTENSITY: 3`
- `VISUAL_DENSITY: 6`

理由：这是本地开发者工具的密集产品 UI，不是营销落地页。需要清晰、可信、可恢复，避免炫技动画。

## 影响

正面：

- Codex Box 获得独立命名空间，不再把 OpenCodex 暴露成主路径。
- `1455` 与 `8765` 语义彻底分开。
- 后续可以安全加入 OpenCodex、Codex++、CC Switch、Cockpit Tools 的一键导入。

成本：

- 现有 `opencodex_config_read` 等命令名可暂时保留以减少前端改动，但返回路径必须切到 `~/.codex/codex-box/*`。
- 文档中所有“主写入 `~/.opencodex`”表述需要改为“Codex Box 主目录 + OpenCodex 兼容导入”。
- 代理路由读取逻辑只能从 Codex Box 主目录读取；旧 OpenCodex 文件只允许出现在导入/残留提示，不允许作为实时兜底。

## 红线

- 不自动删除或修改第三方工具目录。
- 不读取、抓取或复用官方账号 token。
- 不把明文 API Key 写入日志、诊断报告或 `config.toml`。
- 任何写入仍必须 backup、diff、confirm、atomic write、rollback。
