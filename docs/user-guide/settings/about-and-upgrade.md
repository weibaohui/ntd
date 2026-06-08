# 关于 / 版本升级

> **位置**：设置 →关于
> **前端**：`frontend/src/components/settings/AboutPanel.tsx`
> **后端**：`backend/src/handlers/mod.rs`（`version_handler` / `version_latest_handler` / `version_upgrade_handler`）

查看 ntd版本、git信息、检查最新版本、一键升级。

---

## 1. 版本信息

|字段 |来源 |
|------|------|
| Version | `NTD_VERSION`（编译时注入） |
| Git SHA | `NTD_GIT_SHA` |
| Git Describe | `NTD_VERSION_FULL`（如 `v1.2.3-4-gabcdef`） |

开发模式（`NTD_MODE=dev`）下都是 `unknown`。

---

## 2.检查最新版本

### 2.1手动检查

点「**检查更新**」按钮 → 后端调 `npm view @weibaohui/nothing-todo version` → 显示远端最新版本号。

### 2.2定时检查

-页面 mount时**静默**调一次
- **目前没有** WebSocket新版本推送机制，需用户**手动**点「检查更新」按钮才能再次触发
-右上角小红点提示

>说明：早期版本曾规划过「WebSocket也会定期推新版本」，但当前实现**没有**该推送通道。后续如需，应在 `backend/src/handlers/mod.rs` 的 events handler 中加定时任务。

### 2.3 API

| Method | Path |
|--------|------|
| GET | `/api/version` |
| GET | `/api/version/latest` |

返回示例：
```json
{
 "code":0,
 "data": { "latest": "1.3.0" },
 "message": "ok"
}
```

---

## 3.一键升级

### 3.1流程

点「**一键升级**」→ 后端 `POST /api/version/upgrade`：

1. 后端跑 `npm install -g --prefix={prefix} @weibaohui/nothing-todo@latest`
 - `prefix` 由后端探测 npm 全局目录写权限决定（无写权限时回落到 `~/.npm-global`）
 -拿到 stdout / stderr
 -失败立即返回错误
2.成功后 fork 一个后台线程跑 daemon 重部署（**HTTP响应会正常返回，daemon 重启在后台异步进行**）：
 - `ntd daemon stop` —停掉当前服务（停失败不阻断，可能已停）
 - `ntd daemon uninstall` —卸载旧服务配置（清掉 plist/systemd unit）
 - `ntd daemon install --force` — **必须传 `--force`**，否则已存在配置会静默跳过，binary路径不会更新
 - `ntd daemon start` —启动新版本服务
3. 用户收到 HTTP响应后**稍后刷新页面**即可看到新版本

### 3.2 API

| Method | Path |
|--------|------|
| POST | `/api/version/upgrade` |

返回示例：
```json
{
 "upgraded": true,
 "restarted": true,
 "npmOutput": "...",
 "restartMessage": "npm升级成功，正在后台重新部署服务，请稍后刷新页面"
}
```

> ⚠️ 如果 npm升级失败，**不会重启**，ntd继续跑老版本。

### 3.3注意事项

-升级会**丢失当前进程状态**（WebSocket断连、内存中的数据没了）
-升级前建议先手动触发一次**数据库备份**
-升级期间正在跑的 Todo会被 SIGKILL，状态变 failed
-跨大版本可能需要**清浏览器缓存**（前端 dist文件变了）

---

## 4.分享卡

面板底部有「**分享给朋友**」按钮 → 生成一张带版本号、QR码、安装指引的图。

适合推给同事/朋友，让他们扫码装 ntd。

---

## 5.故障排查

### 5.1检查更新失败「npm view command not found」

- 没装 Node.js
- 或 PATH不对
- macOS：`brew install node`

### 5.2升级失败「Permission denied」

- npm全局安装需要 root或 sudo
- 后端用 `tokio::process::Command`直接跑，**不自动加 sudo**
-解决：用 root用户跑 ntd 服务，或把 npm全局目录设成当前用户可写：
 ```bash
 npm config set prefix '~/.npm-global'
 export PATH=~/.npm-global/bin:$PATH
 ```

### 5.3升级后还是老版本

- 看 `ntd --version`输出版本
- 检查 PATH：`which ntd`
- 多版本共存时旧版优先 →删旧版

---

## 6.升级 vs重新部署

|方式 |适用 |
|------|------|
| 一键升级 | npm全局安装的标准部署 |
|重新部署 |二进制部署（`ntd daemon install`）、Docker |
| Git pull +手动 build |开发模式 |

一键升级只走 npm渠道；其他部署方式需要你自己 pull 代码 + build + restart。
