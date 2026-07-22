//! 执行器模型列表：调各执行器的 `models` 子命令，解析可选模型。
//!
//! 用于执行器页「默认模型」字段的下拉选项。MVP 支持 pi（`pi --list-models` 表格）；
//! 其余执行器返回空，前端降级为手填。后续按执行器在 `list_models` 的 match 里追加。

use std::time::Duration;

use crate::models::ExecutorType;

/// 按执行器类型调 models 子命令并解析返回可选模型列表。
///
/// - 不支持的类型返回空 vec（前端手填兜底）；
/// - 失败（二进制缺失 / 超时 / 解析失败）静默返回空，不阻断前端——
///   模型列表是「增强」，拉不到就退回手填，不应让整个执行器页报错。
pub async fn list_models(et: ExecutorType, exec_path: &str) -> Vec<String> {
    // 每个执行器的 models 子命令 args；未列入的返回空。
    let args: &[&str] = match et {
        ExecutorType::Pi => &["--list-models"],
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
    parse_pi_models(&String::from_utf8_lossy(&out.stdout))
}

/// 解析 `pi --list-models` 的空格对齐表格：跳过表头，取每行第 2 列（model）。
///
/// pi 输出形如：
/// ```text
/// provider           model                              context  ...
/// agnes-ai           agnes-2.0-flash                    512K     ...
/// minimax-anthropic  MiniMax-M3                         1M       ...
/// ```
/// 第 2 列即 model 标识（可能含 `/`，如 `deepseek/deepseek-v4-flash`）。
fn parse_pi_models(output: &str) -> Vec<String> {
    output
        .lines()
        .skip(1) // 跳表头
        .filter_map(|line| line.split_whitespace().nth(1).map(str::to_string))
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pi_models_extracts_model_column() {
        let out = "provider           model                              context\n\
                   agnes-ai           agnes-2.0-flash                    512K\n\
                   minimax-anthropic  MiniMax-M3                         1M\n\
                   jiutian            deepseek/deepseek-v4-flash         128K";
        assert_eq!(
            parse_pi_models(out),
            vec!["agnes-2.0-flash", "MiniMax-M3", "deepseek/deepseek-v4-flash"]
        );
    }

    /// 只有表头或空输入时返回空（不 panic、不含表头残留）。
    #[test]
    fn parse_pi_models_empty_when_no_data_rows() {
        assert!(parse_pi_models("provider model context").is_empty());
        assert!(parse_pi_models("").is_empty());
    }

    /// 跳过空行，避免表格里的空行产生空字符串项。
    #[test]
    fn parse_pi_models_skips_blank_lines() {
        let out = "provider model\nminimax-anthropic MiniMax-M3\n\nagnes-ai agnes-2.0-flash";
        assert_eq!(parse_pi_models(out), vec!["MiniMax-M3", "agnes-2.0-flash"]);
    }
}
