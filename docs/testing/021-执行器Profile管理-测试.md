# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI | 2026-07-22 | 初始版本 |

# 1. 测试目标

验证执行器 Profile 管理系统的正确性，覆盖：

1. **数据结构**：Profile 数据结构的序列化/反序列化、字段合并/拆分正确性
2. **配置生成器**：各执行器生成器的元数据、路径解析
3. **配置持久化**：profiles.yaml 加载/保存、备份清理
4. **API 校验**：Profile 名称合法性校验
5. **集成验证**：编译零告警 + 全部回归测试通过

# 2. 测试范围

## 2.1 单元测试

### profiles.rs — ProfilesConfig 数据结构

| 测试名 | 覆盖内容 | 验证点 |
|--------|---------|--------|
| `test_profiles_config_default_has_default_profile` | 默认值 | 创建默认 Config 后，`current_profile` 为 "default"，且包含同名 Profile |
| `test_executor_settings_to_map_includes_all_fields` | ExecutorSettings → HashMap | 通用字段 + extra 字段全部出现在 to_map() 结果中 |
| `test_executor_settings_from_map_round_trip` | HashMap → ExecutorSettings | HashMap 反构造后各字段值正确 |
| `test_executor_settings_to_map_empty_extra` | 空 extra | 无字段时 to_map 返回空 map |

### handlers/profiles.rs — API 校验

| 测试名 | 覆盖内容 | 验证点 |
|--------|---------|--------|
| `test_validate_profile_name_valid` | 合法名称 | "default"、"my-profile"、"work_config" 等通过校验 |
| `test_validate_profile_name_invalid` | 非法名称 | 空字符串、含空格、含特殊字符、中文等被拒绝 |

### profiles/generators.rs — 配置生成器

| 测试名 | 覆盖内容 | 验证点 |
|--------|---------|--------|
| `test_claude_code_generates_valid_json` | ClaudeCodeGenerator | executor_name()、default_filename()、config_path() 正确 |
| `test_pi_generator_metadata` | PiGenerator | 同上 |
| `test_atomcode_generator_metadata` | AtomCodeGenerator | 同上 |
| `test_kilo_generator_metadata` | KiloGenerator | 同上 |
| `test_generators_config_path_resolution` | 路径解析 | session_dir 展开后路径包含正确文件名 |
| `test_cleanup_old_backups_keeps_only_n` | 备份清理 | 15 份旧备份清理后只剩 10 份 |
| `test_backup_existing_config_does_not_panic` | 备份不 panic | 备份函数调用不 panic |

# 3. 测试方法

### 3.1 单元测试

全部使用 Rust 原生 `#[test]` 驱动，无需外部依赖。

```bash
cd backend && cargo test --lib profiles 2>&1
cd backend && cargo test --lib handlers::profiles 2>&1
```

### 3.2 集成测试

通过 cargo 回归测试验证系统整体不受影响：

```bash
cd backend && cargo test 2>&1 | tail -5
# 预期输出： test result: ok. 1295 passed; 0 failed
```

### 3.3 代码质量检查

```bash
cd backend && cargo clippy --all-targets -- -D warnings
# 预期输出：零告警零错误
```

### 3.4 前端 TypeScript 检查

```bash
cd frontend && npx tsc --noEmit
# 预期输出：零错误
```

# 4. 测试数据

## 4.1 正常数据

```rust
ExecutorSettings {
    api_key: Some("sk-test-key".to_string()),
    base_url: Some("https://test.api.com".to_string()),
    model: Some("test-model".to_string()),
    extra: [("custom".to_string(), "val".to_string())].into(),
}
```

## 4.2 边界数据

```rust
// 空 extra
ExecutorSettings { api_key: None, base_url: None, model: None, extra: HashMap::new() }

// 极短 profile name: "a"
// 含中划线的 profile name: "my-profile"
// 含下划线的 profile name: "work_config"
```

## 4.3 异常数据

```rust
// 空名称
validate_profile_name("") → Err
// 含空格
validate_profile_name("with space") → Err  
// 含特殊字符
validate_profile_name("special!") → Err
// 非 ASCII
validate_profile_name("中文") → Err
```

# 5. 测试通过标准

- 所有单元测试通过（1295+ 个测试）
- `cargo clippy --all-targets -- -D warnings` 零告警
- 前端 `npx tsc --noEmit` 零错误
- 新增代码行覆盖率 > 80%

# 6. 安全测试要点

- 切换 Profile 前自动备份原配置文件，不直接覆写
- 备份目录 `~/.ntd/profile_backups/` 自动创建
- 备份清理只删超出保留数量的旧文件，不删不相关文件
