//! 工艺产物捕获。
//!
//! 环节执行完成后，根据 `loop_steps.expected_artifacts` 配置自动捕获产物：
//! - `file`：检查 workspace 相对路径是否存在，记录路径快照；
//! - `text`：从 execution_record.result 中按 marker 提取；
//! - `url`：从结果中正则匹配 URL；
//! - `json`：从结果中提取 JSON 块。
//!
//! 所有文件类路径必须经 `resolve_within_workspace` 校验，禁止目录遍历。

use std::path::{Path, PathBuf};

use crate::db::entity::loop_step_artifacts;
use crate::db::Database;
use crate::models::ExecutionRecord;

/// 产物捕获错误。
#[derive(Debug, thiserror::Error)]
pub enum ArtifactCaptureError {
    #[error("路径逃逸或非法路径: {0}")]
    InvalidPath(String),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("数据库错误: {0}")]
    Db(#[from] sea_orm::DbErr),
    #[error("JSON 解析错误: {0}")]
    Json(String),
}

/// 期望产物定义（与 `services/process::ExpectedArtifact` 同构，但独立拷贝避免模块循环）。
#[derive(Debug, Clone)]
pub struct ArtifactSpec {
    pub name: String,
    pub artifact_type: String,
    pub locator: String,
}

impl ArtifactSpec {
    /// 从 installer.rs 的 `ExpectedArtifact` 构造。
    pub fn from_expected(value: &crate::services::process::ExpectedArtifact) -> Self {
        Self {
            name: value.name.clone(),
            artifact_type: value.artifact_type.clone(),
            locator: value.locator_string(),
        }
    }
}

/// 产物捕获结果。
#[derive(Debug, Clone)]
pub struct CapturedArtifact {
    pub name: String,
    pub artifact_type: String,
    pub locator: String,
    pub content_text: Option<String>,
}

/// 捕获一次环节执行的所有产物并写入数据库。
///
/// `execution_record` 为环节执行结果，用于提取 text/url/json 类产物。
/// `captured_by` 建议传 execution_record_id 字符串，或 "manual"。
pub async fn capture_step_artifacts(
    db: &Database,
    loop_step_execution_id: i64,
    workspace_path: &str,
    specs: &[ArtifactSpec],
    execution_record: Option<&ExecutionRecord>,
    captured_by: Option<&str>,
) -> Result<Vec<loop_step_artifacts::Model>, ArtifactCaptureError> {
    let mut models = Vec::with_capacity(specs.len());

    for spec in specs {
        if let Some(captured) = capture_one_artifact(workspace_path, spec, execution_record).await? {
            let model = db
                .create_loop_step_artifact(
                    loop_step_execution_id,
                    &captured.name, &captured.artifact_type, &captured.locator,
                    captured.content_text.as_deref(), captured_by,
                )
                .await?;
            models.push(model);
        }
    }

    Ok(models)
}

/// 捕获单个产物。file 类型不存在时返回 Ok(None)，由 artifact_present 门禁检测缺失。
async fn capture_one_artifact(
    workspace_path: &str,
    spec: &ArtifactSpec,
    execution_record: Option<&ExecutionRecord>,
) -> Result<Option<CapturedArtifact>, ArtifactCaptureError> {
    match spec.artifact_type.as_str() {
        "file" => capture_file_artifact(workspace_path, spec).await,
        "text" => Ok(Some(capture_text_artifact(spec, execution_record)?)),
        "url" => Ok(Some(capture_url_artifact(spec, execution_record)?)),
        "json" => Ok(Some(capture_json_artifact(spec, execution_record)?)),
        "delivery-state" | "repair-log" => Ok(Some(CapturedArtifact {
            name: spec.name.clone(), artifact_type: spec.artifact_type.clone(),
            locator: spec.locator.clone(), content_text: None,
        })),
        // 未知类型按 text 兜底。
        _ => Ok(Some(capture_text_artifact(spec, execution_record)?)),
    }
}

