---
name: release
description: Veil 项目的正式发版流程。当用户说"发版"、"release"、"打 v0.x.x"、"发布新版本"、"发 v0.3.15"等时使用。发版前强制同步迭代记录，随后自动从提交记录生成版本更新内容与提交清单写入 RELEASE_NOTES.md，调用 scripts/release.sh 完成改版本号 → 提交 → 打 tag → 推送 master+tag（触发 CI 多平台构建与 GitHub Release），发版后回填发布提交，全程输出成功/失败日志。
---

# Veil 发版 Skill

调用 [scripts/release.sh](../../../scripts/release.sh) 完成 Veil 正式发版。该脚本会推送 `v*` tag 触发 [.github/workflows/release.yml](../../../.github/workflows/release.yml) 的 CI 多平台构建 + GitHub Release 上传，**不可撤销**。

## 前置：确定目标版本

1. 若用户给了版本号（如 `v0.3.15`），直接用，去掉前缀 `v`。
2. 若没给，查当前版本号：读 [Cargo.toml](../../../Cargo.toml) 第 6 行 `version = "x.y.z"`，把 patch +1（或按用户意图 minor/major +1），与用户确认后使用。
3. 查最近 tag：`git tag --sort=-v:refname | head -1`，确认新版本号大于最近 tag。

## 强制前置：同步本地迭代记录

每次发版都必须更新 [Veil本地迭代记录.md](../../../迭代/Veil本地迭代记录.md)，包括 major、minor 和 patch 版本。即使本次只有维护性改动，也必须记录版本与变更，不能跳过。

发版前至少完成：

1. 更新文档头部：当前分支、当前版本 `vX.Y.Z`、主要功能提交、发布日期。
2. 在“当前版本概览”增加目标版本行。
3. 在“详细介绍”增加 `vX.Y.Z 迭代` 章节，记录：
   - 变更内容；
   - 解决的问题；
   - 影响模块；
   - 兼容性与生效方式；
   - 验证结果；
   - 与上游关系。
4. 更新迭代资料索引、计划状态、待办与评估队列。
5. 发布提交暂写为 `发布提交：待发版后回填`，不要猜测提交号。
6. 检查新增 Markdown 相对链接均存在。
7. 提交迭代记录后再调用发版脚本。`scripts/release.sh` 只允许 `RELEASE_NOTES.md` 保持未提交。

脚本会强制检查：

- 迭代记录自上一个 tag 以来确实发生过提交；
- 文档头部包含目标版本；
- 版本概览包含目标版本行；
- 详细介绍包含目标版本迭代章节；
- 发布提交仍为“待发版后回填”占位。

任一检查失败，不得发版。

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
- 前置检查（master 分支、迭代记录已更新、除 RELEASE_NOTES.md 外工作区干净、tag 不存在）
- 自动改 [Cargo.toml](../../../Cargo.toml)、[ui/package.json](../../../ui/package.json)、[plugin.json](../../../plugin/claude-code/plugin.json)、RELEASE_NOTES.md 的版本号
- 提交 `chore: bump version to vX.Y.Z`
- 打 tag
- 推送 master → 推送 tag（触发 CI）
- 每步打印 `==> 步骤` / `✓ 成功` / `✗ 失败` 日志
- 输出实际发布提交号，供迭代记录回填

### 3. 回填发布提交

脚本成功后，使用脚本输出的发布提交号，将迭代记录中的：

```text
发布提交：待发版后回填
```

替换为实际提交号，然后提交并推送文档：

```bash
git add 迭代/Veil本地迭代记录.md
git commit -m "docs: record v<新版本号> release commit"
git push origin master
```

如果发版脚本失败：

- 未产生发布提交时，不要回填；
- 已产生提交或 tag 时，严格按脚本最后输出的 `✗` 行和“修复后手动推送”提示处理；
- 不要重复执行不可撤销的推送步骤。

### 4. 报告结果

**成功**：报告 CI 已触发，给出 Actions 与 Releases 页链接：
- https://github.com/guangcheng9/Veil/actions
- https://github.com/guangcheng9/Veil/releases

**失败**：照脚本最后输出的 `✗` 行与“修复后手动推送”提示如实报告，不要重试不可撤销的推送步骤。

## 约束

- **迭代记录是发版前置条件**：未同步、未提交或脚本检查失败时，不得调用发版脚本。
- **全流程推送，不提供 `--no-push`**：调用此 skill 即同意完成代码 + 文档 + tag 全部推送并触发 CI。
- **不可撤销**：推送 tag 触发 CI 公开发布。若用户只是想试跑/预演，**不要调用此 skill**；预演应在独立分支手动修改版本号，不创建或推送 tag。
- **正文由 skill 生成，版本号由脚本同步**：两者职责分离，避免重复。
- **发布提交由脚本生成后再回填**：不要在发版前猜测 release commit hash。
