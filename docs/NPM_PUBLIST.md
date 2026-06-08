# NothingTodo npm 发布系统

## 架构概述

使用 npm scoped package `@weibaohui/nothing-todo` 分发 Rust 编译的跨平台二进制。

- **主包** `@weibaohui/nothing-todo`：跨平台 wrapper，安装时自动选择对应平台包
- **平台包** `@weibaohui/nothing-todo-{linux-x64,linux-arm64,darwin-arm64,windows-x64}`：各平台独立包

> 注：实际 `script/npm_publish.sh:58-63` 的 `PLATFORMS` 数组只有 4 个元素，**未打包 `darwin-x64`**（Intel macOS 由 `darwin-arm64` Rosetta 兼容）。

## 目录结构

```
packages/
├── nothing-todo/                    # 主包（wrapper）
│   ├── install.js                  # 跨平台安装脚本
│   └── package.json
├── nothing-todo-linux-x64/         # Linux x86_64 平台包
│   ├── bin/ntd                     # 二进制
│   └── package.json
├── nothing-todo-linux-arm64/       # Linux ARM64 平台包
│   ├── bin/ntd
│   └── package.json
├── nothing-todo-darwin-arm64/      # macOS ARM64 平台包
│   ├── bin/ntd
│   └── package.json
└── nothing-todo-windows-x64/       # Windows x86_64 平台包
    ├── bin/ntd.exe
    └── package.json
```

> 注：当前 `PLATFORMS` 数组不包含 `darwin-x64`（Intel macOS）包；如需新增请同步修改 `script/npm_publish.sh:58-63` 和此目录树。

---

## 快速发布（推荐）

```bash
./script/npm_publish.sh v0.1.2
```

脚本自动执行：
1. 检查交叉编译产物（`backend/target/cross/`）
2. 检查工作区是否干净
3. 创建 Git Tag
4. 同步版本号到所有 `package.json`
5. 提交版本更新
6. 推送代码和标签
7. 按顺序发布平台包到 npm
8. 发布主包

---

## 完整发布流程（按顺序执行）

### 步骤 1: 交叉编译

```bash
make cross-build
```

产物位置：`backend/target/cross/`

### 步骤 2: 复制二进制到平台包目录

```bash
# Linux x64
cp backend/target/cross/ntd-x86_64-unknown-linux-gnu packages/nothing-todo-linux-x64/bin/ntd

# Linux arm64
cp backend/target/cross/ntd-aarch64-unknown-linux-gnu packages/nothing-todo-linux-arm64/bin/ntd

# macOS arm64
cp backend/target/cross/ntd-aarch64-apple-darwin packages/nothing-todo-darwin-arm64/bin/ntd

# Windows x64
cp backend/target/cross/ntd-x86_64-pc-windows-gnu.exe packages/nothing-todo-windows-x64/bin/ntd.exe
```

> **另**：i686（32 位 Windows）二进制当前未打包。`PLATFORMS` 数组仅含 4 个目标（linux x64/arm64、darwin arm64、windows x64）。

### 步骤 3: 检查工作区是否干净

```bash
git status --porcelain
```

### 步骤 4: 创建 Git Tag

```bash
git tag v0.1.2
git tag -l "v*"
```

### 步骤 5: 推送到远程仓库

```bash
git push origin HEAD
git push origin v0.1.2
```

### 步骤 6: 登录 npm

```bash
# 检查是否已登录
npm whoami

# 如果未登录，执行登录
npm login
# 输入用户名: weibaohui
# 输入密码、邮箱、OTP
```

### 步骤 7: 执行发布

```bash
./script/npm_publish.sh v0.1.2
```

或手动分步发布：

```bash
# 1. 同步版本号
for pkg in packages/*/package.json; do
  VERSION=0.1.2
  node -e "const fs=require('fs'); const p=JSON.parse(fs.readFileSync('$pkg','utf8')); p.version='$VERSION'; fs.writeFileSync('$pkg', JSON.stringify(p, null, 2)+'\n');"
done

# 2. 发布平台包（注意：实际只发布 4 个，无 darwin-x64）
cd packages/nothing-todo-linux-x64 && npm publish --access public && cd -
cd packages/nothing-todo-linux-arm64 && npm publish --access public && cd -
cd packages/nothing-todo-darwin-arm64 && npm publish --access public && cd -
cd packages/nothing-todo-windows-x64 && npm publish --access public && cd -

# 3. 发布主包
cd packages/nothing-todo && npm publish --access public && cd -
```

### 步骤 8: 验证发布

```bash
npm view @weibaohui/nothing-todo
npm install -g @weibaohui/nothing-todo
ntd --help
```

---

## 常见问题

### Q: 版本号应该在哪里更新？

版本号统一从 Git Tag 获取（`v` 前缀），脚本自动同步到所有 `package.json`。

### Q: 如果发布失败怎么办？

```bash
# 查看已发布版本
npm view @weibaohui/nothing-todo

# 重新发布（确保版本号已递增）
./script/npm_publish.sh v0.1.3
```

### Q: 如何发布 beta 版本？

```bash
./script/npm_publish.sh v0.2.0-beta.1
```

用户安装 beta 版本：
```bash
npm install -g @weibaohui/nothing-todo@beta
```

### Q: npm 发布需要哪些权限？

- npm 账号：`weibaohui`
- 包必须设置为 public（`--access public`）
- Scoped package `@weibaohui/*` 需要先在 npm 网站创建组织或确认账号有发布权限

---

## 用户安装

```bash
# 全局安装（自动选择对应平台）
npm install -g @weibaohui/nothing-todo

# 或使用 npx（无需安装）
npx @weibaohui/nothing-todo
```

---

## 发布前检查清单

- [ ] 代码已提交并推送
- [ ] 执行 `make cross-build` 生成所有平台二进制
- [ ] 二进制已复制到 `packages/*/bin/` 目录
- [ ] Git Tag 已创建并推送
- [ ] npm 已登录（`npm whoami` 确认）
- [ ] 版本号已正确同步
- [ ] 发布后验证 `npm view @weibaohui/nothing-todo`

---

## 注意事项

1. **Windows 二进制**：Rust 交叉编译产出 `.exe` 文件，npm 包中放在 `bin/ntd.exe`
2. **平台包顺序**：必须先发布所有平台包，最后发布主包（主包作为入口）
3. **跨平台二进制**：Linux/macOS 二进制需要执行权限（`chmod +x`）
4. **交叉编译依赖**：确保 Docker 已运行（cross 工具需要）
