# 关于 / 版本升级

> **位置**：设置 → 关于
> **前端**：`frontend/src/components/settings/AboutPanel.tsx`
> **后端**：`backend/src/handlers/mod.rs`（`version_handler` / `version_latest_handler` / `version_upgrade_handler`）

查看 ntd 版本、git 信息、检查最新版本、一键升级。

---

## 1. 版本信息

| 字段 | 来源 |
|------|------|
| Version | `NTD_VERSION`（编译时注入） |
| Git SHA | `NTD_GIT_SHA` |
| Git Describe | `NTD_VERSION_FULL`（如 `v1.2.3-4-gabcdef`） |

开发模式（`NTD_MODE=dev`）下都是 `unknown`。

---

## 2. 检查最新版本

### 2.1 手动检查

点「**检查更新**」按钮 → 后端调 `npm view @weibaohui/nothing-todo version` → 显示远端最新版本号。

### 2.2 定时检查

- 页面 mount 时**静默**调一次
- 启动 WebSocket 也会定期（每小时）推一个「有新版本」事件
- 右上角小红点提示

### 2.3 API

| Method | Path |
|--------|------|
| GET | `/api/version` |
| GET | `/api/version/latest` |

返回示例：
```json
{
  "code": 0,
  "data": { "latest": "1.3.0" },
  "message": "ok"
}
```

---

## 3. 一键升级

### 3.1 流程

点「**一键升级**」→ 后端 `POST /api/version/upgrade`：

1. 后端跑 `npm install -g @weibaohui/nothing-todo@latest`
   - 拿到 stdout / stderr
   - 失败立即返回错误
2. 成功后跑 `ntd daemon restart`
3. **重启后当前进程被 kill，HTTP 响应不会发出**
4. 客户端等待超时后会重新连接，看到新版本

### 3.2 API

| Method | Path |
|--------|------|
| POST | `/api/version/upgrade` |

返回示例（成功路径在重启前发出）：
```json
{
  "upgraded": true,
  "restarted": true,
  "npmOutput": "...",
  "restartMessage": ""
}
```

> ⚠️ 如果 npm 升级失败，**不会重启**，ntd 继续跑老版本。

### 3.3 注意事项

- 升级会**丢失当前进程状态**（WebSocket 断连、内存中的数据没了）
- 升级前建议先手动触发一次**数据库备份**
- 升级期间正在跑的 Todo 会被 SIGKILL，状态变 failed
- 跨大版本可能需要**清浏览器缓存**（前端 dist 文件变了）

---

## 4. 分享卡

面板底部有「**分享给朋友**」按钮 → 生成一张带版本号、QR 码、安装指引的图。

适合推给同事/朋友，让他们扫码装 ntd。

---

## 5. 故障排查

### 5.1 检查更新失败「npm view command not found」

- 没装 Node.js
- 或 PATH 不对
- macOS：`brew install node`

### 5.2 升级失败「Permission denied」

- npm 全局安装需要 root 或 sudo
- 后端用 `tokio::process::Command` 直接跑，**不自动加 sudo**
- 解决：用 root 用户跑 ntd 服务，或把 npm 全局目录设成当前用户可写：
  ```bash
  npm config set prefix '~/.npm-global'
  export PATH=~/.npm-global/bin:$PATH
  ```

### 5.3 升级后还是老版本

- 看 `ntd --version` 输出版本
- 检查 PATH：`which ntd`
- 多版本共存时旧版优先 → 删旧版

---

## 6. 升级 vs 重新部署

| 方式 | 适用 |
|------|------|
| 一键升级 | npm 全局安装的标准部署 |
| 重新部署 | 二进制部署（`ntd daemon install`）、Docker |
| Git pull + 手动 build | 开发模式 |

一键升级只走 npm 渠道；其他部署方式需要你自己 pull 代码 + build + restart。
