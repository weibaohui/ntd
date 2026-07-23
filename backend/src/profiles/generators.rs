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
use serde::Serialize;

use super::{ExecutorRef, ProfilesConfig, Protocol, Provider};

// ============================================================================
// 备份 helper
// ============================================================================

/// 备份原始配置文件：在同目录创建 `{原文件名}.bak-{时间戳}`。
/// 保留最近 5 份备份，超过的自动清理最旧的。
fn backup_existing_config(config_path: &std::path::Path) {
    if !config_path.exists() {
        return;
    }
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let backup_path = config_path.with_extension(format!(
        "{}.bak-{}",
        config_path.extension().map(|e| e.to_string_lossy()).unwrap_or_default(),
        timestamp
    ));

    if let Err(e) = std::fs::copy(config_path, &backup_path) {
        tracing::warn!(error = %e, source = %config_path.display(), target = %backup_path.display(), "failed to backup");
    }

    // 清理同一目录下同模式的旧备份，保留最近 5 份
    if let Some(dir) = config_path.parent() {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        let mut backups: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name_os = e.file_name();
                let name = name_os.to_string_lossy();
                let file_name_os = config_path.file_name().unwrap_or_default();
                let file_name = file_name_os.to_string_lossy();
                name.starts_with(file_name.as_ref())
                    && name.contains(".bak-")
                    && e.path().is_file()
            })
            .collect();
        backups.sort_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()));
        if backups.len() > 5 {
            for entry in backups.iter().take(backups.len() - 5) {
                let _ = std::fs::remove_file(entry.path());
            }
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

    /// 根据 (exec_ref, provider) 生成配置文件的预览内容（不写盘）。
    /// 返回 (目标文件路径, 文件内容)。
    fn preview(&self, exec_ref: &ExecutorRef, provider: &Provider, session_dir: &str) -> Result<(String, String), String>;

    /// 根据 (exec_ref, provider) 生成配置文件并写入磁盘。
    fn generate(&self, exec_ref: &ExecutorRef, provider: &Provider, session_dir: &str) -> Result<(), String>;
}

/// 注册表。
pub struct ProfileGeneratorRegistry {
    generators: std::collections::HashMap<String, Box<dyn ProfileGenerator>>,
}