/// 捕获文件类产物。只校验路径和存在性，不存内容——内容等用户点击时从磁盘读取。
/// 文件不存在时返回 None（跳过此产物，由 artifact_present 门禁检测缺失）。
async fn capture_file_artifact(
    workspace_path: &str,
    spec: &ArtifactSpec,
) -> Result<Option<CapturedArtifact>, ArtifactCaptureError> {
    let resolved = resolve_within_workspace(workspace_path, &spec.locator)?;
    if !resolved.exists() {
        return Ok(None);
    }
    Ok(Some(CapturedArtifact {
        name: spec.name.clone(),
        artifact_type: spec.artifact_type.clone(),
        locator: spec.locator.clone(),
        content_text: None,
    }))
}

/// 安全读取文件，限制最大字节数（当前 file 产物不存快照，保留备用）。
#[allow(dead_code)]
async fn read_limited(path: &Path, max_bytes: usize) -> Result<String, ArtifactCaptureError> {
    use tokio::io::AsyncReadExt;

    let mut file = tokio::fs::File::open(path).await?;
    let mut buf = vec![0u8; max_bytes];
    let n = file.read(&mut buf).await?;
    buf.truncate(n);
    String::from_utf8(buf).map_err(|e| ArtifactCaptureError::Json(format!("文件非 UTF-8: {}", e)))
}

/// 从 execution_record.result 中按 marker 提取文本产物。
fn capture_text_artifact(
    spec: &ArtifactSpec,
    execution_record: Option<&ExecutionRecord>,
) -> Result<CapturedArtifact, ArtifactCaptureError> {
    let output = execution_record
        .and_then(|r| r.result.as_deref())
        .unwrap_or("");
    let content = extract_after_marker(output, &spec.locator);

    Ok(CapturedArtifact {
        name: spec.name.clone(),
        artifact_type: spec.artifact_type.clone(),
        locator: spec.locator.clone(),
        content_text: content,
    })
}

/// 从 execution_record.result 中提取 URL。
fn capture_url_artifact(
    spec: &ArtifactSpec,
    execution_record: Option<&ExecutionRecord>,
) -> Result<CapturedArtifact, ArtifactCaptureError> {
    let output = execution_record
        .and_then(|r| r.result.as_deref())
        .unwrap_or("");
    let url = extract_first_url(output);

    Ok(CapturedArtifact {
        name: spec.name.clone(),
        artifact_type: spec.artifact_type.clone(),
        locator: url.clone().unwrap_or_else(|| spec.locator.clone()),
        content_text: url,
    })
}

/// 从 execution_record.result 中提取 JSON 块。
fn capture_json_artifact(
    spec: &ArtifactSpec,
    execution_record: Option<&ExecutionRecord>,
) -> Result<CapturedArtifact, ArtifactCaptureError> {
    let output = execution_record
        .and_then(|r| r.result.as_deref())
        .unwrap_or("");
    let json = extract_json_block(output);

    Ok(CapturedArtifact {
        name: spec.name.clone(),
        artifact_type: spec.artifact_type.clone(),
        locator: spec.locator.clone(),
        content_text: json,
    })
}

