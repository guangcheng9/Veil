Veil 0.3.17

## Fixed

- 控制台：新标签页 / 重开浏览器后，保存上游、重启转发等写操作不再返回 401。
  - 启动与 `veil ui` 输出的控制台地址现在自带 `?token=`，可直接点开使用。
  - 管理令牌由 sessionStorage 改为 localStorage 持久化（旧令牌自动迁移）。

## Update

Console: Check for updates, or `veil update`.

If GitHub downloads fail:

```
gh release download v0.3.17 --repo Mouseww/veil -p veil-windows-x64.exe
```

## Commits

- 6154427 fix(console): 控制台地址带管理令牌，令牌改用 localStorage 持久化
- 11e3a38 build: 更新veil相关包版本到0.3.16
