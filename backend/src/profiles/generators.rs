//! 内置执行器的配置生成器。
//!
//! 每个生成器将 `(ExecutorRef, Provider)` 转换为各执行器原生格式的配置文件。
//!
//! # 匹配逻辑
//!
//! 生成器根据 `Provider.protocol` 输出不同格式：
//!
//! | Protocol | 适用范围 | 输出格式 |
//! |----------|---------|----------|
//! | Anthropic | Claude Code | settings.json (env block) |
//! | OpenAI | PI、AtomCode、Kilo 等 | 各执行器的 provider 配置 |

use std::path::PathBuf;

use super::{ExecutorRef, ProfilesConfig, Protocol, Provider};

// ============================================================================
// 备份 helper
// ============================================================================

fn backup_existing_config(config_path: &std::path::Path) {
    if !config_path.exists() {
        return;
    }
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let backup_dir = home.join(".ntd").join("profile_backups");
    if let Err(e) = std::fs::create_dir_all(&backup_dir) {
        tracing::warn!(error = %e, path = %backup_dir.display(), "failed to create backup directory");
        return;
    }

    let file_stem = config_path.file_stem().unwrap_or_default().to_string_lossy();
    let ext = config_path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let backup_name = format!("{}_{}{}", file_stem, timestamp, ext);
    let backup_path = backup_dir.join(&backup_name);

    if let Err(e) = std::fs::copy(config_path, &backup_path) {
        tracing::warn!(error = %e, source = %config_path.display(), target = %backup_path.display(), "failed to backup");
    }

    cleanup_old_backups(&backup_dir, &file_stem, 10);
}

fn cleanup_old_backups(dir: &std::path::Path, prefix: &str, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut backups: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(prefix) && e.path().is_file())
        .collect();
    backups.sort_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()));
    if backups.len() > keep {
        for entry in backups.iter().take(backups.len() - keep) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

// ============================================================================
// ProfileGenerator trait
// ============================================================================

/// 生成器 trait：将 (ExecutorRef, Provider) 转换为执行器原生格式。
///
/// `generate` 接收：
/// - `exec_ref` — Profile 中的执行器引用（含 provider name + model name）
/// - `provider` — 从 Provider Pool 查出的完整 Provider 对象
/// - `session_dir` — 执行器的 session 目录（用于确定配置文件路径）
pub trait ProfileGenerator: Send + Sync {
    fn executor_name(&self) -> &str;

    fn config_path(&self, session_dir: &str) -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let expanded = if session_dir.starts_with('~') {
            let relative = session_dir.trim_start_matches('~').trim_start_matches(std::path::MAIN_SEPARATOR);
            home.join(relative)
        } else {
            PathBuf::from(session_dir)
        };
        expanded.join(self.default_filename())
    }

    fn default_filename(&self) -> &str;

    /// 根据 (exec_ref, provider) 生成配置文件。
    fn generate(&self, exec_ref: &ExecutorRef, provider: &Provider, session_dir: &str) -> Result<(), String>;
}

/// 注册表。
pub struct ProfileGeneratorRegistry {
    generators: HashMap<String, Box<dyn ProfileGenerator>>,
}

impl ProfileGeneratorRegistry {
    pub fn new() -> Self {
        let mut reg = Self { generators: HashMap::new() };
        reg.register(Box::new(ClaudeCodeGenerator));
        reg.register(Box::new(PiGenerator));
        reg.register(Box::new(AtomCodeGenerator));
        reg.register(Box::new(KiloGenerator));
        reg
    }

    pub fn register(&mut self, gen: Box<dyn ProfileGenerator>) {
        let name = gen.executor_name().to_string();
        self.generators.insert(name, gen);
    }

    pub fn get(&self, name: &str) -> Option<&dyn ProfileGenerator> {
        self.generators.get(name).map(|b| b.as_ref())
    }
}

impl Default for ProfileGeneratorRegistry {
    fn default() -> Self { Self::new() }
}

// ============================================================================
// Helper — 从 ProfilesConfig 解析 provider + model
// ============================================================================