/// 解析相对路径，确保其位于 workspace 目录内。
///
/// 防御：
/// - 绝对路径直接拒；
/// - 含 `..` 父级引用直接拒；
/// - 拼接后经 `std::fs::canonicalize` 校验是否仍在 workspace 下（canonicalize 失败则回退到字符串级校验）。
fn resolve_within_workspace(workspace_path: &str, rel_path: &str) -> Result<PathBuf, ArtifactCaptureError> {
    let rel = Path::new(rel_path);

    // 字符串级校验：挡住绝对路径、父级引用、前缀。
    if rel.is_absolute() {
        return Err(ArtifactCaptureError::InvalidPath(format!(
            "absolute path not allowed: {}",
            rel_path
        )));
    }
    if rel.components().any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::Prefix(_))) {
        return Err(ArtifactCaptureError::InvalidPath(format!(
            "parent directory traversal not allowed: {}",
            rel_path
        )));
    }
    if rel.as_os_str().is_empty() {
        return Err(ArtifactCaptureError::InvalidPath("empty path".to_string()));
    }

    let base = Path::new(workspace_path);
    let candidate = base.join(rel);

    // IO 级兜底：canonicalize 能拦住符号链接绕过。
    // 若文件尚不存在（artifact_present 会失败），canonicalize 会报错，此时回退到字符串级前缀比较。
    match base.canonicalize() {
        Ok(base_canonical) => {
            // 文件可能尚不存在，先基于 base_canonical 拼接出绝对候选路径。
            let absolute_candidate = base_canonical.join(rel);
            match absolute_candidate.canonicalize() {
                Ok(candidate_canonical) => {
                    if !candidate_canonical.starts_with(&base_canonical) {
                        return Err(ArtifactCaptureError::InvalidPath(format!(
                            "path escapes workspace: {}",
                            rel_path
                        )));
                    }
                    Ok(candidate_canonical)
                }
                Err(_) => {
                    // 文件不存在时，用 normalize 后的绝对路径兜底校验。
                    let normalized = normalize_path(&absolute_candidate);
                    if !normalized.starts_with(&base_canonical) {
                        return Err(ArtifactCaptureError::InvalidPath(format!(
                            "path escapes workspace: {}",
                            rel_path
                        )));
                    }
                    Ok(normalized)
                }
            }
        }
        Err(_) => {
            // workspace 本身不存在时，只能依赖字符串级校验。
            Ok(candidate)
        }
    }
}

/// 简单的路径 normalize：保留 root 前缀，解析 `.` 和 `..`，不访问文件系统。
fn normalize_path(path: &Path) -> PathBuf {
    let mut root = PathBuf::new();
    let mut normal_parts: Vec<PathBuf> = Vec::new();
    let comps = path.components().collect::<Vec<_>>();

    for c in comps {
        match c {
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                root.push(c.as_os_str());
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normal_parts.pop();
            }
            std::path::Component::Normal(p) => {
                normal_parts.push(PathBuf::from(p));
            }
        }
    }

    let mut result = root;
    for p in normal_parts {
        result.push(p);
    }
    result
}

/// 从文本中提取 marker 之后、下一个空行/换行之前的内容。
fn extract_after_marker(text: &str, marker: &str) -> Option<String> {
    let pos = text.find(marker)?;
    let after = &text[pos + marker.len()..];
    let end = after.find('\n').unwrap_or(after.len());
    let slice = after[..end].trim();
    if slice.is_empty() {
        None
    } else {
        Some(slice.to_string())
    }
}

/// 用简单正则提取第一个 http/https URL。
fn extract_first_url(text: &str) -> Option<String> {
    // 局部 use 而非顶部导入：LazyLock 仅此一处使用，就近声明让「静态缓存只服务
    // 本函数」的关系一目了然，也不给模块顶部 import 表添噪音
    use std::sync::LazyLock;
    // 095-3：LazyLock 进程级一次编译，替代每次调用现编译（同款范式见
    // adapters/mod.rs THINK_RE）。正则模式是编译期常量，unwrap 不可达；
    // 原实现的 `.ok()?` 在 LazyLock 下无意义（失败即进程不可用，panic 更诚实）。
    static URL_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
        #[allow(clippy::unwrap_used)]
        // 简易 URL 正则：http(s):// 后跟非空白字符，直到遇到引号、括号或空白。
        regex::Regex::new(r#"https?://[^\s"'()\]\}>]+"#).unwrap()
    });
    // find 取首个匹配即返回：调用方只需要「文本里有没有可打开的链接」，
    // 全量 find_iter 是浪费；map 转 owned String 脱离 text 借用生命周期
    URL_RE.find(text).map(|m| m.as_str().to_string())
}

/// 从文本中提取 ```json ... ``` 代码块，或第一个顶层 JSON 对象/数组。
fn extract_json_block(text: &str) -> Option<String> {
    // 先尝试 Markdown JSON 代码块。
    if let Some(block) = extract_markdown_json_block(text) {
        return Some(block);
    }
    // 回退：找第一个 `{` 或 `[`，做括号匹配后校验为合法 JSON。
    find_and_validate_json(text)
}

