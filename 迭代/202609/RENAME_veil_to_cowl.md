# Veil → Cowl 改名方案

- 状态：**待开发**

将项目展示名由 `Veil` 改为 `Cowl`，延续「遮蔽」语义（兜帽，遮住面孔）。本文档列出所有需要修改的位置与策略，待决策点定夺后落地执行。

## 命名映射

| 类别 | Veil 形态 | Cowl 目标 | 说明 |
|---|---|---|---|
| 包 / crate 名（小写） | `veil` / `veil-engine` / `veil-store` | `cowl` / `cowl-engine` / `cowl-store` | crate 名、内部依赖 |
| 二进制名 | `veil.exe` / `veil` | `cowl.exe` / `cowl` | 产物、命令名 |
| 展示名（首字母大写） | `Veil` | `Cowl` | README、UI 文案、日志、help |
| 环境变量前缀 | `VEIL_` | `COWL_` | `VEIL_DATA_DIR` 等 |
| HTTP 头 | `x-veil-admin-token` | `x-cowl-admin-token` | 建议双头兼容 |
| 数据目录 | `%LOCALAPPDATA%\veil` | `%LOCALAPPDATA%\cowl` | 建议回退读 veil |
| pid 文件 | `veil.pid` | `cowl.pid` | 建议回退读 veil |
| 发布资产名 | `veil-{os}-{arch}[.exe]` | `cowl-{os}-{arch}[.exe]` | CI 产物 |
| 升级源 repo | `guangcheng9/Veil` / `Mouseww/veil` | **待定** | update.rs / install.ps1 |

## 决策点（待定）

1. **向后兼容**：是否保留 `VEIL_*` 环境变量、`veil` 数据目录、`veil.pid` 的回退读取。现有代码已有 `DGW → Veil` 三层回退先例，推荐沿用。
2. **升级源 repo**：`update.rs` 的 `REPO` 与 `install.ps1` 的 `$repo` 改成哪个 GitHub 仓库。
3. **命令别名**：是否同时安装 `veil` 命令别名指向 `cowl` 二进制（迁移期友好）。
4. **crate 目录**：`crates/veil/` 等三个目录是否跟着包名一起改名（影响所有内部 import 路径）。

---

## 一、Rust workspace / crate 层

### 1. Cargo 配置
- `Cargo.toml`:3 `members` 三个 crate 路径
- `crates/veil/Cargo.toml`:2 `name`、:6 `description`、:9-10 依赖路径
- `crates/veil-engine/Cargo.toml`:2 `name`
- `crates/veil-store/Cargo.toml`:2 `name`、:8
- `Cargo.lock` 包名条目 :1774,1797,1798,1802,1818,1829（`cargo build` 自动重写）

### 2. crate 目录改名（⚠️ 决策点 4）
- `crates/veil/` → `crates/cowl/`
- `crates/veil-engine/` → `crates/cowl-engine/`
- `crates/veil-store/` → `crates/cowl-store/`

若目录改名，下列 `use veil::` / `use veil_engine::` / `use veil_store::` import 必须同步：
- `crates/veil/src/main.rs`:8-18
- `crates/veil/src/lifecycle.rs`:8
- `crates/veil-engine/src/lib.rs`:11,19
- `crates/veil-engine/src/creator.rs`:42
- `crates/veil-store/src/sqlite.rs`:7-9,248,249
- `crates/veil-store/src/crypto.rs`:7,77
- `crates/veil/tests/admin_api.rs`
- `crates/veil/tests/proxy_core.rs`:13-15
- `crates/veil/tests/smoke.rs`:13-14
- `crates/veil/tests/sse_restore.rs`:2-5,159

若只改包名不改目录，import 改成 `use cowl::` 即可，路径不动。

---

## 二、CLI / 二进制

### `crates/veil/src/main.rs`
- :1 注释 `CLI: \`veil\``
- :56 `Veil -- keep secrets on your machine`
- :58-68 help 文案全部 `veil.exe` / `veil setup` / `veil start` …
- :104 `Veil is running`
- :114 `Next: veil setup`
- :193,304,313,417,485,487 其余 `Veil`/`veil`

### `crates/veil/build.rs`
- :17 stub HTML `<title>Veil</title>` → `Cowl`
- :24,27-29 `VEIL_SKIP_UI_BUILD` → `COWL_SKIP_UI_BUILD`

### `crates/veil/src/process.rs`
- :30 `runtime_path` → `data_dir.join("cowl.pid")`（⚠️ 决策点 1：读取时回退认 `veil.pid`）
- :41 fallback 字符串 `"veil"`
- :51 tmp `.veil.pid.{}.tmp` → `.cowl.pid.{}.tmp`
- :192,200,224,234,506,520,638,642 文案

### `crates/veil/src/update.rs`（⚠️ 决策点 2）
- :8 `REPO = "guangcheng9/Veil"` → 你的 fork repo
- :16-25 `asset_name()` 全部 `veil-*` → `cowl-*`
- :150,235,274-286,316-343 其余 `VEIL_*` / 文案

### 其余含展示名
- `crates/veil/src/clients.rs`:99,147
- `crates/veil/src/setup.rs`:57,71
- `crates/veil/src/admin/api.rs`:10,83,343,515,531,547,573
- `crates/veil/src/logging.rs`:22,48,53,74-76
- `crates/veil/src/proxy/forward.rs`:11-15,239,260,337,451
- `crates/veil/src/proxy/sse.rs`:2-5
- `crates/veil/src/lifecycle.rs`:8,503

---

## 三、环境变量 / 数据目录（兼容性核心）

