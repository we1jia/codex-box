# ADR 0004 · OpenCodex 能力复现策略

> 状态：Accepted
> 日期：2026-06-25
> 决策者：项目组

---

## 背景

Codex Box v0.1 的定位是“本地配置与网关管理器”。经过进一步对齐，真实目标已经升级为：把 OpenCodex 的核心能力复现进 Codex Box，并以 Codex Box 自有的 Tauri 桌面控制台承载。

OpenCodex 的关键价值是：

- 在目标机器上启动 gateway
- 通过浏览器访问目标机器上的 Codex 工作流
- 支持本机 / LAN / 移动端访问
- 提供访问密码、health、日志和运行状态

同时，OpenCodex 是 AGPL-3.0 项目。Codex Box 需要明确许可证和实现边界。

---

## 决策

Codex Box 采用“功能复现，不复制实现”的策略：

1. **复现功能结果**
   - gateway 启停
   - host / port / local URL / LAN URL
   - 访问密码
   - health endpoint
   - 日志脱敏与查看
   - Codex Desktop / CLI / CODEX_HOME 检测
   - 移动端访问入口

2. **保持独立代码路径**
   - 不复制 OpenCodex 源码、UI、长段文案。
   - `docs/references/opencodex-technical-notes.md` 只记录接口事实、运行方式、安全边界和架构判断。
   - 如未来必须直接使用 OpenCodex 代码，必须先单独评估 AGPL 义务并更新 ADR。

3. **允许过渡期外部托管**
   - 当前可保留外部 OpenCodex checkout 启动能力用于验证。
   - 该能力必须被标记为过渡态。
   - 长期必须移除硬编码个人路径，改成 Codex Box 自有 `Embedded` runtime 或用户显式配置路径。

4. **不混用认证边界**
   - 官方订阅通道只做状态识别。
   - 不读取、不复用、不转发官方账号 token。
   - 第三方 API 只走 `base_url + env secret`。

---

## 影响

### 正面

- 产品目标和用户真实需求对齐。
- Gateway / Mobile Access / Codex Runtime 成为主线能力，而不是 P2 增强项。
- 可以继续保留 Codex Box 的自有 UI 和配置管理优势。

### 成本

- PRD、数据模型、架构文档需要升级到 v0.2。
- 原先“只做 gateway preset”的路线需要回退。
- M2.5 需要优先处理 runtime、auth、health、log、LAN 访问等底座能力。

### 风险

- 许可证风险：直接复制 OpenCodex 代码会触发 AGPL 义务。
- 安全风险：LAN 访问如果默认开放，会暴露本机 Codex 工作流。
- 维护风险：Codex Desktop 内部实现可能变化，任何依赖内部资源的能力都必须隔离为只读检测或实验功能。

---

## 红线

- 不抓取任何账号 token。
- 不绕过 OpenAI 官方登录。
- 不规避 rate limit 或账号配额限制。
- 不默认修改 Codex Desktop 内部文件。
- 不默认解包官方 `app.asar`。
- 不接管系统全局代理。
- 不上传用户配置。
- LAN 访问必须配置访问密码。

---

## 后续动作

1. `PRD.md` 升级为 v0.2。
2. `docs/data-model/v0.2.md` 补齐 gateway / runtime / access 模型。
3. `docs/architecture/v0.2.md` 作为主架构方案。
4. 删除 M0 阶段的旧执行计划文档。
5. 后续实现从 `ExternalOpenCodexCheckout` 过渡到 `Embedded` runtime。