impl ProfileGeneratorRegistry {
    pub fn new() -> Self {
        let mut reg = Self { generators: std::collections::HashMap::new() };
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

    fn preview(&self, exec_ref: &ExecutorRef, provider: &Provider, session_dir: &str) -> Result<(String, String), String> {
        let expanded_path = expand_config_path(self.config_path(session_dir));
        let content = Self::render_json(exec_ref, provider)?;
        Ok((expanded_path.to_string_lossy().to_string(), content))
    }

    fn generate(&self, exec_ref: &ExecutorRef, provider: &Provider, session_dir: &str) -> Result<(), String> {
        let (path_str, content) = self.preview(exec_ref, provider, session_dir)?;
        let expanded_path = std::path::PathBuf::from(&path_str);
        backup_existing_config(&expanded_path);
        ensure_parent_dir(&expanded_path)?;
        std::fs::write(&expanded_path, &content).map_err(|e| format!("write: {}", e))?;
        tracing::info!(path = %path_str, "Claude Code config generated");
        Ok(())
    }
}

impl ClaudeCodeGenerator {
    fn render_json(exec_ref: &ExecutorRef, provider: &Provider) -> Result<String, String> {
        let mut root = serde_json::Map::new();
        if provider.protocol == Protocol::Anthropic {
            let mut env_map = serde_json::Map::new();
            env_map.insert("ANTHROPIC_AUTH_TOKEN".to_string(), serde_json::Value::String(provider.api_key.clone()));
            env_map.insert("ANTHROPIC_BASE_URL".to_string(), serde_json::Value::String(provider.base_url.clone()));
            env_map.insert("ANTHROPIC_MODEL".to_string(), serde_json::Value::String(exec_ref.model.clone()));
            env_map.insert("API_TIMEOUT_MS".to_string(), serde_json::Value::String("3000000".to_string()));
            env_map.insert("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_string(), serde_json::Value::String("1".to_string()));
            let model_supports_1m = provider.models.iter().any(|m| m.name == exec_ref.model && m.supports_1m_context);
            if model_supports_1m {
                env_map.insert("ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(), serde_json::Value::String(format!("{}[1M]", exec_ref.model)));
                env_map.insert("ANTHROPIC_DEFAULT_FABLE_MODEL".to_string(), serde_json::Value::String(format!("{}[1M]", exec_ref.model)));
                env_map.insert("ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(), serde_json::Value::String(exec_ref.model.clone()));
            }
            root.insert("env".to_string(), serde_json::Value::Object(env_map));
            root.insert("skipDangerousModePermissionPrompt".to_string(), serde_json::Value::Bool(true));
        } else {
            let mut cfg_map = serde_json::Map::new();
            cfg_map.insert("apiKey".to_string(), serde_json::Value::String(provider.api_key.clone()));
            cfg_map.insert("baseUrl".to_string(), serde_json::Value::String(provider.base_url.clone()));
            cfg_map.insert("model".to_string(), serde_json::Value::String(exec_ref.model.clone()));
            root.insert("provider".to_string(), serde_json::Value::Object(cfg_map));
        }
        serde_json::to_string_pretty(&root).map_err(|e| format!("serialize: {}", e))
    }
}

// ============================================================================
// PI Generator
// ============================================================================

/// PI → `~/.pi/agent/models.json` + `~/.pi/agent/settings.json`
///
/// PI 使用 `~/.pi/agent/models.json` 管理 provider 定义（含多模型），
/// `~/.pi/agent/settings.json` 选择默认 provider 和 model。
/// 生成器会追加/更新一个 provider 条目到 models.json，
/// 同时更新 settings.json 的 defaultProvider/defaultModel。
///
/// 协议映射：
/// - Protocol::Anthropic → `"api": "anthropic-messages"`
/// - Protocol::Openai → `"api": "openai-completions"`
pub struct PiGenerator;

impl PiGenerator {
    /// 渲染 models.json 的新 provider 条目（不写盘）。
    fn render_provider_entry(provider: &Provider) -> serde_json::Value {
        let api_type = if provider.protocol == Protocol::Anthropic {
            "anthropic-messages"
        } else {
            "openai-completions"
        };
        let models: Vec<serde_json::Value> = provider.models.iter().map(|m| {
            let mut model = serde_json::Map::new();
            model.insert("id".to_string(), serde_json::Value::String(m.name.clone()));
            if let Some(ref dn) = m.display_name {
                model.insert("name".to_string(), serde_json::Value::String(dn.clone()));
            }
            model.insert("input".to_string(), serde_json::Value::Array(vec![
                serde_json::Value::String("text".to_string()),
            ]));
            // 将 supports_1m_context (bool) 转为 contextWindow 数值
            model.insert("contextWindow".to_string(), serde_json::Value::Number(
                serde_json::Number::from(if m.supports_1m_context { 1000000u64 } else { 128000u64 })
            ));
            serde_json::Value::Object(model)
        }).collect();

        let mut entry = serde_json::Map::new();
        entry.insert("baseUrl".to_string(), serde_json::Value::String(provider.base_url.clone()));
        entry.insert("api".to_string(), serde_json::Value::String(api_type.to_string()));
        entry.insert("apiKey".to_string(), serde_json::Value::String(provider.api_key.clone()));
        entry.insert("models".to_string(), serde_json::Value::Array(models));
        serde_json::Value::Object(entry)
    }
}

impl ProfileGenerator for PiGenerator {
    fn executor_name(&self) -> &str { "pi" }

    /// PI 的模型配置在 `agent/models.json`，非 `config.yaml`。
    /// 同时生成后会更新 `agent/settings.json` 的默认值。
    fn default_filename(&self) -> &str { "models.json" }

    /// 覆盖 config_path：PI 的模型定义在 ~/.pi/agent/models.json
    fn config_path(&self, session_dir: &str) -> std::path::PathBuf {
        // 使用 session_dir (通常是 ~/.pi) 拼接 agent/models.json
        let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        let expanded = if session_dir.starts_with('~') {
            let relative = session_dir.trim_start_matches('~').trim_start_matches(std::path::MAIN_SEPARATOR);
            home.join(relative)
        } else {
            std::path::PathBuf::from(session_dir)
        };
        expanded.join("agent").join("models.json")
    }

    fn preview(&self, exec_ref: &ExecutorRef, provider: &Provider, _session_dir: &str) -> Result<(String, String), String> {
        // models.json 路径
        let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        let models_path = home.join(".pi").join("agent").join("models.json");
        let settings_path = home.join(".pi").join("agent").join("settings.json");

        // 生成 provider 条目
        let provider_entry = Self::render_provider_entry(provider);
        let provider_name = exec_ref.provider.clone();

        // 读现有 models.json 或创建新结构
        let mut root: serde_json::Value = std::fs::read_to_string(&models_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| serde_json::json!({"providers": {}}));

        if let Some(providers) = root.get_mut("providers").and_then(|p| p.as_object_mut()) {
            providers.insert(provider_name.clone(), provider_entry);
        }

        // 构建设置更新
        let mut settings_update = serde_json::Map::new();
        settings_update.insert("defaultProvider".to_string(), serde_json::Value::String(provider_name));
        settings_update.insert("defaultModel".to_string(), serde_json::Value::String(exec_ref.model.clone()));

        let models_content = serde_json::to_string_pretty(&root).map_err(|e| format!("serialize models: {}", e))?;
        let settings_content = serde_json::to_string_pretty(&settings_update).map_err(|e| format!("serialize settings: {}", e))?;

        // 预览：显示 models.json 主要变更 + settings.json 变更
        let preview = format!(
            "=== {} ===\n{}\n\n=== {} ===\n{}",
            models_path.display(),
            models_content,
            settings_path.display(),
            settings_content,
        );

        Ok((models_path.to_string_lossy().to_string(), preview))
    }

    fn generate(&self, exec_ref: &ExecutorRef, provider: &Provider, _session_dir: &str) -> Result<(), String> {
        let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        let models_path = home.join(".pi").join("agent").join("models.json");
        let settings_path = home.join(".pi").join("agent").join("settings.json");

        // 备份
        if models_path.exists() { backup_existing_config(&models_path); }
        if settings_path.exists() { backup_existing_config(&settings_path); }

        // 确保 agent 目录存在
        if let Some(parent) = models_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("create dir: {}", e))?;
        }
        if let Some(parent) = settings_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("create dir: {}", e))?;
        }

