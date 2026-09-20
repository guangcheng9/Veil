#!/usr/bin/env bash
# Veil 发版脚本：改版本号 → 提交 → 打 tag → 推送（触发 CI 自动发布）
#
# 用法：
#   scripts/release.sh 0.3.15           # 改版本、提交、打 tag、推送
#   scripts/release.sh 0.3.15 --no-push # 只到打 tag，不推送（默认推送）
#
# 推送 v* tag 会触发 .github/workflows/release.yml 的多平台构建 + GitHub Release 上传，
# 属于不可撤销的公开发布动作，脚本会在推送前再确认一次。
#
# RELEASE_NOTES.md 的版本号会被自动替换，但正文（Added/Update 等条目）请手动维护。

set -euo pipefail

# --- 参数解析 ---
PUSH=1
VERSION=""
for arg in "$@"; do
  case "$arg" in
    --no-push) PUSH=0 ;;
    -*) echo "未知参数: $arg" >&2; exit 2 ;;
    *)
      if [ -z "$VERSION" ]; then
        VERSION="${arg#v}"   # 允许传 v0.3.15 或 0.3.15
      else
        echo "多余参数: $arg" >&2; exit 2
      fi
      ;;
  esac
done

if [ -z "$VERSION" ]; then
  echo "用法: $0 <version> [--no-push]"
  echo "示例: $0 0.3.15"
  exit 2
fi

TAG="v${VERSION}"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

echo "==> 目标版本: ${VERSION}  (tag: ${TAG})"

# --- 前置检查 ---
BRANCH="$(git rev-parse --abbrev-ref HEAD)"
if [ "$BRANCH" != "master" ]; then
  echo "❌ 当前分支不是 master（而是 ${BRANCH}），发版请在 master 上进行。" >&2
  exit 1
fi

if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "❌ 工作区有未提交的改动，请先 commit 或 stash。" >&2
  git status --short >&2
  exit 1
fi

if git rev-parse "$TAG" >/dev/null 2>&1; then
  echo "❌ tag ${TAG} 已存在。" >&2
  exit 1
fi

LAST_TAG="$(git tag --sort=-v:refname | head -1)"
if [ -n "$LAST_TAG" ]; then
  echo "==> 自 ${LAST_TAG} 以来的提交："
  git log "${LAST_TAG}..HEAD" --oneline | sed 's/^/    /'
fi

# --- 改版本号 ---
echo "==> 更新版本号 → ${VERSION}"
# Cargo.toml（workspace 根 version = "..."）
sed -i -E 's|^version = "[0-9]+\.[0-9]+\.[0-9]+"|version = "'"${VERSION}"'"|' Cargo.toml
# ui/package.json
sed -i -E 's|"version": "[0-9]+\.[0-9]+\.[0-9]+"|"version": "'"${VERSION}"'"|' ui/package.json
# RELEASE_NOTES.md（标题行 + gh release download 行的版本号）
sed -i -E 's|Veil [0-9]+\.[0-9]+\.[0-9]+|Veil '"${VERSION}"'|g' RELEASE_NOTES.md
sed -i -E 's|gh release download v[0-9]+\.[0-9]+\.[0-9]+|gh release download v'"${VERSION}"'|g' RELEASE_NOTES.md

echo "==> 改动预览："
git --no-pager diff

echo
echo "⚠️  请确认 RELEASE_NOTES.md 正文内容已更新（Added/Update 等条目需手动维护）。"
read -r -p "以上版本号改动正确吗？继续将提交 [y/N] " ans
case "$ans" in
  y|Y|yes) ;;
  *) echo "已取消。"; exit 1 ;;
esac

# --- 提交 + 打 tag ---
git add Cargo.toml ui/package.json RELEASE_NOTES.md
git commit -m "chore: bump version to ${TAG}"

git tag "$TAG"
echo "==> 已打 tag ${TAG}"

# --- 推送 ---
if [ "$PUSH" -eq 1 ]; then
  echo
  echo "⚠️  推送 ${TAG} 将触发 CI 在 5 个平台构建并公开发布到 GitHub Releases，不可撤销。"
  read -r -p "确认推送 ${TAG} 到 origin 吗？[y/N] " ans
  case "$ans" in
    y|Y|yes) ;;
    *) echo "已取消推送。本地已提交并打 tag，手动推送请运行: git push origin ${TAG}"; exit 0 ;;
  esac
  git push origin "$TAG"
  echo "==> 已推送 ${TAG}，CI 已触发。"
  echo "    https://github.com/guangcheng9/Veil/actions"
  echo "    https://github.com/guangcheng9/Veil/releases"
else
  echo "==> --no-push：本地已完成提交与打 tag，未推送。"
  echo "    手动推送: git push origin ${TAG}"
fi
