# 设计方案：代理可控化 + 控制面生命周期 + 进程对账加固

> 状态：草案
> 起因：Codex 路由测试报 HTTP 502 `upstream error`。根因是 Veil 的 reqwest 客户端隐式继承启动时刻的系统代理/环境变量代理，且用户不可见、不可控；同时网页控制台缺少可重复启停转发的入口，`veil stop` 在 exe 变动/stale pid 场景下清理不彻底，会留下占端口的孤儿进程影响下次启动。

## 1. 目标

1. **消除环境依赖型 502**：让 Veil 到上游的代理行为由配置决定，不再被动继承系统代理/环境变量。
2. **网页可控启停**：管理控制台（18788）常驻；网页可停止和再次启动代理转发，不需要重新执行命令。网页另提供明确的“退出 Veil”，用于完全结束进程。
3. **单实例与进程对账**：用操作系统级单实例锁、实例身份、pid 和端口共同对账；重复双击、前台崩溃、exe 重新编译等场景都不应产生第二个实例或误杀其他程序。
4. **状态可见、故障可诊断**：用户可在网页和 CLI 看到实际运行状态、PID、监听端口、启动时间和端口冲突原因。

## 2. 非目标

- 不改变 Veil 的脱敏/映射核心逻辑。
- 不改变现有路由表、TLS、allowlist 等配置语义。
- 不引入外部进程管理器（supervisor / systemd / NSSM）。
- 不承诺通过普通网页在 Veil 已完全退出后重新拉起 exe；此时必须由系统级启动入口负责。

## 3. 现状分析

### 3.1 代理隐式继承

`AppState::new` 构造 reqwest 客户端时未指定代理策略（`crates/veil/src/proxy/forward.rs:44-49`）：

```rust
client: reqwest::Client::builder()
    .redirect(reqwest::redirect::Policy::none())
    .tcp_nodelay(true)
    .pool_max_idle_per_host(16)
    .build()
    .expect("reqwest client"),
```

reqwest 默认会依次探测：环境变量（`HTTP_PROXY`/`HTTPS_PROXY`/`ALL_PROXY`，大小写均认）→ Windows 系统代理（注册表 `Internet Settings\ProxyServer`）。Veil 进程启动那一刻的环境决定后续所有转发是否走代理，用户既看不见也改不了。本机系统代理 `127.0.0.1:7890` 在旧进程启动时处于失效状态，于是所有上游连接失败，统一返回 `upstream error`（`forward.rs:163-169`）。

### 3.2 网页无启停入口与生命周期状态

`AdminState`（`crates/veil/src/admin/api.rs:18-26`）与路由表（`crates/veil/src/admin/mod.rs:18-40`）未暴露生命周期接口。前端状态页没有停止按钮，顶部“运行中”只在页面首次加载时检查一次，因此服务停止、崩溃或端口不一致后，页面状态会过期。

当前后台模式的真实行为也需要向用户说清楚：

| 用户动作 | 当前行为 |
|---|---|
| 双击 `veil.exe` 或运行 `veil` | 父进程启动一个脱离终端的后台子进程，并打开管理页。 |
| 关闭启动它的终端 | 后台子进程继续运行，属预期行为。 |
| 关闭浏览器页面/标签页 | Veil 继续运行；网页只是管理客户端，不拥有服务进程。 |
| `veil start --foreground` 后关闭终端 | 服务进程会被终止，操作系统会释放端口，但 pid 文件可能残留。 |

因此，关闭网页本身不会额外制造进程残留；真正的问题是后台进程仍在时缺少可见状态和可控停止入口。双击启动时不应要求用户保留终端输出记住地址：成功启动后自动打开浏览器；`veil ui` 与 `veil status` 始终输出当前管理地址和实际状态。

### 3.3 pid/stop 逻辑、重复启动和端口变更的漏洞

`process.rs`：

- `try_start`（72-99）：只读 pid 文件判断"是否已在运行"，不看端口实际占用。
- pid 文件不是互斥锁。两个近乎同时的双击可能都读到“未运行”，均创建后台子进程；最终只有一个能绑定端口，失败进程仍可能覆盖 pid 文件。顺序地多次启动通常会复用已有服务，但并发启动不能可靠保证。
- `try_start` 在绑定任何监听端口前就写 pid；后续 `TcpListener::bind` 失败时会遗留“运行中”的 pid 文件。
- `stop`（128-141）：`existing.exe == current_exe()` 用 exe 路径字符串比对。重新编译 exe（路径相同、二进制不同）或前台崩溃留下孤儿时：
  - 若 pid 已死 → 只删 pid 文件（正确）。
  - 若 pid 存活但 exe 路径字符串不等 → **只删 pid 文件，不杀进程**，留下占 18787/18788/18791 的孤儿。
