# Veil 启动 / 测试 / 打包方案

> 本文档按**使用场景**组织，每个方案内整合前端（`ui/`）与后端（`crates/veil` workspace）步骤，不再按前后端分层级。
>
> 默认端口：管理 UI `18788`，代理 `18787`，绑定 `127.0.0.1`。

---

## 0. 前置条件

| 工具 | 用途 | 版本要求 |
| --- | --- | --- |
| Rust toolchain (stable) | 编译后端 + workspace 测试 | CI 用 stable |
| Node.js | 前端 dev / 测试 / 构建 | `22.x` |
| npm | 前端依赖管理 | 随 Node 提供 |

Windows 平台额外：`cargo` 默认走 MSVC，需安装 Visual Studio Build Tools。

首次拉取依赖：

```bash
cargo fetch              # 仓库根目录
cd ui && npm ci && cd ..  # 前端依赖
```

关键事实（影响所有方案）：
- 前端产物 `ui/dist/` 会被后端在**编译期**通过 `rust-embed` 嵌入（[crates/veil/src/admin/mod.rs:14-16](crates/veil/src/admin/mod.rs#L14-L16)）。所以**编译/打包后端前必须先有 `ui/dist`**，否则 UI fallback 返回 `ui not built`。
- `VEIL_SKIP_UI_BUILD=1` 环境变量可跳过该嵌入校验，仅用于纯后端测试/CI，**不能用于分发产物**。

---

## 1. 方案 A：本地 Dev 开发联调（前后端）

**目的**：改 UI 或后端代码，热重载，前后端实时联调。

前提：后端跑在 `18788`，Vite dev server 把 `/api/*` 反向代理过去并自动注入 `admin.token`（见 [vite.config.ts](ui/vite.config.ts)）。

```bash
# 终端 1 — 后端（前台运行，日志可见）
cargo run -p veil -- start --foreground

# 终端 2 — 前端
cd ui && npm run dev
```

- 浏览器访问 Vite 提示地址（通常 `http://localhost:5173`）
- 后端控制台地址：`http://127.0.0.1:18788/?token=<admin.token>`
- 改后端 Rust 代码 → 重跑终端 1；改前端代码 → Vite HMR

仅看 UI、不改后端时，也可用已安装的 release 二进制代替终端 1：

```bash
veil start --foreground   # 或双击 veil.exe
```

---

## 2. 方案 B：测试方案（前后端 + 格式）

**目的**：本地跑通 CI 等价检查。

```bash
# 2.1 前端单测 + 类型检查 + 构建
cd ui && npm test && npm run build && cd ..

# 2.2 后端全 workspace 测试（跳过 UI 嵌入校验）
VEIL_SKIP_UI_BUILD=1 cargo test --workspace

# 2.3 针对性后端集成测试
cargo test -p veil --test smoke
cargo test -p veil --test admin_api
cargo test -p veil --test proxy_core
cargo test -p veil --test sse_restore
cargo test -p veil-engine          # 引擎单测
cargo test -p veil-store           # 存储单测

# 2.4 格式检查（CI 强制）
cargo fmt --all -- --check

# 2.5 插件脚本测试
node --test plugin/claude-code/scripts/veil.test.mjs
```

CI 等价流水线见 [.github/workflows/ci.yml](.github/workflows/ci.yml)：`cargo fmt --check` → `npm ci && npm test && npm run build` → `VEIL_SKIP_UI_BUILD=1 cargo test --workspace` → 插件测试。

---

## 3. 方案 C：本地打包当前平台（Release）

**目的**：给本机/某台机器临时出一个可用二进制。

```bash
# 1. 前端构建（产物到 ui/dist）
cd ui && npm ci && npm run build && cd ..

# 2. 后端 release 编译（嵌入 ui/dist）
cargo build --release -p veil

# 3. 取产物
#    Windows: target/release/veil.exe
#    Linux/macOS: target/release/veil
```

产物可直接 `veil start` 使用，UI 已嵌入，无需额外前端文件。

### 3.1 产物分发：单文件即可用

`target/release/veil(.exe)` 是**完全自包含的单文件**（约 10–11 MB）：

- **UI 已嵌入**：前端经 `rust-embed` 编译进二进制，不需要附带 `ui/dist` 或任何前端文件。
- **无外部运行时依赖**：Rust release 产物在 Windows 上静态链接了 CRT，**目标机器无需安装 VC++ Redistributable / Node.js / Rust**，只要是 64 位对应系统即可双击运行。
- **配置首次运行自动生成**：数据目录（`admin.token` 等）在首次启动时自动创建——Windows `%LOCALAPPDATA%\veil`，macOS `~/Library/Application Support/veil`，Linux `~/.local/share/veil`（见 [data_dir.rs](crates/veil/src/data_dir.rs)）。无需预置配置文件。

所以**直接把这一个二进制文件发给对方即可**，不用打 ZIP、不用附 README。对方收到后：

```bash
veil                              # 双击 / 后台守护 + 自动开浏览器
veil start --foreground           # 前台运行，日志直出（调试首选）
veil setup --clients all --yes    # 非交互配置上游与客户端
```

首次访问控制台需要 admin token，位于数据目录下的 `admin.token` 文件。

### 3.2 ⚠️ 本产物只能在本平台使用

本地 `cargo build --release` 产出的是**当前平台**的二进制（Windows 上编译得到 `veil.exe`，仅 Windows 64 位可运行；mac/Linux 同理）。**不能跨平台**——Mac/Linux 用户拿到 `veil.exe` 也无法运行。

给其他平台用户分发，走 [方案 E](#5-方案-e正式多平台打包发布推荐生产流程) 的 CI 自动发布，或参考 [5.2 本地交叉编译](#52-备选路径本地交叉编译某平台)。

### 3.3 Windows 文件锁（EPERM unlink）排查

在 Windows 上跑 `npm ci` / `npm run build` 时常见报错：

```
npm error code EPERM
npm error syscall unlink
npm error path ...\node_modules\@esbuild\win32-x64\esbuild.exe
# 或 ...\@rollup\rollup-win32-x64-msvc\rollup.win32-x64-msvc.node
```

**根因**：`npm ci` 要先清掉旧的 `node_modules`，但其中的原生依赖（`esbuild.exe` / `rollup.*.node`）被残留进程占用，无法删除。常见占用者：

- 残留的 Vite dev server（`veil` 项目自身的 `npm run dev`，或 HBuilderX/其他项目里跑着的 vite）
- 上一轮 build 未退出的 esbuild 子进程
- Windows Defender / 杀软实时扫描锁住文件（罕见，通常前两者）

**排查与清理**（Git Bash / PowerShell 均可）：

```bash
# 1. 查残留的 esbuild / node dev 进程
tasklist | grep -iE "esbuild|node"

# 2. 定位占用 ui/node_modules 的 vite/dev 进程
wmic process where "name='node.exe'" get ProcessId,CommandLine | grep -iE "veil|vite|rollup|esbuild"

# 3. 按 PID 杀掉（替换为实际 PID；或按映像名批量杀 esbuild）
taskkill //F //PID <PID1> //PID <PID2>
taskkill //F //IM esbuild.exe

# 4. 确认清空后重试
cd ui && npm ci && npm run build && cd ..
```

> 💡 `taskkill //F //IM esbuild.exe`（Git Bash 里用双斜杠转义）可一次性清掉所有 esbuild 子进程；vite dev server 需按 PID 精确杀，避免误伤 MCP server、编辑器 tsserver 等无关 node 进程。
>
> 若 `taskkill` 后仍 EPERM，多半是杀软锁文件——等几秒重试，或临时给 `ui\node_modules` 目录加杀软排除项。

---

## 4. 方案 D：纯后端验证（跳过前端）

**目的**：不关心 UI，只验证后端逻辑/测试。

```bash
VEIL_SKIP_UI_BUILD=1 cargo build -p veil
VEIL_SKIP_UI_BUILD=1 cargo test --workspace
cargo run -p veil -- start --foreground   # 控制面可用，但访问 UI 返回 "ui not built"
```

> ⚠️ 产物**不可分发给最终用户**。

---

## 5. ✅ 方案 E：正式多平台打包发布（推荐生产流程）

**目的**：产出 Windows / Linux(x64,arm64) / macOS(arm64,x64) 五平台二进制 + 校验和。

### 5.1 推荐路径：CI 自动 release（打 tag 触发）

由 [.github/workflows/release.yml](.github/workflows/release.yml) 在推送 `v*` tag 时完成：前端 `npm ci && npm run build` → 后端 `cargo build --release -p veil --target <target>` → 拷贝到 `dist/<asset>` → 汇总生成 `SHA256SUMS` → 上传 GitHub Release。

```bash
git tag v0.3.15
git push origin v0.3.15
# CI 完成后，Releases 页得到各平台二进制 + SHA256SUMS
```

### 5.2 备选路径：本地交叉编译某平台

对照 release 矩阵：

| 目标 | 产物名 |
| --- | --- |
| `x86_64-pc-windows-msvc` | `veil-windows-x64.exe` |
| `x86_64-unknown-linux-gnu` | `veil-linux-x64` |
| `aarch64-unknown-linux-gnu` | `veil-linux-arm64` |
| `aarch64-apple-darwin` | `veil-macos-arm64` |
| `x86_64-apple-darwin` | `veil-macos-x64` |

示例（Linux ARM64）：

```bash
rustup target add aarch64-unknown-linux-gnu
sudo apt-get install -y gcc-aarch64-linux-gnu
cd ui && npm ci && npm run build && cd ..
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc \
  cargo build --release -p veil --target aarch64-unknown-linux-gnu
# 产物：target/aarch64-unknown-linux-gnu/release/veil
```

### 5.3 CI release 产物各平台怎么用

CI 完成后，GitHub Releases 页会得到 **5 个平台二进制 + 1 个 `SHA256SUMS` 校验文件**。最终用户按自己平台下载**一个文件**即可，无需安装任何运行时（UI 已嵌入，单文件自包含）。

**第 1 步：按平台选下载哪个**

| 用户平台 | 下载文件 | 说明 |
| --- | --- | --- |
| Windows 64 位 | `veil-windows-x64.exe` | 直接是 exe，双击即用 |
| Linux x64（Intel/AMD） | `veil-linux-x64` | 常见 PC / 服务器 |
| Linux ARM64（树莓派 / ARM 服务器 / Apple Silicon Linux VM） | `veil-linux-arm64` | |
| macOS Apple Silicon（M1/M2/M3…） | `veil-macos-arm64` | 近年新款 Mac |
| macOS Intel | `veil-macos-x64` | 老款 Intel Mac |

> 用户可在终端跑 `uname -m`（Linux/macOS）或看「系统信息」确认架构：`x86_64`/`amd64` → x64，`aarch64`/`arm64` → arm64。

**第 2 步：校验完整性（推荐，可选）**

下载 `SHA256SUMS` 文件，与二进制放同目录，核对哈希：

```bash
# Linux / macOS / Git Bash
sha256sum -c SHA256SUMS --ignore-missing veil-linux-x64
# 或只看单个文件哈希对比
sha256sum veil-linux-x64

# Windows PowerShell
Get-FileHash veil-windows-x64.exe -Algorithm SHA256   # 与 SHA256SUMS 里对应行比对
```

**第 3 步：各平台首次运行**

```bash
# Windows — 双击 veil-windows-x64.exe，或在 PowerShell:
.\veil-windows-x64.exe

# Linux — 先加执行权限再跑
chmod +x veil-linux-x64
./veil-linux-x64

# macOS — Apple Silicon / Intel 通用，同样先加权限
chmod +x veil-macos-arm64
./veil-macos-arm64
```

首次启动自动创建数据目录（Windows `%LOCALAPPDATA%\veil`，macOS `~/Library/Application Support/veil`，Linux `~/.local/share/veil`），内含 `admin.token` 与配置。控制台地址：`http://127.0.0.1:18788/?token=<admin.token>`。

**第 4 步（可选）：配置上游与客户端**

```bash
veil setup --clients claude,codex --upstream https://api.anthropic.com
veil setup --clients all --yes    # 非交互（脚本/CI）
```

> ⚠️ macOS 首次运行若被 Gatekeeper 拦截（"无法验证开发者"）：System Settings → Privacy & Security → 点「仍要打开」；或在终端 `xattr -d com.apple.quarantine veil-macos-arm64` 去隔离属性后再跑。
>
> ⚠️ Linux 若报 `cannot bind 127.0.0.1:18788` 等端口占用，参考 [避坑清单](#8-避坑清单) 第 5 条。

---

## 6. 服务进程日常使用（veil CLI）

无论用上述哪个方案产出二进制后，服务操作都走 CLI（见 [crates/veil/src/main.rs:55-69](crates/veil/src/main.rs#L55-L69)）：

```bash
veil                              # 默认 start，后台守护 + 自动开浏览器
veil start --foreground           # 前台运行，日志直出（调试首选）
veil status                       # 控制面/代理状态
veil stop                         # 优雅停止
veil ui                           # 打印/打开控制台 URL
veil setup --clients claude,codex --upstream https://api.anthropic.com
veil setup --clients all --yes    # 非交互（脚本/CI）
veil update --check               # 检查 GitHub 最新 release
```

---

## 7. 速查表

| 场景 | 命令 |
| --- | --- |
| 本地 dev 联调 | 终端1 `cargo run -p veil -- start --foreground`；终端2 `cd ui && npm run dev` |
| 前端测试 | `cd ui && npm test` |
| 前端打包 | `cd ui && npm run build` |
| 后端测试 | `VEIL_SKIP_UI_BUILD=1 cargo test --workspace` |
| 格式检查 | `cargo fmt --all -- --check` |
| 本地 release 包 | `npm run build`(ui) → `cargo build --release -p veil` |
| 本地产物分发 | 单文件 `veil(.exe)` 直接发，单平台自包含（见 [3.1](#31-产物分发单文件即可用)） |
| Windows EPERM unlink | 查 `tasklist \| grep esbuild` → `taskkill //F //IM esbuild.exe`（见 [3.3](#33-windows-文件锁eperm-unlink排查)） |
| 正式多平台发布 | `git push origin v<ver>` 触发 CI |
| CI 产物各平台使用 | 按 [5.3 表](#53-ci-release-产物各平台怎么用) 选对应平台单文件下载即用 |
| 启动服务 | `veil` / `veil start --foreground` |
| 停止 / 状态 | `veil stop` / `veil status` |
| 配置客户端 | `veil setup --clients <list> --upstream <url>` |

---

## 8. 避坑清单

1. **先 build 前端，再 build 后端** — `rust-embed` 编译期读 `ui/dist`，顺序反了会导致后端编译失败或 UI 404。
2. **`VEIL_SKIP_UI_BUILD=1`** 只用于纯后端测试/CI，产物不可分发。
3. **Windows 后台守护** 用 `spawn_daemon` + startup lock；要看日志必须 `--foreground`。
4. **数据目录**（含 `admin.token`）：Windows `%LOCALAPPDATA%\veil`，macOS `~/Library/Application Support/veil`，Linux `~/.local/share/veil`（见 [vite.config.ts:16-27](ui/vite.config.ts#L16-L27)）。
5. **端口冲突**：管理端口 `18788`、代理端口 `18787` 被占时启动报 `cannot bind ...` 并调用 `describe_port_owner` 提示占用者。
6. **本地产物不可跨平台**：`cargo build --release` 出的是当前平台二进制（Windows 上只得 `veil.exe`），给其他平台用户走 [方案 E](#5-方案-e正式多平台打包发布推荐生产流程) CI 或 [5.2 交叉编译](#52-备选路径本地交叉编译某平台)。
7. **Windows `npm ci` 报 EPERM unlink**：残留 vite/esbuild 进程锁住 `node_modules` 原生依赖，按 [3.3](#33-windows-文件锁eperm-unlink排查) 查杀后再重试。
8. **macOS Gatekeeper 拦截 CI 产物**：`xattr -d com.apple.quarantine <二进制>` 去隔离属性后再运行。
