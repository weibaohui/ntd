//! 执行器模型列表：调各执行器的 `models` 子命令，解析可选模型。
//!
//! 用于执行器页「默认模型」字段的下拉选项。支持 pi（`pi --list-models` 表格）、
//! mimo/opencode/kilo（`models` 子命令，每行一个）。其余执行器不支持，前端手填。
//! 结果带 TTL 缓存，避免每次展开下拉都 spawn 子命令。

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use parking_lot::RwLock;

use crate::models::ExecutorType;

/// 模型列表缓存 TTL：5 分钟。执行器升级后最多 5 分钟自动失效；
/// 执行器路径变更时 cache key 含 path，会立即失效。
const MODELS_CACHE_TTL: Duration = Duration::from_secs(300);

/// 该执行器是否支持通过 models 子命令列模型。
///
/// 和 `list_models` 的 match 分支同源，是前端「Select(有选项) vs Input(手填)」的
/// 单一事实来源——避免前端再硬编码一份执行器名单导致漂移。
pub fn supports_models(et: ExecutorType) -> bool {
    matches!(
        et,
        ExecutorType::Pi | ExecutorType::Mimo | ExecutorType::Opencode | ExecutorType::Kilo
    )
}

/// 按执行器类型调 models 子命令并解析返回可选模型列表。
///
/// - 不支持的类型（supports_models=false）返回空 vec（前端手填兜底）；
/// - 失败（二进制缺失 / 超时 / 解析失败）静默返回空，不阻断前端——
///   模型列表是「增强」，拉不到就退回手填，不应让整个执行器页报错；
/// - 结果带 TTL 缓存（key 含 path），避免反复 spawn 子命令。
pub async fn list_models(et: ExecutorType, exec_path: &str) -> Vec<String> {
    if !supports_models(et) {
        return vec![];
    }
    let key = (et, exec_path.to_string());
    // 先查缓存：未过期直接返回，避免每次展开下拉都 spawn 子命令（可能查网络）。
    if let Some((at, models)) = models_cache().read().get(&key) {
        if at.elapsed() < MODELS_CACHE_TTL {
            return models.clone();
        }
    }
    // 每个执行器的 models 子命令 args。
    let args: &[&str] = match et {
        ExecutorType::Pi => &["--list-models"],
        ExecutorType::Mimo | ExecutorType::Opencode | ExecutorType::Kilo => &["models"],
        _ => return vec![],
    };
    // models 子命令可能查网络，设 15s 超时避免卡住前端请求。
    let output = tokio::time::timeout(
        Duration::from_secs(15),
        tokio::process::Command::new(exec_path).args(args).output(),
    )
    .await;
    // 超时或 spawn 失败都视作「拉不到」，返回空。
    let Ok(Ok(out)) = output else {
        return vec![];
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    // pi 输出是表格需特殊解析；mimo/opencode/kilo 每行一个模型。
    let models = match et {
        ExecutorType::Pi => parse_pi_models(&stdout),
        ExecutorType::Mimo | ExecutorType::Opencode | ExecutorType::Kilo => parse_simple_lines(&stdout),
        _ => vec![],
    };
    // 写缓存（即使空也缓存，避免反复跑失败的子命令；TTL 后失效）。
    models_cache()
        .write()
        .insert(key, (Instant::now(), models.clone()));
    models
}

/// 模型列表缓存条目：(拉取时刻, 模型列表)。抽成 type alias 避免 clippy::type_complexity。
type ModelsCacheMap = HashMap<(ExecutorType, String), (Instant, Vec<String>)>;

/// 进程级模型列表缓存（TTL 见 `MODELS_CACHE_TTL`）。OnceLock 保证懒初始化。
fn models_cache() -> &'static RwLock<ModelsCacheMap> {
    static CACHE: OnceLock<RwLock<ModelsCacheMap>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// 解析 `pi --list-models` 的空格对齐表格：跳过表头，取每行组合 `provider/model`。
///
/// pi 输出形如：
/// ```text
/// provider           model                              context
/// agnes-ai           agnes-2.0-flash                    512K
/// minimax-anthropic  MiniMax-M3                         1M
/// jiutian            deepseek/deepseek-v4-flash         128K
/// ```
/// pi 的 `--model` 参数接受 `provider/model` 格式，因此返回 `{provider}/{model}`。
/// model 列本身可能含 `/`（如 `deepseek/deepseek-v4-flash`），组合为
/// `jiutian/deepseek/deepseek-v4-flash`——pi 实测接受此完整格式。
fn parse_pi_models(output: &str) -> Vec<String> {
    output
        .lines()
        .skip(1) // 跳表头
        .filter_map(|line| {
            let cols: Vec<&str> = line.split_whitespace().collect();
            // 每行至少要有 provider 和 model 两列
            let provider = cols.first()?;
            let model = cols.get(1)?;
            Some(format!("{}/{}", provider, model))
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// 解析每行一个模型的输出（mimo/opencode/kilo 的 `models` 子命令）。
///
/// 这些执行器的 `models` 子命令直接输出每行一个 `provider/model` 标识符，
/// 没有表头或其他元信息。过滤空行、trim 后返回。
fn parse_simple_lines(output: &str) -> Vec<String> {
    output
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_models_only_for_known_executors() {
        // 已适配 models 子命令的执行器
        assert!(supports_models(ExecutorType::Pi));
        assert!(supports_models(ExecutorType::Mimo));
        assert!(supports_models(ExecutorType::Opencode));
        assert!(supports_models(ExecutorType::Kilo));
        // 未适配的执行器
        assert!(!supports_models(ExecutorType::Claudecode));
        assert!(!supports_models(ExecutorType::Codex));
    }

    #[test]
    fn parse_pi_models_combines_provider_and_model() {
        let out = "provider           model                              context\nagnes-ai           agnes-2.0-flash                    512K\nminimax-anthropic  MiniMax-M3                         1M\njiutian            deepseek/deepseek-v4-flash         128K";
        assert_eq!(
            parse_pi_models(out),
            vec![
                "agnes-ai/agnes-2.0-flash",
                "minimax-anthropic/MiniMax-M3",
                "jiutian/deepseek/deepseek-v4-flash"
            ]
        );
    }

    #[test]
    fn parse_pi_models_empty_when_no_data_rows() {
        assert!(parse_pi_models("provider model context").is_empty());
        assert!(parse_pi_models("").is_empty());
    }

    #[test]
    fn parse_pi_models_skips_blank_lines() {
        let out = "provider model\nminimax-anthropic MiniMax-M3\n\nagnes-ai agnes-2.0-flash";
        assert_eq!(
            parse_pi_models(out),
            vec!["minimax-anthropic/MiniMax-M3", "agnes-ai/agnes-2.0-flash"]
        );
    }

    #[test]
    fn parse_simple_lines_each_line_is_a_model() {
        let out = "mimo/mimo-auto\nxiaomi/mimo-v2.5-pro\nzai/glm-5";
        assert_eq!(
            parse_simple_lines(out),
            vec!["mimo/mimo-auto", "xiaomi/mimo-v2.5-pro", "zai/glm-5"]
        );
    }

    #[test]
    fn parse_simple_lines_empty_when_no_output() {
        assert!(parse_simple_lines("").is_empty());
    }

    #[test]
    fn parse_simple_lines_skips_blank_lines() {
        let out = "mimo/mimo-auto\n\nzai/glm-5\n\nxiaomi/mimo-v2.5";
        assert_eq!(
            parse_simple_lines(out),
            vec!["mimo/mimo-auto", "zai/glm-5", "xiaomi/mimo-v2.5"]
        );
    }
}
