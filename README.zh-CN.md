# Veil

**在本机与模型 API 之间做脱敏反向代理：客户端仍使用原来的 Key 和登录态，请求正文中的敏感值先被替换为占位符，响应返回时再还原为原文。**

Veil 适合这样的场景：你把配置、日志或 `.env` 内容交给编程助手排查，但希望上游模型只看到可替换的占位符，而不是真实密码、Token、连接串或个人信息。

[下载](https://github.com/guangcheng9/Veil/releases/latest) · [English](README.md) · Apache-2.0 · 无自动遥测

> Veil 是本地脱敏网关，不是完整的 DLP 系统。它能拦截已配置规则能识别的正文内容；Headers、Query、Path、JSON key、未识别的格式、Base64/二进制内容不在脱敏范围内。请先阅读下文「安全边界」。

## 当前能力

- 支持 Anthropic Messages、OpenAI Chat Completions、OpenAI Responses 三类上游协议。
- 支持 JSON 请求/响应与 SSE 流式响应，占位符跨分块时也会还原。
- 内置私钥、API Key、Token、数据库连接串、密码、手机号、身份证、IP 规则；邮箱规则默认关闭。
- 每个客户端可拥有独立本机端口和独立上游，互不覆盖。
- 管理页提供看板、规则、上游、密钥、流量五个页面。
- 规则支持开关、编辑、导入、导出和恢复默认。
- 需要时可通过 HTTP/HTTPS 代理访问上游；留空则强制直连，不继承系统代理。
- 提供 Windows、Linux、macOS 的单文件 Release；管理页已嵌入二进制。

## 工作方式

Veil 作为反向代理运行在客户端和上游模型 API 之间。客户端只需把 Base URL 指向本机：

- Anthropic：`http://127.0.0.1:18787`
- OpenAI 兼容：`http://127.0.0.1:18787/v1`

请求经过 Veil 时，正文中的字符串值会按规则替换为 `{{类型_ULID}}`。响应返回后，Veil 再按当前 Creator 将占位符还原为原文。

示意（示例值均为虚构）：

```text
用户发送：
连接 postgres://demo:demo@db.example.com/app

模型接收：
连接 {{CONNSTR_01ARZ3NDEKTSV4RRFFQ69G5FAV}}
```

模型回复中如果原样使用这个占位符，回到客户端时会再次变成连接串。

### 映射作用域

映射不是按“单次对话”保存，而是按 **Creator + 明文** 保存：

- Creator 从客户端已有的 `x-api-key`、`Authorization` 或 `api-key` 请求头派生；这些请求头仍会原样转发给上游，Veil 只保存相应凭证的哈希及映射数据。
- 同一个 Creator 下，相同明文会复用同一个占位符。
- 更换 API Key、OAuth Token 刷新后会产生新的 Creator，旧映射不会自动复用。
- 没有上述凭证时，所有请求使用同一个匿名 Creator。
- 映射只会在本机持久化，不会作为额外请求头或参数发送给上游。

## 快速开始

### Windows

在 PowerShell 中执行：

```powershell
irm https://raw.githubusercontent.com/guangcheng9/Veil/master/scripts/install.ps1 | iex
```

安装脚本会下载 `veil-windows-x64.exe` 到 `%LOCALAPPDATA%\veil\bin`，加入用户 PATH，并启动 Veil。随后打开一个新终端：

```powershell
veil setup
```

按编号选择需要接入的软件。Veil 会尝试读取该软件当前的 API 地址，为它分配独立的本机端口，再把软件指向 Veil。

如果新增了专用端口，需要同时重启 Veil 和该软件。只修改已有端口的上游时，重启转发即可。

也可以从 [Releases](https://github.com/guangcheng9/Veil/releases/latest) 直接下载 `veil-windows-x64.exe` 后双击运行。

### Linux / macOS

从 [Releases](https://github.com/guangcheng9/Veil/releases/latest) 下载对应二进制：

| 平台 | 产物 |
|---|---|
| Windows x64 | `veil-windows-x64.exe` |
| Linux x64 | `veil-linux-x64` |
| Linux ARM64 | `veil-linux-arm64` |
| macOS Apple Silicon | `veil-macos-arm64` |
| macOS Intel | `veil-macos-x64` |

Linux/macOS 首次运行前添加执行权限：

```bash
chmod +x veil-linux-x64
./veil-linux-x64
```

Release 同时提供 `SHA256SUMS`，建议在分发或企业内部安装前校验文件完整性。

## 支持的软件和协议

### 已适配的软件

`veil setup` 当前支持：

| 软件 | 协议类型 |
|---|---|
| Claude Code | Anthropic |
| Codex | OpenAI |
| PI Agent | 自动 |
| CodeBuddy | 自动 |
| Grok Builder | OpenAI |
| Hermes | 自动 |
| Trae | 自动 |

命令行也可以直接指定：

```powershell
veil setup --clients claude,codex
veil setup --clients all
```

“自动”表示该类客户端使用同一个上游根地址承载其识别到的协议族。已有可读取的客户端地址或已有路由优先；`--upstream` 主要用于未识别客户端和通用兜底上游。

### 手动接入其他工具

只要工具支持修改 Base URL，并且使用下列协议之一，就可以手动接入：

| 协议 | Base URL |
|---|---|
| Anthropic Messages | `http://127.0.0.1:18787` |
| OpenAI Chat Completions / Responses | `http://127.0.0.1:18787/v1` |

当前识别的请求路径包括：

- `/v1/messages`
- `/v1/messages/count_tokens`
- `/v1/chat/completions`
- `/v1/responses`
- `/v1/models`
- `/v1/embeddings`

通用端口 `18787` 只识别上述路径，未识别的路径返回 `404`。按软件创建的 Anthropic/OpenAI 专用端口会把其他路径归入该端口对应的协议族。

在管理页「上游」→「通用」中填写实际上游根地址：

- 可以填写官方 API、公司中转或 LiteLLM 地址。
- Veil 会去掉末尾 `/v1`，再按请求路径重新拼接，避免出现 `/v1/v1/...`。
- 上游修改后需要重启转发才会生效。
- 工具中的 Key 仍使用上游提供的 Key；Veil 不要求额外填写模型 API Key。

## 安全边界与失败行为

### 会处理什么

- 请求体中的 JSON 字符串值，以及非 JSON 文本请求体。
- 响应体中的 JSON 字符串值。
- SSE 响应中的文本 payload。
- 流式响应中跨分块的占位符还原。

### 不会处理什么

- 请求头、响应头、Query、Path、HTTP 方法和 JSON key 不会脱敏；转发层仍会重写 `Host`、`Accept-Encoding`、`Content-Length`、`Transfer-Encoding` 等必要传输头。
- Base64、data URL、多模态二进制字段会跳过错敏，避免破坏图片、音频或文件内容。
- `multipart/form-data` 会被拒绝，不上游。
- 未出现在规则中的自定义秘密格式不会被识别。
- Base64 或二进制附件中的秘密不会被扫描。

### 常见错误

| 状态 | 含义 |
|---|---|
| `404` | 请求路径未被当前监听端口识别 |
| `413` | 请求体超过限制；默认 32 MiB，可配置，上限 128 MiB |
| `415` | 请求为 multipart/form-data |
| `502` | JSON 无法解析、脱敏失败、映射写入失败、上游连接失败或 JSON 响应还原失败 |

无法完成脱敏或映射写入时，Veil 不会把该请求继续转发。若响应中的占位符不属于当前 Creator，Veil 不会越权解密。

## 规则与映射

打开管理页 →「规则」：

- 内置规则覆盖 PEM 私钥、API Key、Token、数据库/中间件连接串、密码赋值、中国内地手机号、身份证和可解析的 IPv4/IPv6。
- IP 规则会处理所有可解析地址，包括公网地址；默认白名单包含 `127.0.0.1`、`localhost`、`0.0.0.0`、`::1`，这些值不会被替换。
- 邮箱规则默认关闭，以减少误报。
- 支持自定义正则和词表。词表适合项目代号、内部服务名等固定文本。
- 支持导出自定义规则、导入自定义规则和恢复内置默认规则。
- 白名单采用精确字符串匹配，命中的内容不会替换。

规则、白名单和上游设置当前都需要**重启转发**后生效。看板上的重启按钮会在存在未应用配置时显示提示。

### 映射存储

映射保存在本机 SQLite 中：

- 敏感映射列使用 AES-256-GCM 加密。
- 相等查询使用 HMAC-SHA256，不使用明文哈希索引。
- 占位符 ID、Creator 哈希等必要元数据不做整行加密。
- 默认主密钥文件为数据目录下的 `master.key`。

如果 `mappings.db` 和 `master.key` 被一起复制，持有者可以解密映射。Veil 不能抵御能够读取整个数据目录的本机攻击者。

## 管理页

默认地址为 `http://127.0.0.1:18788`。首次启动或执行 `veil ui` 时，启动器会自动带上管理令牌；直接手动打开裸地址时，写操作可能因缺少令牌返回 `401`。

| 页面 | 当前内容 |
|---|---|
| 看板 | 服务控制、最近 512 条缓存事件指标、最近 20 分钟请求趋势、更新检查 |
| 规则 | 内置/自定义规则开关与编辑、白名单、导入、导出、恢复默认 |
| 上游 | 三类通用上游、按软件独立上游、可选 HTTP/HTTPS 代理、请求体上限、优雅停止超时 |
| 密钥 | 主密钥轮换、映射清理入口、管理令牌重置 |
| 流量 | 实时查看最近缓存请求，不含正文、Query 或明文机密 |

看板指标来自内存中的最近 512 条流量事件，每 2 秒刷新；不是累计总量。20 分钟趋势也使用同一份缓存，缓存覆盖不足时可能不完整。

流量表只展示方法、路径、状态、是否流式、命中规则类型、上游首包耗时、错误类别和 Creator 短前缀。

## 数据目录与隐私

默认数据目录：

- Windows：`%LOCALAPPDATA%\veil`
- macOS：`~/Library/Application Support/veil`
- Linux：`${XDG_DATA_HOME:-~/.local/share}/veil`

若已存在旧版 `dgw` 数据目录且尚无 `veil` 目录，Veil 会继续使用旧目录。可通过 `VEIL_DATA_DIR` 覆盖。

数据目录通常包含：

- `config.toml`
- `mappings.db`
- `master.key`
- `admin.token`
- 运行时元数据和可选调试日志

Veil 不包含自动遥测或崩溃上报。正常聊天流量只发送到配置的上游 API；安装、手动检查更新和下载更新时会访问 GitHub。

调试日志仅在设置 `VEIL_DEBUG=1` 时写入本地，不记录请求正文、Query、明文机密或完整凭证。

## CLI

```text
veil                         启动 Veil 并打开管理页
veil start --foreground      前台运行，日志直达终端
veil stop                    停止 Veil 进程
veil status                  查看控制面和转发状态
veil setup                   交互式接入已支持的软件
veil setup --clients LIST    非交互接入指定软件
veil setup --upstream URL    设置通用/兜底上游根地址
veil ui                      打印并打开带管理令牌的管理页地址
veil update --check          检查 GitHub Release
veil update                  下载并安装最新版
veil help                    查看帮助
```

常用环境变量：

```text
VEIL_DATA_DIR
VEIL_MASTER_KEY
VEIL_BIND
VEIL_MODE=desktop|server
VEIL_UPSTREAM_PROXY
VEIL_DEBUG
VEIL_GITHUB_MIRROR
```

旧的 `DGW_*` 环境变量仍兼容。

## 已知限制

- 仅支持上文列出的协议路径，不是任意 API 的通用透明代理。
- 不支持 multipart 和附件扫描。
- Base64、data URL 和多模态二进制内容会原样转发。
- Headers、Query、Path 和 JSON key 不脱敏。
- 规则、白名单和上游修改需要重启转发；请求体上限修改也需要重启转发后生效。
- 当前版本中，上游页的“映射 TTL”字段会保存到配置，但尚未接入 SQLite 映射存储；按 Creator 清除映射的接口也尚未实际删除数据库记录。不要把这两项当作已完整可用的清理机制。
- 主密钥轮换会替换 `master.key`，但当前运行中的映射存储不会热重载；需要重启 Veil 才能按新密钥重新打开存储。
- 未签名的 Release 二进制在 macOS 上可能触发 Gatekeeper 提示。

## 更多文档

- [使用说明](docs/使用.md)
- [启动、测试与构建](LAUNCH.md)
- [Claude Code 插件](plugin/claude-code/README.md)

## License

Apache-2.0