/// 解析 exec_ref 为完整的 Provider + model 名称。
/// 如果 provider 不存在或 model 不在列表中，返回 Err。
/// 根据 exec_ref 解析出 provider 和 model。
/// 返回 (Provider 引用, model 名称 clone)。
/// model 名称先尝试从 provider.models 列表中精确匹配，找不到时直接用 exec_ref.model。
pub fn resolve_provider<'a>(
    config: &'a ProfilesConfig,
    exec_ref: &ExecutorRef,
) -> Result<(&'a Provider, String), String> {
    let provider = config.providers.get(&exec_ref.provider)
        .ok_or_else(|| format!("Provider '{}' not found in provider pool", exec_ref.provider))?;

    // 验证 model 存在（允许通过即使不在列表，但给出 warn）
    let model_exists = provider.models.iter().any(|m| m.name == exec_ref.model);
    if !model_exists {
        tracing::warn!(
            provider = %exec_ref.provider,
            model = %exec_ref.model,
            "model not found in provider's model list, will use anyway"
        );
    }

    Ok((provider, exec_ref.model.clone()))
}

// ============================================================================
// Claude Code Generator
// ============================================================================

/// Claude Code → `~/.claude/settings.json`
///
/// 根据 provider.protocol 输出不同格式：
/// - Anthropic 协议 → env 模式（ANTHROPIC_AUTH_TOKEN / ANTHROPIC_BASE_URL / ANTHROPIC_xxx_MODEL）
/// - OpenAI 协议 → OpenAI-compatible provider 配置
pub struct ClaudeCodeGenerator;

impl ProfileGenerator for ClaudeCodeGenerator {
    fn executor_name(&self) -> &str { "claudecode" }
    fn default_filename(&self) -> &str { "settings.json" }