- 下次 `TcpListener::bind` 报端口占用，错误信息不告诉用户谁占着，只能手动 `netstat` + `taskkill`。
- 配置端口在运行时没有真实热重绑：改了 `proxy_port`、`management_port` 或 route port 后，内存中的监听器仍是旧端口，容易出现“配置显示新端口、实际仍监听旧端口”。

## 4. 详细设计

### 4.1 代理配置化

#### 配置字段

`Config`（`crates/veil/src/config.rs:71-100`）新增字段：

```rust
/// 上游代理。空 = 永远直连上游（默认，切断对系统代理/环境变量的隐式依赖）；
/// 形如 "http://127.0.0.1:7890" = 显式走指定代理。
#[serde(default)]
pub upstream_proxy: String,
```

`Config::default` 初始化为空串。`apply_env_overrides` 增加可选覆盖：`VEIL_UPSTREAM_PROXY`（便于 server 模式容器化部署）。

#### 客户端构造

`AppState::new`（`forward.rs:33-54`）按 `upstream_proxy` 构造 reqwest：

```rust
let mut builder = reqwest::Client::builder()
    .redirect(reqwest::redirect::Policy::none())
    .tcp_nodelay(true)
    .pool_max_idle_per_host(16);
let proxy = state.upstream.upstream_proxy.trim();   // 透传到 UpstreamConfig
if proxy.is_empty() {
    builder = builder.no_proxy();
} else {
    match reqwest::Proxy::all(proxy) {
        Ok(p) => builder = builder.proxy(p),
        Err(e) => { /* 返回配置错误；不启动时静默改为直连 */ }
    }
}
builder.build().expect("reqwest client")
```

`UpstreamConfig`（`classify.rs:25-76`）加 `pub upstream_proxy: String` 字段，`From<&Config>` 与 `for_route` 透传。

#### 管理面

- `GET /api/status`（`admin/api.rs` `StatusBody`）回传 `upstream_proxy`。
- `PUT /api/settings` 或新增 `PUT /api/upstream` 接受该字段；空串合法。
- 前端"上游"卡片加一个"代理（可选）"输入框，占位符说明"留空=直连上游"。

#### 行为矩阵

| `upstream_proxy` | 行为 |
|---|---|
| 空（默认） | `.no_proxy()`，永远直连上游。治 502。 |
| `http://127.0.0.1:7890` | 显式走该代理，确定性生效。 |
| 非法 URL | 保存设置时返回 400；启动时明确失败并保留旧的有效运行配置。绝不静默回退直连，以免用户以为流量会经代理却直接出网。 |

### 4.2 控制面常驻，网页启停代理转发

#### 模型与边界

Veil 分为同一进程内的两个层次：

| 层次 | 监听端口 | 职责 | 生命周期 |
|---|---|---|---|
| 控制面 | `management_port`（默认 18788） | 管理网页、状态、配置与生命周期命令 | Veil 进程存活期间常驻 |
| 数据面 | `proxy_port` 与所有已启用 route port | 脱敏、转发和流量记录 | 可由控制面反复启动、停止、重启 |

这使“停止代理转发”后网页仍可用，也能再次点击“启动代理转发”。“退出 Veil”才会关闭控制面和整个进程；普通网页不能在进程已经退出后自行重新执行 exe。

#### 生命周期控制器与接口

新增进程内 `LifecycleController`（建议新建 `crates/veil/src/lifecycle.rs`）。它独占数据面监听器和 route server 的 JoinHandle，使用 `tokio::sync::mpsc` 接收命令、`watch` 发布状态；不要使用 `Arc<oneshot::Sender<()>>`，因为 oneshot sender 调用 `send` 后即被消费，不能支持重复的启动/停止。

```text
GET  /api/lifecycle          返回控制面、数据面和监听端口的实际状态
POST /api/lifecycle/start    启动数据面；已运行时幂等返回当前状态
POST /api/lifecycle/stop     优雅停止数据面；控制面继续运行
POST /api/lifecycle/restart  停止后重新读取配置并启动数据面
POST /api/lifecycle/exit     优雅停止数据面和控制面，然后退出整个 Veil 进程
```

