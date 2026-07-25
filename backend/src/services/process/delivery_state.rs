//! `.ntd/delivery-state.md` 维护。
//!
//! 工艺实例全局状态文件，记录当前阶段、产物清单、门禁结果。
//! 仅维护文件内容，不参与运行时逻辑。

use std::path::Path;

use tokio::io::AsyncWriteExt;

/// delivery-state 文件写入错误。
#[derive(Debug, thiserror::Error)]
pub enum DeliveryStateError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialize 错误: {0}")]
    Serialize(String),
}

/// 写入 delivery-state.md。
///
/// # 参数
/// - `workspace_path`：工作空间根目录。
/// - `process_name`：工艺名称。
/// - `process_version`：工艺版本。
/// - `loop_id`：Loop ID。
/// - `current_phase`：当前阶段名。
/// - `current_step`：当前环节名。
/// - `artifacts`：已捕获产物摘要（`{name} -> {locator} -> {status}`）。
/// - `gates`：门禁结果摘要（`{step_name} -> {gate_name} -> {status} -> {score?}`）。
#[allow(clippy::too_many_arguments)]
pub async fn write_delivery_state(
    workspace_path: &str,
    process_name: Option<&str>,
    process_version: Option<&str>,
    loop_id: i64,
    current_phase: &str,
    current_step: &str,
    artifacts: &[ArtifactEntry],
    gates: &[GateEntry],
) -> Result<(), DeliveryStateError> {
    // 确保 `.ntd/` 目录存在。
    let dir = Path::new(workspace_path).join(".ntd");
    tokio::fs::create_dir_all(&dir).await?;

    let mut content = String::new();
    content.push_str("# Delivery State\n\n");

    // 元信息。
    content.push_str(&format!("- Loop ID: {}\n", loop_id));
    if let Some(pn) = process_name {
        content.push_str(&format!("- 工艺: {}\n", pn));
    }
    if let Some(pv) = process_version {
        content.push_str(&format!("- 版本: {}\n", pv));
    }
    content.push_str(&format!("- 当前阶段: {}\n", current_phase));
    content.push_str(&format!("- 当前环节: {}\n\n", current_step));

    // 产物清单。
    if !artifacts.is_empty() {
        content.push_str("## 产物清单\n\n");
        content.push_str("| 名称 | 路径 | 状态 |\n");
        content.push_str("|------|------|------|\n");
        for a in artifacts {
            content.push_str(&format!("| {} | {} | {} |\n", a.name, a.locator, a.status));
        }
        content.push('\n');
    }

    // 门禁结果。
    if !gates.is_empty() {
        content.push_str("## 门禁结果\n\n");
        content.push_str("| 环节 | 门禁 | 类型 | 状态 | 评分 |\n");
        content.push_str("|------|------|------|------|------|\n");
        for g in gates {
            let score_str = g
                .score
                .map(|s| s.to_string())
                .unwrap_or_else(|| "-".to_string());
            content.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                g.step_name, g.gate_name, g.gate_type, g.status, score_str
            ));
        }
        content.push('\n');
    }

    let filepath = dir.join("delivery-state.md");
    let mut file = tokio::fs::File::create(&filepath).await?;
    file.write_all(content.as_bytes()).await?;

    Ok(())
}

/// 产物摘要条目，用于生成 delivery-state 中的产物清单表格。
#[derive(Debug, Clone)]
pub struct ArtifactEntry {
    pub name: String,
    pub locator: String,
    pub status: String,
}

/// 门禁摘要条目，用于生成 delivery-state 中的门禁结果表格。
#[derive(Debug, Clone)]
pub struct GateEntry {
    pub step_name: String,
    pub gate_name: String,
    pub gate_type: String,
    pub status: String,
    pub score: Option<i32>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_write_delivery_state_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().to_str().unwrap();

        let artifacts = vec![
            ArtifactEntry {
                name: "PRD".to_string(),
                locator: "docs/PRD.md".to_string(),
                status: "✅ 已捕获".to_string(),
            },
        ];
        let gates = vec![
            GateEntry {
                step_name: "生成 PRD".to_string(),
                gate_name: "PRD 存在".to_string(),
                gate_type: "artifact_present".to_string(),
                status: "passed".to_string(),
                score: None,
            },
        ];

        write_delivery_state(
            ws,
            Some("标准交付"),
            Some("1.0.0"),
            42,
            "需求",
            "生成 PRD",
            &artifacts,
            &gates,
        )
        .await
        .unwrap();

        let filepath = tmp.path().join(".ntd/delivery-state.md");
        assert!(filepath.exists(), "delivery-state.md should exist");

        let content = std::fs::read_to_string(&filepath).unwrap();
        assert!(content.contains("标准交付"), "should contain process name");
        assert!(content.contains("✅ 已捕获"), "should contain artifact status");
        assert!(content.contains("passed"), "should contain gate status");
    }
}