    fn generate(&self, exec_ref: &ExecutorRef, provider: &Provider, session_dir: &str) -> Result<(), String> {
        let expanded_path = expand_config_path(self.config_path(session_dir));
        backup_existing_config(&expanded_path);
        ensure_parent_dir(&expanded_path)?;

        let mut root = serde_json::Map::new();

        if provider.protocol == Protocol::Anthropic {
            // Anthropic 协议：写 env 变量块（Claude Code 原生支持的格式）
            let mut env_map = serde_json::Map::new();
            env_map.insert("ANTHROPIC_AUTH_TOKEN".to_string(), serde_json::Value::String(provider.api_key.clone()));
            env_map.insert("ANTHROPIC_BASE_URL".to_string(), serde_json::Value::String(provider.base_url.clone()));
            env_map.insert("ANTHROPIC_MODEL".to_string(), serde_json::Value::String(exec_ref.model.clone()));
            env_map.insert("API_TIMEOUT_MS".to_string(), serde_json::Value::String("3000000".to_string()));
            env_map.insert("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_string(), serde_json::Value::String("1".to_string()));

            if provider.supports_1m_context {
                // 1M 上下文模型用 [1M] 后缀标记，让 Claude Code 知道扩展上下文能力
                env_map.insert("ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(), serde_json::Value::String(format!("{}[1M]", exec_ref.model)));
                env_map.insert("ANTHROPIC_DEFAULT_FABLE_MODEL".to_string(), serde_json::Value::String(format!("{}[1M]", exec_ref.model)));
                env_map.insert("ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(), serde_json::Value::String(exec_ref.model.clone()));
            }

            root.insert("env".to_string(), serde_json::Value::Object(env_map));
            root.insert("skipDangerousModePermissionPrompt".to_string(), serde_json::Value::Bool(true));
        } else {
            // OpenAI 协议：写 OpenAI-compatible 顶层字段
            let mut cfg_map = serde_json::Map::new();
            cfg_map.insert("apiKey".to_string(), serde_json::Value::String(provider.api_key.clone()));
            cfg_map.insert("baseUrl".to_string(), serde_json::Value::String(provider.base_url.clone()));
            cfg_map.insert("model".to_string(), serde_json::Value::String(exec_ref.model.clone()));
            root.insert("provider".to_string(), serde_json::Value::Object(cfg_map));
        }

        let json = serde_json::to_string_pretty(&root).map_err(|e| format!("serialize: {}", e))?;
        std::fs::write(&expanded_path, &json).map_err(|e| format!("write: {}", e))?;

        tracing::info!(path = %expanded_path.display(), provider = %provider.name, model = %exec_ref.model, "Claude Code config generated");
        Ok(())
    }
}

// ============================================================================
// PI Generator
// ============================================================================

/// PI → `~/.pi/config.yaml`
///
/// PI 使用 provider 级别的 API Key 配置，写入 flat YAML。
pub struct PiGenerator;

impl ProfileGenerator for PiGenerator {
    fn executor_name(&self) -> &str { "pi" }
    fn default_filename(&self) -> &str { "config.yaml" }

    fn generate(&self, exec_ref: &ExecutorRef, provider: &Provider, session_dir: &str) -> Result<(), String> {
        let expanded_path = expand_config_path(self.config_path(session_dir));
        backup_existing_config(&expanded_path);
        ensure_parent_dir(&expanded_path)?;

        // PI 使用 flat key-value 的 YAML 配置
        let mut config = std::collections::HashMap::new();
        config.insert("api_key".to_string(), provider.api_key.clone());
        config.insert("base_url".to_string(), provider.base_url.clone());
        config.insert("default_model".to_string(), exec_ref.model.clone());

        let yaml = serde_yaml::to_string(&config).map_err(|e| format!("serialize: {}", e))?;
        std::fs::write(&expanded_path, &yaml).map_err(|e| format!("write: {}", e))?;

        tracing::info!(path = %expanded_path.display(), "PI config generated");
        Ok(())
    }
}

// ============================================================================
// AtomCode Generator
// ============================================================================

/// AtomCode → `~/.atomcode/config.toml`
///
/// 追加 provider 段到现有 config.toml，不覆写非凭据配置。
pub struct AtomCodeGenerator;

impl ProfileGenerator for AtomCodeGenerator {
    fn executor_name(&self) -> &str { "atomcode" }
    fn default_filename(&self) -> &str { "config.toml" }

    fn generate(&self, exec_ref: &ExecutorRef, provider: &Provider, session_dir: &str) -> Result<(), String> {
        let expanded_path = expand_config_path(self.config_path(session_dir));
        if expanded_path.exists() {
            backup_existing_config(&expanded_path);
        }
        ensure_parent_dir(&expanded_path)?;

        // 生成 provider TOML 段，追加到现有文件之后
        let mut output = String::new();
        output.push_str("\n# ntd profile provider — generated by `ntd profile apply`\n");
        output.push_str("# Do not edit manually; will be overwritten.\n\n");

        let section_name = "ntd-profile";
        output.push_str(&format!("[providers.{}]\n", section_name));
        output.push_str("type = \"openai\"\n");
        output.push_str(&format!("model = {:?}\n", exec_ref.model));
        output.push_str(&format!("base_url = {:?}\n", provider.base_url));
        output.push_str(&format!("api_key = {:?}\n", provider.api_key));

        // 追加到原有配置之后（不覆写）
        let existing = std::fs::read_to_string(&expanded_path).unwrap_or_default();
        let final_content = if existing.trim().is_empty() {
            output
        } else {
            format!("{}{}", existing.trim_end(), output)
        };

        std::fs::write(&expanded_path, &final_content).map_err(|e| format!("write: {}", e))?;

        tracing::info!(path = %expanded_path.display(), "AtomCode config updated");
        Ok(())
    }
}

// ============================================================================
// Kilo Generator
// ============================================================================

/// Kilo → `~/.kilo/config.json`
pub struct KiloGenerator;

impl ProfileGenerator for KiloGenerator {
    fn executor_name(&self) -> &str { "kilo" }
    fn default_filename(&self) -> &str { "config.json" }

    fn generate(&self, exec_ref: &ExecutorRef, provider: &Provider, session_dir: &str) -> Result<(), String> {
        let expanded_path = expand_config_path(self.config_path(session_dir));
        backup_existing_config(&expanded_path);
        ensure_parent_dir(&expanded_path)?;

        let mut map = serde_json::Map::new();
        map.insert("api_key".to_string(), serde_json::Value::String(provider.api_key.clone()));
        map.insert("base_url".to_string(), serde_json::Value::String(provider.base_url.clone()));
        map.insert("model".to_string(), serde_json::Value::String(exec_ref.model.clone()));
        map.insert("provider_type".to_string(), serde_json::Value::String(
            if provider.protocol == Protocol::Anthropic { "anthropic".to_string() } else { "openai".to_string() }
        ));

        let json = serde_json::to_string_pretty(&map).map_err(|e| format!("serialize: {}", e))?;
        std::fs::write(&expanded_path, &json).map_err(|e| format!("write: {}", e))?;

        tracing::info!(path = %expanded_path.display(), "Kilo config generated");
        Ok(())
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn expand_config_path(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    if s.starts_with('~') {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let relative = s.trim_start_matches('~').trim_start_matches(std::path::MAIN_SEPARATOR);
        home.join(relative)
    } else {
        path
    }
}

fn ensure_parent_dir(path: &std::path::Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create dir: {}", e))?;
    }
    Ok(())
}

use std::collections::HashMap;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark, clippy::match_same_arms, clippy::from_over_into, clippy::unwrap_or_default)]
mod tests {
    use super::*;

    fn sample_provider() -> Provider {
        Provider {
            name: "Test AI".to_string(),
            api_key: "sk-test".to_string(),
            base_url: "https://api.test.com/v1".to_string(),
            protocol: Protocol::Openai,
            supports_1m_context: true,
            models: vec![
                super::super::ProviderModel { name: "gpt-4o".to_string(), display_name: Some("GPT-4o".to_string()) },
            ],
        }
    }

    #[test]
    fn test_claude_code_generator_anthropic() {
        let gen = ClaudeCodeGenerator;
        assert_eq!(gen.executor_name(), "claudecode");

        // 验证配置文件路径
        let path = gen.config_path("~/.claude");
        assert!(path.to_string_lossy().contains(".claude/settings.json"));
    }

    #[test]
    fn test_pi_generator() {
        let gen = PiGenerator;
        assert_eq!(gen.executor_name(), "pi");
        assert_eq!(gen.default_filename(), "config.yaml");
    }

    #[test]
    fn test_atomcode_generator() {
        let gen = AtomCodeGenerator;
        assert_eq!(gen.executor_name(), "atomcode");
        assert_eq!(gen.default_filename(), "config.toml");
    }

    #[test]
    fn test_kilo_generator() {
        let gen = KiloGenerator;
        assert_eq!(gen.executor_name(), "kilo");
        assert_eq!(gen.default_filename(), "config.json");
    }

    #[test]
    fn test_resolve_provider_found() {
        let mut cfg = ProfilesConfig::default();
        cfg.providers.insert("test".to_string(), sample_provider());

        let exec_ref = ExecutorRef { provider: "test".to_string(), model: "gpt-4o".to_string() };
        let result = resolve_provider(&cfg, &exec_ref);
        assert!(result.is_ok(), "should resolve: {:?}", result.err());
        let (provider, model) = result.unwrap();
        assert_eq!(provider.api_key, "sk-test");
        assert_eq!(model, "gpt-4o");
    }

    #[test]
    fn test_resolve_provider_not_found() {
        let cfg = ProfilesConfig::default();
        let exec_ref = ExecutorRef { provider: "nonexistent".to_string(), model: "x".to_string() };
        assert!(resolve_provider(&cfg, &exec_ref).is_err());
    }

    #[test]
    fn test_cleanup_old_backups() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();
        for i in 0..15 {
            let f = dir.join(format!("settings_20260722_{:02}0000.bak", i));
            std::fs::write(&f, "bak").unwrap();
        }
        cleanup_old_backups(dir, "settings", 10);
        let remaining: Vec<_> = std::fs::read_dir(dir).unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("settings"))
            .collect();
        assert_eq!(remaining.len(), 10);
    }
}