所有 mutating 接口均使用 `require_token(..., true)`。`exit` handler 必须先返回 `202 Accepted`，再由控制器开始退出，避免客户端先看到网络断开而收不到确认。

`GET /api/status` 或独立的 `GET /api/lifecycle` 需返回至少：

```json
{
  "instance_id": "01...",
  "pid": 12345,
  "started_at": "2026-09-18T09:30:00Z",
  "control_plane": "running",
  "proxy_state": "running",
  "listeners": [18787, 18791, 18792],
  "last_lifecycle_error": null
}
```

`proxy_state` 为 `starting`、`running`、`stopping`、`stopped`、`degraded` 之一。只有全部配置的数据面端口都成功监听才进入 `running`；任一端口失败时释放本次已绑定的全部数据面监听器，进入 `degraded` 并保留具体错误。

`veil status` 使用同一套对账结果，输出管理地址、PID、实例 ID、启动时间、控制面状态、数据面状态、实际监听端口和最后生命周期错误；不得只根据 pid 是否存活报“运行中”。

#### 关闭顺序

对数据面停止或退出均使用 `axum::serve(...).with_graceful_shutdown(...)`：停止接收新请求，最多等待 5 秒处理在途请求；超时后记录原因并释放监听器。`exit` 还必须等待管理服务与全部 route task 结束，随后仅删除属于当前 `instance_id` 的 runtime 文件并释放单实例锁。

CLI `veil stop` 先调用本机带 token 的 `POST /api/lifecycle/exit` 并等待进程和端口释放；仅在优雅停止超时、且已严格确认目标为本实例时，才升级为强制终止。

#### 前端按钮与页面说明

状态页增加“服务控制”区域，明确区分以下操作：

| 页面控件 | 可用条件 | 动作 | 成功后的页面说明 |
|---|---|---|---|
| `停止转发` | 数据面为 `running` | 调 `POST /api/lifecycle/stop`，停止 18787 和全部 route port | 显示“转发已停止；管理控制台仍可使用”。配置和流量历史仍可查看。 |
| `启动转发` | 数据面为 `stopped` 或 `degraded` | 调 `POST /api/lifecycle/start` | 显示实际监听端口；配置正确时客户端可立即再次通过 Veil 转发。 |
| `重启转发` | 数据面为 `running` | 调 `POST /api/lifecycle/restart` | 显示“正在应用配置并重启转发”；完成后展示新端口。 |
| `退出 Veil` | 控制面可访问 | 二次确认后调 `POST /api/lifecycle/exit` | 确认文案必须说明“这会关闭代理和管理控制台；之后需双击 `veil.exe` 或运行 `veil` 重新启动”。 |

按钮在 `starting`、`stopping` 时禁用，避免重复请求。页面每 2 秒轮询生命周期状态并在窗口重新获得焦点时立即刷新；顶部状态不能只在首次加载时判断。

调用“退出 Veil”后，前端等待接口确认，然后轮询直至控制面断开并显示只读离线页：“Veil 已退出。此页面不能启动已退出的进程；请双击 `veil.exe` 或运行 `veil`。”此时禁用所有配置和生命周期操作。关闭浏览器标签页不会停止 Veil，也不会新增残留进程。

### 4.3 单实例、端口对账与安全清理

#### 单实例锁优先于 pid 文件

pid 文件仅用于状态展示和故障诊断，不能承担互斥。启动时必须先获取并持续持有操作系统锁：

- Windows：基于 canonical data dir 哈希创建当前用户会话范围的 named mutex，例如 `Local\Veil-<data-dir-hash>`。
- Unix：在 data dir 创建并持续持有独占 advisory lock。

锁由操作系统在进程异常退出时自动释放。启动器拿不到锁时不得再创建后台子进程；它应短暂等待控制面完成启动，然后探测管理 API。确认返回对应实例后只打开现有管理页；超时则报告“已有 Veil 实例正在启动或无响应”，而不是盲目启动第二个实例。

`PidFile` 扩展为 runtime metadata：`pid`、`process_started_at`、`instance_id`、canonical `exe_path`、`management_port`、实际 `listener_ports` 与 `proxy_state`。控制面成功绑定后以 `starting` 原子写入记录；数据面完整启动后原子更新为 `running`，任一端口失败则更新为 `degraded` 并仅保留控制面端口。正常退出时只删除 `instance_id` 匹配的记录。