        // 读取或创建 models.json
        let mut root: serde_json::Value = std::fs::read_to_string(&models_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| serde_json::json!({"providers": {}}));

        let provider_entry = Self::render_provider_entry(provider);
        let provider_name = exec_ref.provider.clone();

        if let Some(providers) = root.get_mut("providers").and_then(|p| p.as_object_mut()) {
            providers.insert(provider_name.clone(), provider_entry);
        }

        let models_json = serde_json::to_string_pretty(&root).map_err(|e| format!("serialize models: {}", e))?;
        std::fs::write(&models_path, &models_json).map_err(|e| format!("write models: {}", e))?;

        // 更新 settings.json 的默认值（保留原有其他字段）
        let mut settings: serde_json::Value = std::fs::read_to_string(&settings_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| serde_json::json!({}));

        if let Some(obj) = settings.as_object_mut() {
            obj.insert("defaultProvider".to_string(), serde_json::Value::String(provider_name));
            obj.insert("defaultModel".to_string(), serde_json::Value::String(exec_ref.model.clone()));
        }

        let settings_json = serde_json::to_string_pretty(&settings).map_err(|e| format!("serialize settings: {}", e))?;
        std::fs::write(&settings_path, &settings_json).map_err(|e| format!("write settings: {}", e))?;

        tracing::info!(models = %models_path.display(), settings = %settings_path.display(), "PI config generated");
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

impl AtomCodeGenerator {
    fn render_toml(exec_ref: &ExecutorRef, provider: &Provider) -> String {
        let mut output = String::new();
        output.push_str("\n# ntd profile provider — generated by profile apply\n");
        output.push_str("# Do not edit manually; will be overwritten.\n\n");
        let section_name = "ntd-profile";
        output.push_str(&format!("[providers.{}]\n", section_name));
        output.push_str("type = \"openai\"\n");
        output.push_str(&format!("model = {:?}\n", exec_ref.model));
        output.push_str(&format!("base_url = {:?}\n", provider.base_url));
        output.push_str(&format!("api_key = {:?}\n", provider.api_key));
        output
    }
}

impl ProfileGenerator for AtomCodeGenerator {
    fn executor_name(&self) -> &str { "atomcode" }
    fn default_filename(&self) -> &str { "config.toml" }

    fn preview(&self, exec_ref: &ExecutorRef, provider: &Provider, session_dir: &str) -> Result<(String, String), String> {
        let path = expand_config_path(self.config_path(session_dir));
        let content = Self::render_toml(exec_ref, provider);
        Ok((path.to_string_lossy().to_string(), content))
    }

