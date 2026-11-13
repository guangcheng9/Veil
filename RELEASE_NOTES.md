Veil 0.3.18

## Added

- 看板：新增数据范围、缓存容量、更新时间与时间窗口覆盖提示，明确每项指标的统计口径。
- 看板：流量和生命周期请求支持取消，页面刷新或切换时不会继续堆积过期请求。
- 管理认证：兼容 `x-dgw-admin-token` 和 Bearer 认证，同时保留现有管理令牌的认证优先级。

## Fixed

- 脱敏引擎：保护 data URL 和多模态 Base64 内容，避免图片、音频和文件载荷被脱敏规则破坏。
- Base64 校验：拒绝非法长度、混合编码和错误 padding。
- 更新与安装：仓库迁移到 `guangcheng9/Veil`，Claude Code 插件版本与正式发布版本保持一致。

## Update

Console: Check for updates, or `veil update`.

If GitHub downloads fail:

```
gh release download v0.3.18 --repo guangcheng9/Veil -p veil-windows-x64.exe
```

## Commits

- 9983c8e feat: 重构仪表盘并新增流量API中止信号支持
- 2f910c0 docs: 记录上游 v0.4.1 安全移植说明
- b1a89da fix: 保护多模态载荷并兼容管理认证
- 39cf272 docs: 添加 Veil 本地迭代记录文档