#### 端口归属查询与身份确认

`process.rs` 新增 `port_owner_pid(port: u16) -> Option<u32>`；Windows 用 `GetExtendedTcpTable`（`iphlpapi`）查询 owner PID，Unix 后续可用 `/proc/net/tcp` 扩展。诊断中还应读取进程创建时间和 canonical exe 路径。

不能仅因“端口被 `veil.exe` 占用”就自动杀进程：它可能来自另一个 data dir、另一个安装目录或另一个用户。允许自动回收的最低条件是同时匹配：

1. runtime metadata 中的 PID；
2. PID 的进程创建时间；
3. 本实例的 canonical exe path；
4. 被占用端口属于 metadata 中的完整 listener 集合；
5. 管理 API 无响应或未返回同一 `instance_id`。

任一条件不满足时不杀进程，而是返回明确错误，例如“端口 18791 被 PID 4567（`other.exe`）占用”。这比“保证杀干净”更安全：Veil 能自动清理已确认属于自己的孤儿，绝不凭进程名误杀其他程序。

#### 启动、停止与修复流程

```text
启动
1. 获取单实例锁；失败则探测现有管理 API，确认已运行后打开既有管理页。
2. 读取 runtime metadata，校验 PID、创建时间、instance_id 与管理 API。死 pid 或不匹配记录只作为 stale metadata 删除。
3. 先绑定 management port，原子写入 `starting` metadata；再由 LifecycleController 预绑定 proxy port 和所有 route port。
4. 任意数据面端口失败：关闭本次全部已绑定的**数据面**监听器，metadata 更新为 `degraded`，保留管理页并报告端口、PID、进程名和可执行路径。
5. 全部绑定成功后原子更新 metadata 的完整 listener 集合，发布 `running`。启动器等待状态稳定为 `running` 或 `degraded` 后自动打开浏览器；即使失败，用户也能直接看到诊断和修复入口。

停止转发
1. 管理 API 请求控制器优雅停止数据面。
2. 等待全部数据面监听器和任务结束，状态发布为 `stopped`；管理端口继续监听。

退出
1. 管理 API 或 CLI 请求优雅停止数据面和控制面。
2. 等待任务结束、删除当前实例 metadata、释放锁并退出。
3. 超时后仅在身份确认通过时强制终止；否则报告诊断信息并保留现场。
```

#### 多网页与端口配置的语义

多个浏览器标签页访问同一个 `management_port` 是安全的，它们不会启动多个 Veil 服务；配置保存需以版本号（或 `updated_at`）做乐观并发控制，旧页面提交时返回 409 并要求刷新，避免“最后保存者静默覆盖”。

端口修改按以下规则处理：

| 修改项 | 影响与处理 |
|---|---|
| `proxy_port` | 已手动配置到旧代理地址的客户端需要改为新端口；保存后标记“需重启转发”，由“重启转发”统一重绑。 |
| route port | 对应 Claude/Codex 等客户端仍会指向旧端口；必须同步重写该客户端配置，或提示用户重新执行 setup。 |
| `management_port` | 会改变网页地址；第一版不在网页中提供修改入口。通过配置文件或 CLI 修改后需完整退出并重新启动，启动器自动打开新地址。 |

保存前校验端口范围、去重、与全部 route port 的冲突；重启前预绑定，保证状态页展示的端口与实际监听完全一致。

## 5. 影响面

| 文件 | 改动 |
|---|---|
| `crates/veil/src/config.rs` | 加 `upstream_proxy` 字段 + env 覆盖；配置 revision/校验；端口去重和 staged port update 语义 |
| `crates/veil/src/proxy/classify.rs` | `UpstreamConfig` 加 `upstream_proxy`，`From<&Config>` / `for_route` 透传 |
| `crates/veil/src/proxy/forward.rs` | `AppState::new` 按 `upstream_proxy` 构造 reqwest；非法代理配置以明确错误返回 |
| `crates/veil/src/lifecycle.rs`（新增） | 持有数据面监听器/任务，处理 start、stop、restart、exit 并发布 `ProxyState` |
| `crates/veil/src/admin/mod.rs` | 加 lifecycle 路由 |
| `crates/veil/src/admin/api.rs` | `AdminState` 加控制器句柄和状态订阅；`StatusBody` 加实例、PID、端口、生命周期状态和 `upstream_proxy` |
| `crates/veil/src/main.rs` | 取得单实例锁，启动常驻控制面，委托 LifecycleController 启停数据面，所有退出路径清理当前实例 metadata |
| `crates/veil/src/process.rs` | 单实例锁、runtime metadata 原子读写、进程创建时间/路径校验、`port_owner_pid` 与安全诊断 |
| `ui/src/App.tsx`、`ui/src/pages/Status.tsx`、`ui/src/api.ts` | 每 2 秒刷新真实状态；服务控制区的“停止转发 / 启动转发 / 重启转发 / 退出 Veil”；离线只读页；上游代理输入框 |