/// 提取 Markdown ```json``` 代码块内容。
fn extract_markdown_json_block(text: &str) -> Option<String> {
    let start = text.find("```json")?;
    // "```json" 长度为 7，跳过后再 trim 开头换行。
    let after = &text[start + 7..].trim_start();
    let end = after.find("```")?;
    let block = after[..end].trim();
    if block.is_empty() { None } else { Some(block.to_string()) }
}

/// 从文本中找到第一个 `{` 或 `[`，做括号匹配，校验为合法 JSON。
fn find_and_validate_json(text: &str) -> Option<String> {
    let start = text.find('{').or_else(|| text.find('['))?;
    let chars: Vec<char> = text[start..].chars().collect();
    let end = find_json_end_index(&chars)?;
    let block = text[start..start + end + 1].trim();
    if serde_json::from_str::<serde_json::Value>(block).is_ok() {
        Some(block.to_string())
    } else {
        None
    }
}

/// 括号匹配：返回嵌套 JSON 块结束索引，处理字符串内转义。
fn find_json_end_index(chars: &[char]) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;

    for (i, c) in chars.iter().enumerate() {
        if in_string {
            if escape { escape = false; }
            else if *c == '\\' { escape = true; }
            else if *c == '"' { in_string = false; }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' | '[' => depth += 1,
            '}' | ']' => { depth -= 1; if depth == 0 { return Some(i); } }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::bool_assert_comparison,
    clippy::too_many_arguments
)]
mod tests {
    use super::*;
    use crate::models::ExecutionRecord;

    fn make_record(result: &str) -> ExecutionRecord {
        ExecutionRecord {
            id: 1,
            todo_id: 1,
            status: crate::models::ExecutionStatus::Success,
            command: String::new(),
            stdout: String::new(),
            stderr: String::new(),
            result: Some(result.to_string()),
            started_at: String::new(),
            finished_at: None,
            usage: None,
            executor: None,
            model: None,
            trigger_type: "manual".to_string(),
            pid: None,
            task_id: None,
            session_id: None,
            todo_progress: None,
            agent_runs: None,
            execution_stats: None,
            workspace_id: None, // v89 新增字段：测试 make_record 不关心归属
            resume_message: None,
            source_todo_id: None,
            source_todo_title: None,
            rating: None,
            source_execution_record_id: None,
            last_review_status: None,
            last_reviewed_at: None,
            worktree_path: None,
            loop_step_execution_id: None,
            step_id: None,
        }
    }

    #[test]
    fn test_extract_after_marker() {
        let text = "## 结论\n这是结论内容\n下一行";
        assert_eq!(
            extract_after_marker(text, "## 结论\n"),
            Some("这是结论内容".to_string())
        );
    }

    #[test]
    fn test_extract_first_url() {
        let text = "查看 https://example.com/path 获取详情";
        assert_eq!(
            extract_first_url(text),
            Some("https://example.com/path".to_string())
        );
    }

    #[test]
    fn test_extract_json_block_markdown() {
        let text = "结果如下\n```json\n{\"ok\": true}\n```";
        assert_eq!(
            extract_json_block(text),
            Some(r#"{"ok": true}"#.to_string())
        );
    }

    #[test]
    fn test_extract_json_block_raw() {
        let text = "结果如下\n{\"ok\": true, \"list\": [1, 2]}";
        assert_eq!(
            extract_json_block(text),
            Some(r#"{"ok": true, "list": [1, 2]}"#.to_string())
        );
    }

    #[test]
    fn test_resolve_within_workspace_rejects_absolute() {
        let err = resolve_within_workspace("/tmp/ws", "/etc/passwd").unwrap_err();
        assert!(err.to_string().contains("absolute"));
    }

    #[test]
    fn test_resolve_within_workspace_rejects_parent_traversal() {
        let err = resolve_within_workspace("/tmp/ws", "../etc/passwd").unwrap_err();
        assert!(err.to_string().contains("parent"));
    }

    #[tokio::test]
    async fn test_capture_file_artifact_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().to_str().unwrap();
        let file_path = tmp.path().join("docs/PRD.md");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, "# PRD\n").unwrap();

        let spec = ArtifactSpec {
            name: "PRD".to_string(),
            artifact_type: "file".to_string(),
            locator: "docs/PRD.md".to_string(),
        };

        let captured = capture_file_artifact(ws, &spec).await.unwrap();
        // 文件存在时返回 Some，不存在时返回 None。
        assert!(captured.is_some());
        assert_eq!(captured.unwrap().content_text, None);
    }

    #[tokio::test]
    async fn test_capture_file_artifact_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().to_str().unwrap();

        let spec = ArtifactSpec {
            name: "PRD".to_string(),
            artifact_type: "file".to_string(),
            locator: "docs/PRD.md".to_string(),
        };

        let captured = capture_file_artifact(ws, &spec).await.unwrap();
        assert!(captured.is_none());
    }

    #[tokio::test]
    async fn test_capture_text_artifact() {
        let record = make_record("## 产物: PRD\nPRD 内容");
        let spec = ArtifactSpec {
            name: "PRD".to_string(),
            artifact_type: "text".to_string(),
            // marker 包含 heading 自身及换行，表示提取 heading 之后的内容。
            locator: "## 产物: PRD\n".to_string(),
        };

        let captured = capture_text_artifact(&spec,
            Some(&record),
        )
        .unwrap();
        assert_eq!(captured.content_text, Some("PRD 内容".to_string()));
    }

    #[tokio::test]
    async fn test_capture_url_artifact() {
        let record = make_record("结果见 https://example.com/report");
        let spec = ArtifactSpec {
            name: "Report".to_string(),
            artifact_type: "url".to_string(),
            locator: String::new(),
        };

        let captured = capture_url_artifact(&spec,
            Some(&record),
        )
        .unwrap();
        assert_eq!(captured.content_text, Some("https://example.com/report".to_string()));
    }

    #[tokio::test]
    async fn test_capture_json_artifact() {
        let record = make_record("```json\n{\"ok\": true}\n```");
        let spec = ArtifactSpec {
            name: "Result".to_string(),
            artifact_type: "json".to_string(),
            locator: String::new(),
        };

        let captured = capture_json_artifact(&spec,
            Some(&record),
        )
        .unwrap();
        assert_eq!(captured.content_text, Some(r#"{"ok": true}"#.to_string()));
    }

    #[tokio::test]
    async fn test_capture_step_artifacts_end_to_end() {
        let db = Database::new(":memory:").await.unwrap();

        // 插入最小 FK 依赖行。
        db.exec("INSERT INTO todos (id, title, prompt, status) VALUES (1, 'test', 'prompt', 'pending')").await.unwrap();
        db.exec("INSERT INTO loops (id, name) VALUES (1, 'test-loop')").await.unwrap();
        db.exec("INSERT INTO loop_steps (id, loop_id, name, todo_id) VALUES (1, 1, 'step1', 1)").await.unwrap();
        db.exec("INSERT INTO loop_executions (id, loop_id, trigger_type, started_at, status) VALUES (1, 1, 'manual', '2024-01-01', 'running')").await.unwrap();
        db.exec("INSERT INTO loop_step_executions (id, loop_execution_id, step_id, todo_id, status) VALUES (1, 1, 1, 1, 'running')").await.unwrap();

        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().to_str().unwrap();

        let file_path = tmp.path().join("docs/PRD.md");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, "# PRD\n").unwrap();

        let record = make_record("结果见 https://example.com/report");
        let specs = vec![
            ArtifactSpec {
                name: "PRD".to_string(),
                artifact_type: "file".to_string(),
                locator: "docs/PRD.md".to_string(),
            },
            ArtifactSpec {
                name: "Report".to_string(),
                artifact_type: "url".to_string(),
                locator: String::new(),
            },
        ];

        let models = capture_step_artifacts(
            &db,
            1,  // loop_step_execution_id
            ws,
            &specs,
            Some(&record),
            Some("100"),
        )
        .await
        .unwrap();

        assert_eq!(models.len(), 2);
        assert_eq!(models[0].name, "PRD");
        assert_eq!(models[0].artifact_type, "file");
        assert_eq!(models[1].name, "Report");
        assert_eq!(models[1].content_text, Some("https://example.com/report".to_string()));
    }
}
