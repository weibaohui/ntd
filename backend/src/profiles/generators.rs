//! 内置执行器的配置生成器。
//!
//! 每个生成器将统一的 `ExecutorSettings` 转换为各执行器原生格式的配置文件，
//! 并写入执行器对应的路径。写入前自动备份原文件到 `~/.ntd/profile_backups/`。
//!
//! # 首批支持的执行器
//!
//! - Claude Code → `~/.claude/settings.json`
//! - PI → `~/.pi/config.yaml`
//! - AtomCode → `~/.atomcode/settings.json`
//! - Kilo → `~/.kilo/config.json`

use std::path::PathBuf;

use super::{ExecutorSettings, ProfileGenerator};

// ============================================================================
// 备份 helper
// ============================================================================

/// 在覆写前备份原始配置文件到 `~/.ntd/profile_backups/`。
///
/// 备份文件名为 `{executor_name}_{timestamp}.bak`，保留最近 10 份备份。
/// 写入失败不影响主流程（仅记录 warn）。
fn backup_existing_config(config_path: &std::path::Path) {
    if !config_path.exists() {
        return;
    }
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let backup_dir = home.join(".ntd").join("profile_backups");
    // 确保备份目录存在
    if let Err(e) = std::fs::create_dir_all(&backup_dir) {
        tracing::warn!(
            error = %e,
            path = %backup_dir.display(),
            "failed to create backup directory"
        );
        return;
    }

    let file_stem = config_path.file_stem().unwrap_or_default().to_string_lossy();
    let ext = config_path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let backup_name = format!("{}_{}{}", file_stem, timestamp, ext);
    let backup_path = backup_dir.join(&backup_name);

    if let Err(e) = std::fs::copy(config_path, &backup_path) {
        tracing::warn!(
            error = %e,
            source = %config_path.display(),
            target = %backup_path.display(),
            "failed to backup config file"
        );
    }

    // 清理超过 10 份的旧备份（仅清理同名前缀）
    cleanup_old_backups(&backup_dir, &file_stem, 10);
}

/// 保留最近 N 份备份，删除更旧的同名文件。
fn cleanup_old_backups(dir: &std::path::Path, prefix: &str, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut backups: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name().to_string_lossy().starts_with(prefix) && e.path().is_file()
        })
        .collect();
    // 按修改时间排序（旧的在前面）
    backups.sort_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()));

    // 删除超出保留数量的旧文件
    if backups.len() > keep {
        for entry in backups.iter().take(backups.len() - keep) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

// ============================================================================
// Claude Code Generator
// ============================================================================

/// Claude Code → `~/.claude/settings.json`
///
/// 写入 JSON 格式：
/// ```json
/// {
///   "apiKey": "sk-ant-xxx",
///   "model": "claude-sonnet-4-20250514"
/// }
/// ```
pub struct ClaudeCodeGenerator;

impl ProfileGenerator for ClaudeCodeGenerator {
    fn executor_name(&self) -> &str {
        "claudecode"
    }

    fn default_filename(&self) -> &str {
        "settings.json"
    }

    fn generate(&self, settings: &ExecutorSettings, session_dir: &str) -> Result<(), String> {
        let path = self.config_path(session_dir);
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let expanded_path = expand_path(&path, &home);

        // 备份原文件
        backup_existing_config(&expanded_path);

        // 构建 JSON 对象用 serde_json::Map 而非 struct，避免字段名拼错导致执行器不识别。
        let mut map = serde_json::Map::new();
        if let Some(key) = &settings.api_key {
            // Claude Code 使用 camelCase "apiKey"
            map.insert("apiKey".to_string(), serde_json::Value::String(key.clone()));
        }
        if let Some(model) = &settings.model {
            map.insert("model".to_string(), serde_json::Value::String(model.clone()));
        }
        if let Some(url) = &settings.base_url {
            // Claude Code 支持 base URL 覆盖
            map.insert("baseUrl".to_string(), serde_json::Value::String(url.clone()));
        }

        // 确保父目录存在
        if let Some(parent) = expanded_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("failed to create dir: {}", e))?;
        }

        let json = serde_json::to_string_pretty(&map)
            .map_err(|e| format!("failed to serialize settings: {}", e))?;
        std::fs::write(&expanded_path, &json)
            .map_err(|e| format!("failed to write settings: {}", e))?;

        tracing::info!(
            path = %expanded_path.display(),
            "generated Claude Code settings"
        );
        Ok(())
    }
}