## 6. 测试计划

- `config.rs`：`upstream_proxy` roundtrip、env 覆盖、空串默认、非法代理 URL 拒绝保存；端口范围/重复/route 冲突校验；revision 冲突返回 409。
- `process.rs`：
  - 两个并发启动者只有一个取得单实例锁；另一个识别已有实例而不创建后台子进程。
  - dead pid、pid 被复用、已删除 exe、前台崩溃均只清理 stale metadata，不误杀新 PID。
  - management port 已成功绑定但任意数据面 bind 失败时，metadata 原子更新为 `degraded` 且只列出实际管理端口；成功后原子发布完整 listener 集合。
  - 端口被已确认属于同一 instance 的孤儿占用时可安全回收。
  - 端口被其他程序、其他 data dir 的 Veil 或身份无法确认的 Veil 占用时，只返回 PID/进程路径诊断，绝不自动终止。
- `lifecycle.rs`：
  - start/stop/restart 的幂等性和状态迁移；`stopping` 前重复请求不会生成第二组监听器。
  - stop 后 management port 保持可访问，start 可重新绑定全部 proxy/route port。
  - 任一路由端口 bind 失败时，其余本次绑定端口均释放并进入 `degraded`。
  - exit 在响应 `202` 后退出全部服务、删除当前 instance metadata、释放锁。
- `forward.rs`：reqwest 在空/合法 `upstream_proxy` 下的行为（mock upstream）；错误代理配置不静默直连。
- Admin API：所有 lifecycle mutating endpoint 未携带 token 返回 401/403；状态 API 返回实际 PID、instance ID、启动时间、完整监听器和最后生命周期错误。
- 前端：多标签页状态刷新；停止转发后出现“管理控制台仍可使用”说明；退出确认包含“需双击 exe/运行 CLI 才能重启”；控制面断开后禁用操作并展示离线说明。
- 集成：后台 Veil 完成“启动转发 → 停止转发 → 再启动转发 → 退出 Veil”，每步断言端口、管理 API、runtime metadata 与状态一致；随后重新启动且无残留。

## 7. 风险

- **Windows 单实例锁和端口归属查询**：分别需要 `CreateMutexW` 与 `GetExtendedTcpTable`（`iphlpapi`）等 FFI；可用系统命令诊断作临时兜底，但不作为互斥正确性的依据。
- **误杀风险**：绝不以 `veil.exe` 文件名或单个端口作为自动 kill 的依据。无法确认 instance 身份时宁可返回诊断，让用户显式修复。
- **生命周期权限**：start、stop、restart、exit 都必须校验 admin token，避免任何能访问 18788 的人控制本机服务。
- **优雅停止超时**：在途长 SSE/streaming 可能被 5 秒上限中断。默认 5 秒合理，但应记录超时并允许后续配置化。
- **完全退出后的启动限制**：网页由控制面提供；控制面退出后页面只能显示离线说明，不能自行拉起 exe。未来若需要一键从“完全退出”状态启动，需另做系统托盘、Windows 服务或自定义协议启动器。
- **多标签配置并发**：状态读取可并发，配置写入必须做版本校验，避免静默覆盖。

## 8. 里程碑

1. 代理配置化与校验（config + forward + classify + UI 输入框）——**直接治 502，且不因错误配置意外直连**。
2. 单实例锁与 runtime metadata（process.rs + main）——**防并发双击、防 stale pid、提供可核验的运行身份**。
3. 控制面/数据面生命周期（lifecycle + admin + main）——**网页可停止、重新启动和重启代理转发**。
4. 状态与页面控制（UI）——**显示实际监听、PID、错误和退出后的离线说明**。

第 2、3 块需一起完成才能安全上线；第 4 块依赖第 3 块的状态 API。