### `crates/veil/src/env.rs` — 回退链入口
- :3,5,9,11：当前 `VEIL_{suffix}` → `DGW_{suffix}`；改为 `COWL_{suffix}` → `VEIL_{suffix}` → `DGW_{suffix}` 三层

### `crates/veil/src/data_dir.rs`
- :3 注释
- :9 `base.join("veil")` → `base.join("cowl")`
- :10-15 回退链加 `cowl` 在最前：`cowl` → `veil` → `dgw`
- :44-73 测试补 `COWL_DATA_DIR` 用例，保留 `VEIL_DATA_DIR` 回退用例

### `crates/veil/src/master_key.rs`
- :12 注释（注意原文 `VEIL_MASTER_KEY (then VEIL_MASTER_KEY)` 本就重复有误）
- :16 通过 `crate::env::var("MASTER_KEY")` 自动获得三层回退，代码本体无需改
- 测试 :94-110,124,135 改用 `COWL_MASTER_KEY`，补 `VEIL_MASTER_KEY` 回退测试

### `crates/veil/src/config.rs`
- :349-365,394,410,420,426 直接 `std::env::var("VEIL_...")` 处 → 改走 `crate::env::var(...)` 或加 `COWL_` 前缀

### `crates/veil/src/proxy/alias_hint.rs`
- :3,7 `VEIL` 环境变量

### `crates/veil/src/admin/auth.rs`（⚠️ 决策点 1）
- :29,30,42 HTTP 头 `x-veil-admin-token` → 接受 `x-cowl-admin-token` 与 `x-veil-admin-token`

---

## 四、UI / 前端

### 包配置
- `ui/package.json`:2 `veil-ui` → `cowl-ui`
- `ui/package-lock.json`:2,8

### 入口
- `ui/index.html`:6 `<title>Veil</title>`

### 开发 / 构建配置
- `ui/vite.config.ts`
  - :17 `env.VEIL_DATA_DIR || env.DGW_DATA_DIR` → 加 `COWL_DATA_DIR` 在前
  - :21,24,26 数据目录 `veil` → `cowl`，回退 `veil`
  - :28-30 `veil`/legacy `dgw` 回退链加 `cowl`
  - :53,54 `x-veil-admin-token` 头 → 与后端一致

### 业务代码
- `ui/src/App.tsx`:14,17,26,60
- `ui/src/api.ts`:34,84,88,89,118,137,148（HTTP 头 + 文案）
- `ui/src/i18n.tsx`:17-346（中英双语展示名，量大）
- `ui/src/pages/Dashboard.tsx`:143,154
- `ui/src/pages/Upstream.tsx`:148
- `ui/src/pages/Status.tsx`:30

---

## 五、Claude Code 插件（`plugin/claude-code/`）

### 清单文件
- `plugin/claude-code/plugin.json`:2 `name`、:4 描述、:5 `veil` 字段与 repo
- `plugin/claude-code/hooks/hooks.json`:5 `node scripts/veil.mjs start`

### 脚本（含文件名）
- `plugin/claude-code/scripts/veil.mjs` → `cowl.mjs`，内部 :10,18,35,38,69,80,83,90,96 `VEIL`/`veil`
- `plugin/claude-code/scripts/veil.test.mjs` → `cowl.test.mjs`，内部 :12-46,65,68

### Skill
- 目录 `plugin/claude-code/skills/veil/` → `skills/cowl/`
- `plugin/claude-code/skills/veil/SKILL.md`:2-15

### 文档
- `plugin/claude-code/README.md`:1,3

---

## 六、安装脚本 / CI / 发布

### `scripts/install.ps1`（⚠️ 决策点 2、3）
- :1 注释
- :3 `$repo = "Mouseww/veil"` → 你的 fork
- :4 `$dir = ...\veil` → `cowl`
- :7 `$target = ...\veil.exe` → `cowl.exe`
- :11-12 asset 名 → `cowl-windows-x64.exe`
- :14,31,34,37,44,46 文案与进程名
- :34 `Get-Process veil,dgw` 是否保留 `veil` 以兼容老进程（待定）

### `.github/workflows/release.yml`
- :16-28 matrix asset `veil-*` → `cowl-*`
- :31 `VEIL_SKIP_UI_BUILD` → `COWL_SKIP_UI_BUILD`
- :50 `cargo build -p veil` → `-p cowl`
- :55 `release/veil` → `release/cowl`

### `.github/workflows/ci.yml`
- :27,30 `VEIL` / `veil` 同步

### `SHA256SUMS-win.txt`
- :1 重新发布后重算

---

## 七、文档

### 必改
- `README.md` — 全文
- `README.zh-CN.md` — 全文
- `LAUNCH.md` — 全文（命令示例密集）
- `RELEASE_NOTES.md`:1,10,15
- `CONTEXT.md`:1,11,143
- `docs/使用.md` — 全文

### 历史文档（建议留旧名 + 注明）
- `docs/superpowers/plans/2026-09-10-desensitization-gateway.md`
- `迭代/202609/DESIGN_proxy_and_lifecycle.md`

---

## 落地顺序建议

1. 先定四个决策点。
2. Rust 侧：crate 名 / 目录 / import → env.rs 回退链 → data_dir / master_key / config → process.pid / update.asset → 其余文案。
3. UI 侧：package.json / index.html → vite.config → api.ts HTTP 头 → i18n 与各页面文案。
4. 插件：plugin.json → 脚本文件名与内容 → SKILL 目录与文档。
5. CI / 发布：release.yml asset → ci.yml → install.ps1。
6. 文档：README / LAUNCH / 使用.md / RELEASE_NOTES / CONTEXT。
7. `cargo build` 重写 Cargo.lock，本地 `npm run build` 验证 UI，`cargo test` 跑回退用例。
