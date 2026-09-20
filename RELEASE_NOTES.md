Veil 0.3.15

## Added

- Rule: 新增规则描述（description）字段，Dashboard 支持展示与编辑。
- UI: 更新失败的错误消息已本地化，并提供对应的失败处理。
- 发版脚本 `scripts/release.sh`：一键完成改版本号 → 提交 → 打 tag → 推送，触发 CI 自动发布。

## Update

Console: Check for updates, or `veil update`.

If GitHub downloads fail:

```
gh release download v0.3.15 --repo Mouseww/veil -p veil-windows-x64.exe
```

## Commits

- 44fa73c feat: add release skill + non-interactive release script
- ece7259 docs: add release script usage to LAUNCH.md
- 0662fce chore: add release helper script
- 5067ebf feat: 新增规则描述字段并支持展示与编辑
- 9e98b57 feat(ui): add localized error messages and handling for update failures
- 5cd1ef8 docs: add DESIGN_proxy_and_lifecycle design doc
- 8dea6a5 style(test, process): 调整代码格式与函数签名换行
- ae98a61 chore: 清理不必要的Claude权限配置文件