    fn generate(&self, exec_ref: &ExecutorRef, provider: &Provider, session_dir: &str) -> Result<(), String> {
        let (path_str, content) = self.preview(exec_ref, provider, session_dir)?;
        let expanded_path = std::path::PathBuf::from(&path_str);
        if expanded_path.exists() {
            backup_existing_config(&expanded_path);
        }
        ensure_parent_dir(&expanded_path)?;
        // 追加到现有配置之后
        let existing = std::fs::read_to_string(&expanded_path).unwrap_or_default();
        let final_content = if existing.trim().is_empty() { content } else { format!("{}{}", existing.trim_end(), content) };
        std::fs::write(&expanded_path, &final_content).map_err(|e| format!("write: {}", e))?;
        tracing::info!(path = %path_str, "AtomCode config updated");
        Ok(())
    }
}

// ============================================================================
// Kilo Generator
// ============================================================================

/// Kilo → `~/.kilo/config.json`
pub struct KiloGenerator;

impl KiloGenerator {
    fn render_json(exec_ref: &ExecutorRef, provider: &Provider) -> Result<String, String> {
        let mut map = serde_json::Map::new();
        map.insert("api_key".to_string(), serde_json::Value::String(provider.api_key.clone()));
        map.insert("base_url".to_string(), serde_json::Value::String(provider.base_url.clone()));
        map.insert("model".to_string(), serde_json::Value::String(exec_ref.model.clone()));
        map.insert("provider_type".to_string(), serde_json::Value::String(
            if provider.protocol == Protocol::Anthropic { "anthropic" } else { "openai" }.to_string()
        ));
        serde_json::to_string_pretty(&map).map_err(|e| format!("serialize: {}", e))
    }
}

impl ProfileGenerator for KiloGenerator {
    fn executor_name(&self) -> &str { "kilo" }
    fn default_filename(&self) -> &str { "config.json" }

    fn preview(&self, exec_ref: &ExecutorRef, provider: &Provider, session_dir: &str) -> Result<(String, String), String> {
        let path = expand_config_path(self.config_path(session_dir));
        let content = Self::render_json(exec_ref, provider)?;
        Ok((path.to_string_lossy().to_string(), content))
    }

    fn generate(&self, exec_ref: &ExecutorRef, provider: &Provider, session_dir: &str) -> Result<(), String> {
        let (path_str, content) = self.preview(exec_ref, provider, session_dir)?;
        let expanded_path = std::path::PathBuf::from(&path_str);
        backup_existing_config(&expanded_path);
        ensure_parent_dir(&expanded_path)?;
        std::fs::write(&expanded_path, &content).map_err(|e| format!("write: {}", e))?;
        tracing::info!(path = %path_str, "Kilo config generated");
        Ok(())
    }
}

/// 执行器配置定义：名称、显示名、配置文件路径、是否有生成器。
#[derive(Debug, Clone, Serialize)]
pub struct ExecutorConfigDef {
    pub name: String,
    pub display_name: String,
    pub config_path: String,
    pub has_generator: bool,
}

/// 返回所有执行器的配置定义列表（含不支持生成器的执行器）。
/// 信息来自 adapters/mod.rs 的 EXECUTORS 静态数组 + ProfileGeneratorRegistry。
pub fn all_executor_configs() -> Vec<ExecutorConfigDef> {
    let registry = ProfileGeneratorRegistry::new();
    crate::adapters::EXECUTORS.iter().map(|def| {
        // 构建配置文件路径：session_dir + 生成器的 default_filename（如有）
        let config_path = if let Some(gen) = registry.get(def.name) {
            gen.config_path(def.session_dir).to_string_lossy().to_string()
        } else {
            // 无生成器时，从 session_dir 推测常见配置文件名
            format!("{}/config.json", def.session_dir.trim_end_matches('/'))
        };
        ExecutorConfigDef {
            name: def.name.to_string(),
            display_name: def.display_name.to_string(),
            config_path,
            has_generator: registry.get(def.name).is_some(),
        }
    }).collect()
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
            models: vec![
                super::super::ProviderModel { name: "gpt-4o".to_string(), display_name: Some("GPT-4o".to_string()), supports_1m_context: true },
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
        assert_eq!(gen.default_filename(), "models.json");
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
    fn test_backup_creates_bak_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("settings.json");
        std::fs::write(&config_path, "original content").unwrap();
        backup_existing_config(&config_path);
        // 确认备份文件存在（匹配 `settings.json.bak-*`）
        let entries: Vec<_> = std::fs::read_dir(tmp.path()).unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert!(entries.iter().any(|n| n.starts_with("settings.json.bak-")),
            "应有 settings.json.bak-<timestamp>, 实际: {:?}", entries);
        // 原文件保留
        assert!(entries.iter().any(|n| n == "settings.json"));
    }
}
