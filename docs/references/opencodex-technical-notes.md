# OpenCodex 技术拆解

> 来源：`/Users/liuweijia/Desktop/AI/OpenCodex`
> 版本来源：`RyensX/OpenCodex` main 源码包
> 拆解时间：2026-06-24

---

## 1. 项目定位

OpenCodex 不是第三方模型 API 网关，而是 **Codex Desktop 的远程 Web 中间层**。

它的目标是：

- 在本机启动一个 gateway
- 复用已安装的 Codex Desktop 官方资源
- 让手机、平板或另一台电脑通过浏览器访问目标机器上的 Codex
- 提供访问密码、局域网访问、插件、移动端优化

这和 Codex Box 的关系：

- 可参考：gateway 启停、状态、端口、日志、认证、插件、诊断、官方资源只读扫描
- 不直接照搬：隐藏 Electron runtime、patch 官方 IPC、远程操作 Codex Desktop

---

## 2. 目录结构

```text
OpenCodex/
├─ launcher/              # Electron 桌面启动器
├─ gateway/               # Node/Electron gateway
│  ├─ runtime/            # HTTP/WS/IPC 主逻辑
│  ├─ runner/             # 隐藏官方 Electron runner
│  └─ src/official/       # Codex Desktop app.asar 扫描和缓存
├─ web-shell/             # 浏览器壳、bridge polyfill、插件系统
├─ shared/                # i18n、版本信息
└─ docs/                  # 插件文档
```

---

## 3. 核心链路

### 3.1 Launcher

文件：`launcher/main.cjs`

职责：

- 管理 host / port / password / plugin dirs
- 写入 `config.yaml`
- 启动 gateway 子进程
- 展示 gateway 状态和本机/局域网访问地址
- 管理日志轮转
- 准备官方 Electron runtime

可借鉴到 Codex Box：

- `Gateway` 页面可以展示 host、port、进程状态、启动时间、局域网地址、日志路径
- `Settings` 可以管理 gateway host mode、port、访问密码、插件目录
- 日志必须轮转和脱敏

### 3.2 官方资源扫描

文件：

- `gateway/src/official/CodexAsarScanner.ts`
- `gateway/src/official/AsarWebviewExtractor.ts`
- `gateway/src/official/OfficialBundleCache.ts`

做法：

- 扫描本机已安装的 Codex Desktop
- 找到 `app.asar`
- 只解压白名单资源到 runtime cache
- 官方安装目录保持只读
- 解压路径做越界防护

可借鉴到 Codex Box：

- `Diagnostics` 可做 Codex Desktop 安装检测
- `Settings` 可允许用户显式指定 Codex Desktop 路径
- 任何“读取官方资源”的能力都要只读、白名单、缓存隔离

不建议进入当前阶段：

- Codex Box 目前定位是配置与网关管理器，不应默认读取/解包 Codex Desktop 内部资源
- 这类能力应归为实验功能或独立 Gateway 插件

### 3.3 Gateway HTTP / WebSocket

文件：

- `gateway/runtime/server.cjs`
- `gateway/runtime/ipc/ws-hub.cjs`
- `gateway/runtime/http/static-assets.cjs`

做法：

- HTTP 提供 `/api/health`、auth、静态资源、IPC invoke
- WebSocket 负责浏览器和隐藏官方 runtime 的消息转发
- 日志只记录路由摘要，不记录正文、prompt、文件内容
- WebSocket 支持压缩和慢消息诊断

可借鉴到 Codex Box：

- Gateway 诊断状态可以统一成 health endpoint
- 日志字段应只保留 route、method、status、latency、requestId 摘要
- `Diagnostics` 可分离 HTTP health、WS health、provider health

不直接照搬：

- Codex Box 当前不做官方 IPC 转发
- 不应为了模型切换去 patch 官方 renderer 或 IPC

### 3.4 认证

文件：`gateway/runtime/http/auth.cjs`

做法：

- `config.yaml` 里支持访问密码
- 首次启动把明文密码升级为 `sha256-v1:<hash>`
- 登录时前端提交 password hash，不传明文
- token 只保存在内存，不落盘
- cookie 使用 `HttpOnly; SameSite=Lax`
- 登录失败有 rate limit

可借鉴到 Codex Box：

- 如果以后开放本地 gateway Web 访问，必须有访问密码
- token 只放内存，不写配置
- 登录失败 rate limit
- secret 和密码不进日志

### 3.5 插件系统

文件：

- `docs/PLUGINS.md`
- `web-shell/opencodex-plugin-system.js`
- `gateway/runtime/core/plugin-assets.cjs`

做法：

- gateway 扫描内置和外部插件目录
- 插件普通 `<script>` 方式加载
- 插件有注册、启用、激活、dispose 生命周期
- 插件无沙箱，只适合可信插件

可借鉴到 Codex Box：

- M5/M6 后可以考虑 `Gateway Preset / Adapter Plugin`
- 插件启用状态进入 Settings
- 插件目录要显式配置，不自动信任外部路径

当前不建议做：

- 不先引入无沙箱插件系统
- 先把 provider preset、gateway preset 做成内置配置，比插件更稳

---

## 4. 对 Codex Box 的启发

### 应该吸收

1. `Gateway` 页面要真实管理 gateway 生命周期：
   - 启动 / 停止 / 重启
   - host / port
   - local URL / LAN URL
   - 进程状态
   - 日志路径
   - health check

2. `Diagnostics` 页面要增加：
   - Codex Desktop 是否安装
   - Codex CLI / `CODEX_HOME` 是否存在
   - gateway 端口是否占用
   - provider base_url 是否可达
   - secret env 是否存在

3. `Settings` 页面要增加：
   - gateway 默认监听地址
   - gateway 默认端口
   - 是否允许 LAN 访问
   - 访问密码
   - 日志大小和保留策略

4. `Providers` 和 `Profiles` 仍保持 Codex Box 主线：
   - profile 是用户切换入口
   - provider 是模型来源
   - gateway 是高级 provider 类型

### 不应该吸收

1. 不默认 patch Codex Desktop IPC
2. 不默认解包官方 `app.asar`
3. 不做远程控制 Codex Desktop
4. 不抓取或复用官方账号 token
5. 不把 OpenCodex 的浏览器 shell 作为 Codex Box 主界面

---

## 5. 建议落地顺序

### M2

- Profiles / Providers 接真实 config 数据
- Provider kind 增加 `local_gateway`
- Gateway 页面先展示配置和状态骨架

### M3

- Diagnostics 增加 Codex Desktop / Codex CLI / gateway port 检查
- Network 增加 LAN / local 访问风险提示

### M5

- 实现本地 gateway 进程管理
- 支持 OpenCodex / codex-proxy / CLIProxyAPI 作为 gateway preset
- 增加日志查看、health endpoint、端口冲突检测

### M6+

- 再考虑插件系统或 adapter 扩展系统