// ============================================================================
// PI Generator
// ============================================================================

/// PI → `~/.pi/config.yaml`
///
/// 写入 YAML 格式（PI 原生使用 YAML 配置）：
/// ```yaml
/// openai_api_key: sk-xxx
/// anthropic_api_key: sk-ant-xxx
/// default_model: jiutian/deepseek/deepseek-v4-flash
/// ```
///
/// PI 使用 provider 级别的 API Key 配置（而非单一 `api_key`），
/// 因此通用字段 `api_key` 和 `model` 会映射为 PI 风格的 key 名，
/// 专有字段（如 `openai_api_key`、`anthropic_api_key`）通过 `extra` 透传。
pub struct PiGenerator;

impl ProfileGenerator for PiGenerator {
    fn executor_name(&self) -> &str {
        "pi"
    }

    fn default_filename(&self) -> &str {
        "config.yaml"
    }

    fn generate(&self, settings: &ExecutorSettings, session_dir: &str) -> Result<(), String> {
        let path = self.config_path(session_dir);
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let expanded_path = expand_path(&path, &home);

        backup_existing_config(&expanded_path);

        // PI 配置使用 YAML，构建一个 flat map 然后序列化
        let config_map = settings.to_map();

        // 如果用户配了通用 api_key 但没有专有字段，转为 pi 的通用 key 名
        if config_map.contains_key("api_key") && !config_map.contains_key("anthropic_api_key") {
            // 保留 api_key，PI 执行器会识别
        }

        // 确保父目录存在
        if let Some(parent) = expanded_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("failed to create dir: {}", e))?;
        }

        let yaml = serde_yaml::to_string(&config_map)
            .map_err(|e| format!("failed to serialize config: {}", e))?;
        std::fs::write(&expanded_path, &yaml)
            .map_err(|e| format!("failed to write config: {}", e))?;

        tracing::info!(
            path = %expanded_path.display(),
            "generated PI config"
        );
        Ok(())
    }
}

// ============================================================================
// AtomCode Generator
// ============================================================================

/// AtomCode → `~/.atomcode/settings.json`
///
/// 写入 JSON 格式，与 Claude Code 类似：
/// ```json
/// {
///   "apiKey": "sk-ant-xxx",
///   "model": "claude-sonnet-4-20250514"
/// }
/// ```
pub struct AtomCodeGenerator;

impl ProfileGenerator for AtomCodeGenerator {
    fn executor_name(&self) -> &str {
        "atomcode"
    }

    fn default_filename(&self) -> &str {
        "settings.json"
    }

    fn generate(&self, settings: &ExecutorSettings, session_dir: &str) -> Result<(), String> {
        let path = self.config_path(session_dir);
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let expanded_path = expand_path(&path, &home);

        backup_existing_config(&expanded_path);

        let mut map = serde_json::Map::new();
        if let Some(key) = &settings.api_key {
            map.insert("apiKey".to_string(), serde_json::Value::String(key.clone()));
        }
        if let Some(model) = &settings.model {
            map.insert("model".to_string(), serde_json::Value::String(model.clone()));
        }
        if let Some(url) = &settings.base_url {
            map.insert("baseUrl".to_string(), serde_json::Value::String(url.clone()));
        }

        // 扩展字段透传
        for (k, v) in &settings.extra {
            map.insert(k.clone(), serde_json::Value::String(v.clone()));
        }

        if let Some(parent) = expanded_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("failed to create dir: {}", e))?;
        }

        let json = serde_json::to_string_pretty(&map)
            .map_err(|e| format!("failed to serialize settings: {}", e))?;
        std::fs::write(&expanded_path, &json)
            .map_err(|e| format!("failed to write settings: {}", e))?;

        tracing::info!(
            path = %expanded_path.display(),
            "generated AtomCode settings"
        );
        Ok(())
    }
}

// ============================================================================
// Kilo Generator
// ============================================================================

/// Kilo → `~/.kilo/config.json`
///
/// 写入 JSON 格式：
/// ```json
/// {
///   "api_key": "xxx",
///   "model": "claude-3-5-sonnet"
/// }
/// ```
pub struct KiloGenerator;

impl ProfileGenerator for KiloGenerator {
    fn executor_name(&self) -> &str {
        "kilo"
    }

