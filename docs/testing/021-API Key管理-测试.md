# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI | 2026-07-23 | 初始版本 |

# 1. 测试目标

验证 API Key 管理系统的正确性，覆盖：

1. **数据结构**：数据结构的序列化/反序列化、字段合并/拆分
2. **配置生成器**：4 个生成器的元数据、路径解析、写入逻辑
3. **去重逻辑**：AtomCode 重复 apply 不重复追加
4. **备份机制**：备份文件创建 + 自动清理
5. **API 校验**：执行器名称校验、协议映射
6. **集成验证**：编译零告警 + 全部回归测试通过

# 2. 单元测试

## profiles.rs — 数据结构

| 测试名 | 覆盖内容 | 验证点 |
|--------|---------|--------|
| `test_default_has_default_profile` | 默认值 | 默认配置包含 "default" profile，providers 为空 |
| `test_executor_ref_resolve` | model 引用解析 | 对 `exec_ref` 正确从 profiles.yaml 找到 Provider + Model |
| `test_resolve_provider_not_found` | 异常路径 | 找不到 Provider 时返回错误 |

## profiles/generators.rs — 配置生成器

| 测试名 | 覆盖内容 | 验证点 |
|--------|---------|--------|
| `test_claude_code_generator_anthropic` | ClaudeCodeGenerator | executor_name / config_path |
| `test_pi_generator` | PiGenerator | executor_name / default_filename = "models.json" |
| `test_atomcode_generator` | AtomCodeGenerator | default_filename = "config.toml" |
| `test_kilo_generator` | KiloGenerator | default_filename = "config.json" |
| `test_resolve_provider_found` | resolve_provider 成功路径 | 返回 Provider + Model |
| `test_resolve_provider_not_found` | resolve_provider 异常 | Provider 不存在返回错误 |
| `test_backup_creates_bak_file` | 备份逻辑 | 写入 config.json 后产生 `settings.json.bak-*` 文件 |

## handlers/profiles.rs — API 校验

| 测试名 | 覆盖内容 | 验证点 |
|--------|---------|--------|
| `test_validate_profile_name_valid` | 合法名称 | `default` / `my-profile` / `work_config` / `a` 通过 |
| `test_validate_profile_name_invalid` | 非法名称 | 空字符串 / 含空格 / 含特殊字符 / 中文 被拒 |

# 3. 集成测试

通过 cargo 回归测试验证：

```bash
cd backend && cargo test 2>&1 | tail -5
# 预期：test result: ok. 1294 passed; 0 failed
```

```bash
cd backend && cargo clippy --all-targets -- -D warnings
# 预期：零告警零错误
```

```bash
cd frontend && npx tsc --noEmit
# 预期：零错误
```

# 4. 测试方法

### 4.1 单元测试

使用 Rust 内置 `#[test]` / `#[tokio::test]` 框架，无需外部依赖。

```bash
# 运行所有单元测试
cd backend && cargo test --lib

# 运行指定模块
cd backend && cargo test profiles
```

### 4.2 API 端到端测试

启动 dev server 后通过 curl 调用：

```bash
# 健康检查
curl -s http://localhost:18088/health

# 列出供应商
curl -s http://localhost:18088/api/v1/providers

# 创建供应商
curl -s -X POST http://localhost:18088/api/v1/providers \
  -H 'Content-Type: application/json' \
  -d '{"name":"test","display_name":"测试","api_key":"sk-x","base_url":"https://test.com","protocol":"openai","models":[]}'

# 预览应用
curl -s -X POST http://localhost:18088/api/v1/providers/{name}/preview \
  -H 'Content-Type: application/json' \
  -d '{"executor_models":{"claudecode":"deepseek-v4-flash"}}'

# 实际应用
curl -s -X POST http://localhost:18088/api/v1/providers/{name}/apply \
  -H 'Content-Type: application/json' \
  -d '{"executor_models":{"claudecode":"deepseek-v4-flash"}}'

# 验证备份
ls -la ~/.claude/  # 应看到 settings.json.bak-*
```

# 5. 测试数据

## 5.1 合法数据

```rust
Provider {
    name: "Test AI",
    api_key: "sk-test",
    base_url: "https://api.test.com/v1",
    protocol: Protocol::Openai,
    models: vec![
        ProviderModel { name: "gpt-4o", display_name: Some("GPT-4o"), supports_1m_context: false },
    ],
}
```

## 5.2 边界数据

```rust
// 只有 1 个模型，且 supports_1m_context = true
ProviderModel { name: "m", display_name: None, supports_1m_context: true }

// 没有 models（generate 会用空数组，但前端的模型选择下拉会变空）
Provider { models: vec![] }
```

## 5.3 异常数据

```rust
// 空名称 → 后端 BadRequest
name: ""
// 含特殊字符 → 后端 BadRequest
name: "with space"
name: "special!"
// Provider 不存在 → resolve_provider 错误
exec_ref.provider = "nonexistent"
```

# 6. 测试通过标准

- 所有单元测试通过（1294+ 个）
- cargo clippy -- -D warnings 零告警
- 前端 npx tsc --noEmit 零错误
- 端到端 API 测试：
  - 创建 / 列表 / 详情 / 更新 / 删除 API Key
  - 执行器配置定义 API 返回 13 个执行器（4 个有生成器）
  - 预览 API 返回正确路径 + 内容
  - Apply API 写入文件 + 创建备份
  - AtomCode 多次 apply 后文件只有一份 `[providers.ntd-profile]` 段
  - PI apply 后 `models.json` 中 provider 条目可重复 apply 不重复

# 7. 安全测试要点

- 备份文件创建在原文件同目录（不会污染其他目录）
- 备份最近 5 份，最旧的自动清理
- 文件权限继承父目录（依赖 chmod 600）
- API 请求体缺失字段时不报错，使用 `#[serde(default)]` 兜底
