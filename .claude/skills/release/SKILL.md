---
name: release
description: Veil 项目的正式发版流程。当用户说"发版"、"release"、"打 v0.x.x"、"发布新版本"、"发 v0.3.15"等时使用。自动从提交记录生成版本更新内容与提交清单写入 RELEASE_NOTES.md，调用 scripts/release.sh 完成改版本号 → 提交 → 打 tag → 推送 master+tag（触发 CI 多平台构建与 GitHub Release），全程输出成功/失败日志。
---

# Veil 发版 Skill

调用 [scripts/release.sh](../../../scripts/release.sh) 完成 Veil 正式发版。该脚本会推送 `v*` tag 触发 [.github/workflows/release.yml](../../../.github/workflows/release.yml) 的 CI 多平台构建 + GitHub Release 上传，**不可撤销**。

## 前置：确定目标版本

1. 若用户给了版本号（如 `v0.3.15`），直接用，去掉前缀 `v`。
2. 若没给，查当前版本号：读 [Cargo.toml](../../../Cargo.toml) 第 6 行 `version = "x.y.z"`，把 patch +1（或按用户意图 minor/major +1），与用户确认后使用。
3. 查最近 tag：`git tag --sort=-v:refname | head -1`，确认新版本号大于最近 tag。

## 步骤

### 1. 生成 RELEASE_NOTES.md 正文

从上个 tag 到 HEAD 的提交记录是版本更新内容的来源：

```bash
git log <last_tag>..HEAD --oneline --no-decorate
```

按 [Conventional Commits](https://www.conventionalcommits.org/) 分类写 **Added** 段：
- `feat:` → Added（用户可见的新功能）
- `fix:` → 可加 Fixed 段
- `perf:`/`refactor:` 用户可感知时 → Added
- `docs:`/`style:`/`chore:`/`test:`/`ci:` → **不写进用户可见 Release Notes**（属内部改动）

写 [RELEASE_NOTES.md](../../../RELEASE_NOTES.md)，格式：

```markdown
Veil <新版本号>

## Added

- <功能1>（一句话，用户视角，不提 commit hash）
- <功能2>

## Update

Console: Check for updates, or `veil update`.

If GitHub downloads fail:

```
gh release download v<新版本号> --repo guangcheng9/Veil -p veil-windows-x64.exe
```
```

**同时附「提交记录」段**：在文件末尾追加本次发布包含的全部 commit 清单（`git log <last_tag>..HEAD --oneline` 原样列出），供发版日志与回溯。格式：

```markdown

## Commits

- <hash> <subject>
- ...
```

用 Write 工具直接写入 RELEASE_NOTES.md（覆盖整文件）。**不要手动改版本号**——脚本会自动同步标题行和 `gh release download` 行的版本号。

### 2. 调用脚本

```bash
scripts/release.sh <新版本号>
```

脚本会：
- 前置检查（master 分支、除 RELEASE_NOTES.md 外工作区干净、tag 不存在）
- 自动改 [Cargo.toml](../../../Cargo.toml)、[ui/package.json](../../../ui/package.json)、[plugin.json](../../../plugin/claude-code/plugin.json)、RELEASE_NOTES.md 的版本号
- 提交 `chore: bump version to vX.Y.Z`
- 打 tag
- 推送 master → 推送 tag（触发 CI）
- 每步打印 `==> 步骤` / `✓ 成功` / `✗ 失败` 日志

### 3. 报告结果

**成功**：报告 CI 已触发，给出 Actions 与 Releases 页链接：
- https://github.com/guangcheng9/Veil/actions
- https://github.com/guangcheng9/Veil/releases

**失败**：照脚本最后输出的 `✗` 行与 `修复后手动推送` 提示如实报告，不要重试不可撤销的推送步骤。

## 约束

- **全流程推送，不提供 `--no-push`**：调用此 skill 即同意完成代码 + 文档 + tag 全部推送并触发 CI。
- **不可撤销**：推送 tag 触发 CI 公开发布。若用户只是想试跑/预演，**不要调用此 skill**——直接让用户手动 `scripts/release.sh <ver> --no-push`（但当前脚本已移除该选项；预演请手动改版本号不打 tag）。
- **正文由 skill 生成，版本号由脚本同步**：两者职责分离，避免重复。
