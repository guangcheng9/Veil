#!/usr/bin/env bash
# Veil 发版脚本（非交互，供 release skill 调用）
#
# 流程：前置检查 → 改版本号 → 提交 → 打 tag → 推送 master → 推送 tag（触发 CI）
# 全程打印日志（成功 ✓ / 失败 ✗），任何步骤出错即终止并报错。
#
# 用法：scripts/release.sh <version>
# 示例：scripts/release.sh 0.3.15
#
# 前置：
#   - 必须在 master 分支
#   - 除 RELEASE_NOTES.md 外，工作区须干净（skill 会先把生成的正文写入 RELEASE_NOTES.md）
#
# 该脚本不生成 RELEASE_NOTES.md 的正文内容（Added/Commits 等）——那由 release skill
# 基于提交记录生成；脚本只负责把版本号（标题 + gh download 行）同步到目标版本。

set -euo pipefail

log()  { printf '\n==> %s\n' "$*"; }
ok()   { printf '  ✓ %s\n' "$*"; }
err()  { printf '  ✗ %s\n' "$*" >&2; }

VERSION="${1:-}"
if [ -z "$VERSION" ]; then
  err "用法: $0 <version>  （例如 $0 0.3.15）"
  exit 2
fi
VERSION="${VERSION#v}"   # 兼容 v0.3.15 写法
TAG="v${VERSION}"

cd "$(git rev-parse --show-toplevel)"

log "目标版本: ${VERSION}  (tag ${TAG})"

# --- 1. 前置检查 ---
BRANCH="$(git rev-parse --abbrev-ref HEAD)"
if [ "$BRANCH" != "master" ]; then
  err "当前分支 ${BRANCH}，发版须在 master 上进行"
  exit 1
fi
ok "分支: master"

# 除 RELEASE_NOTES.md 外不允许有未提交改动
if git status --porcelain | grep -vE 'RELEASE_NOTES\.md' | grep -q .; then
  err "工作区有除 RELEASE_NOTES.md 外的未提交改动，请先 commit 或 stash："
  git status --short | grep -vE 'RELEASE_NOTES\.md' >&2 || true
  exit 1
fi
ok "工作区检查通过（RELEASE_NOTES.md 可未提交）"

# tag 不能已存在
if git rev-parse "$TAG" >/dev/null 2>&1; then
  err "tag ${TAG} 已存在"
  exit 1
fi
ok "tag ${TAG} 不存在，可创建"

LAST_TAG="$(git tag --sort=-v:refname | head -1)"
log "自 ${LAST_TAG:-仓库起始} 以来的提交："
if [ -n "$LAST_TAG" ]; then
  git log "${LAST_TAG}..HEAD" --oneline | sed 's/^/    /' || true
else
  git log --oneline | sed 's/^/    /' || true
fi

# --- 2. 改版本号 ---
log "更新版本号 → ${VERSION}"
sed -i -E 's|^version = "[0-9]+\.[0-9]+\.[0-9]+"|version = "'"${VERSION}"'"|' Cargo.toml
sed -i -E 's|"version": "[0-9]+\.[0-9]+\.[0-9]+"|"version": "'"${VERSION}"'"|' ui/package.json
sed -i -E 's|"version": "[0-9]+\.[0-9]+\.[0-9]+"|"version": "'"${VERSION}"'"|g' plugin/claude-code/plugin.json
sed -i -E 's|Veil [0-9]+\.[0-9]+\.[0-9]+|Veil '"${VERSION}"'|g' RELEASE_NOTES.md
sed -i -E 's|gh release download v[0-9]+\.[0-9]+\.[0-9]+|gh release download v'"${VERSION}"'|g' RELEASE_NOTES.md
ok "已更新 Cargo.toml / ui/package.json / plugin.json / RELEASE_NOTES.md"

# --- 3. 提交 ---
log "提交版本变更"
git add Cargo.toml ui/package.json plugin/claude-code/plugin.json RELEASE_NOTES.md
git commit -m "chore: bump version to ${TAG}" \
  -m "Co-Authored-By: Claude Code <noreply@anthropic.com>" >/dev/null
ok "已提交版本变更"

# --- 4. 打 tag ---
log "打 tag ${TAG}"
git tag "$TAG"
ok "已打 tag"

# --- 5. 推送 master ---
log "推送 master 分支到 origin"
if git push origin master; then
  ok "master 已推送"
else
  err "推送 master 失败（tag 已在本地创建，未推送）"
  err "修复后手动推送: git push origin master && git push origin ${TAG}"
  exit 1
fi

# --- 6. 推送 tag（触发 CI） ---
log "推送 tag ${TAG}（将触发 CI 多平台构建 + GitHub Release）"
if git push origin "$TAG"; then
  ok "tag 已推送，CI 已触发"
else
  err "推送 tag ${TAG} 失败（master 已推送，tag 未推送）"
  err "修复后手动推送: git push origin ${TAG}"
  exit 1
fi

# --- 完成 ---
log "发版完成 ✅"
echo "  Actions:  https://github.com/guangcheng9/Veil/actions"
echo "  Releases: https://github.com/guangcheng9/Veil/releases"
echo "  CI 完成后 Releases 页将出现 5 个平台二进制 + SHA256SUMS"