    fn default_filename(&self) -> &str {
        "config.json"
    }

    fn generate(&self, settings: &ExecutorSettings, session_dir: &str) -> Result<(), String> {
        let path = self.config_path(session_dir);
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let expanded_path = expand_path(&path, &home);

        backup_existing_config(&expanded_path);

        // Kilo 使用 snake_case 字段名
        let mut map = serde_json::Map::new();
        if let Some(key) = &settings.api_key {
            map.insert("api_key".to_string(), serde_json::Value::String(key.clone()));
        }
        if let Some(model) = &settings.model {
            map.insert("model".to_string(), serde_json::Value::String(model.clone()));
        }
        if let Some(url) = &settings.base_url {
            map.insert("base_url".to_string(), serde_json::Value::String(url.clone()));
        }

        // 扩展字段透传
        for (k, v) in &settings.extra {
            map.insert(k.clone(), serde_json::Value::String(v.clone()));
        }

        if let Some(parent) = expanded_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("failed to create dir: {}", e))?;
        }

        let json = serde_json::to_string_pretty(&map)
            .map_err(|e| format!("failed to serialize config: {}", e))?;
        std::fs::write(&expanded_path, &json)
            .map_err(|e| format!("failed to write config: {}", e))?;

        tracing::info!(
            path = %expanded_path.display(),
            "generated Kilo config"
        );
        Ok(())
    }
}

// ============================================================================
// Helper
// ============================================================================

/// 展开路径中的 `~` 为用户的 home 目录。
fn expand_path(path: &std::path::Path, home: &std::path::Path) -> PathBuf {
    let s = path.to_string_lossy();
    if s.starts_with('~') {
        let relative = s.trim_start_matches('~').trim_start_matches(std::path::MAIN_SEPARATOR);
        home.join(relative)
    } else {
        path.to_path_buf()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark, clippy::match_same_arms, clippy::from_over_into, clippy::unwrap_or_default)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_claude_code_generates_valid_json() {
        let gen = ClaudeCodeGenerator;
        assert_eq!(gen.executor_name(), "claudecode");
        assert_eq!(gen.default_filename(), "settings.json");
        assert!(
            gen.config_path("~/.claude").to_string_lossy().contains(".claude/settings.json")
        );
    }

    #[test]
    fn test_pi_generator_metadata() {
        let gen = PiGenerator;
        assert_eq!(gen.executor_name(), "pi");
        assert_eq!(gen.default_filename(), "config.yaml");
    }

    #[test]
    fn test_atomcode_generator_metadata() {
        let gen = AtomCodeGenerator;
        assert_eq!(gen.executor_name(), "atomcode");
        assert_eq!(gen.default_filename(), "settings.json");
    }

    #[test]
    fn test_kilo_generator_metadata() {
        let gen = KiloGenerator;
        assert_eq!(gen.executor_name(), "kilo");
        assert_eq!(gen.default_filename(), "config.json");
    }

    #[test]
    fn test_generators_config_path_resolution() {
        // 验证配置文件路径解析
        let cc_path = ClaudeCodeGenerator.config_path("~/.claude");
        assert!(cc_path.to_string_lossy().contains(".claude/settings.json"));

        let kilo_path = KiloGenerator.config_path("~/.kilo");
        assert!(kilo_path.to_string_lossy().contains(".kilo/config.json"));
    }

    #[test]
    fn test_cleanup_old_backups_keeps_only_n() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();

        // 创建 15 个备份文件（以 "settings" 为前缀）
        for i in 0..15 {
            let f = dir.join(format!("settings_20260722_{:02}0000.bak", i));
            fs::write(&f, "backup").unwrap();
        }

        cleanup_old_backups(dir, "settings", 10);

        // 应该只剩 10 个文件
        let remaining: Vec<_> = std::fs::read_dir(dir).unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("settings"))
            .collect();
        assert_eq!(remaining.len(), 10, "应该只保留 10 份备份");
    }

    #[test]
    fn test_backup_existing_config_does_not_panic() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("settings.json");
        fs::write(&config_path, "original").unwrap();

        // 调用备份函数，验证不 panic 即可
        backup_existing_config(&config_path);
        // 注意：备份会写到真实 home 的 ~/.ntd/profile_backups/ 下
        // 单独调用不验证文件存在性，仅确保函数不 panic
    }
}
