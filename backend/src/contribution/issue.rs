//! 读取本地专家目录，打包为 PR 提交内容：文件清单 + 简短 PR 描述。
//!
//! PR 方案下专家以真实文件提交（含二进制头像等），正文只放简短的元信息表格与文件清单。

use std::path::Path;

use crate::expert::{ExpertMetadata, ExpertType};

/// 贡献草稿（预览用）：标题 + 简短描述 + 文件清单。
#[derive(Debug, Clone, serde::Serialize)]
pub struct IssueDraft {
    /// PR 标题
    pub title: String,
    /// PR 简短描述（Markdown）
    pub body: String,
    /// 待提交文件的相对路径清单
    pub files: Vec<String>,
}

/// 单个待提交文件：相对路径 + 原始字节内容（提交时 base64 编码）。
pub struct ExpertFile {
    /// 相对专家根目录的路径，如 `agents/demo.md`
    pub path: String,
    /// 文件原始字节内容
    pub content: Vec<u8>,
}

/// 遍历专家目录，收集所有文件（含二进制头像等），按路径排序返回。
pub fn collect_expert_files(expert: &ExpertMetadata) -> Result<Vec<ExpertFile>, String> {
    let dir = Path::new(&expert.definition_dir);
    let mut files = Vec::new();
    walk_dir(dir, dir, &mut files)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// 递归遍历目录收集文件；不跟随符号链接，避免死循环与越界。
fn walk_dir(base: &Path, dir: &Path, out: &mut Vec<ExpertFile>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("读取目录 {} 失败: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        // file_type 不跟随符号链接：symlink 的 is_dir 为 false，不会递归进去。
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            walk_dir(base, &path, out)?;
            continue;
        }
        let rel = path
            .strip_prefix(base)
            .map_err(|e| format!("计算相对路径失败: {e}"))?;
        let content = std::fs::read(&path)
            .map_err(|e| format!("读取文件 {} 失败: {e}", path.display()))?;
        out.push(ExpertFile {
            path: rel.to_string_lossy().to_string(),
            content,
        });
    }
    Ok(())
}

/// 组装贡献草稿：标题 + 简短描述 + 文件清单。
pub fn build_issue_draft(expert: &ExpertMetadata) -> Result<IssueDraft, String> {
    let files = collect_expert_files(expert)?;
    let title = format!("[专家贡献] {} v{}", expert.name, expert.version);
    let body = build_pr_body(expert, &files);
    let file_paths: Vec<String> = files.into_iter().map(|f| f.path).collect();
    Ok(IssueDraft {
        title,
        body,
        files: file_paths,
    })
}

/// 组装简短 PR 描述：元信息表格 + 文件清单（不含文件内容，内容以真实文件提交）。
fn build_pr_body(expert: &ExpertMetadata, files: &[ExpertFile]) -> String {
    let mut s = String::from("## 专家贡献\n\n> 由 ntd 用户提交，等待官方审核合入。\n\n");
    s.push_str("| 属性 | 值 |\n|---|---|\n");
    s.push_str(&format!("| 名称 | `{}` |\n", expert.name));
    s.push_str(&format!("| 类型 | {} |\n", expert_type_label(&expert.expert_type)));
    s.push_str(&format!("| 版本 | `{}` |\n", expert.version));
    if let Some(desc) = expert.description_zh.as_ref().or(expert.description_en.as_ref()) {
        s.push_str(&format!("| 说明 | {} |\n", desc));
    }
    s.push('\n');
    s.push_str(&format!("**提交文件（{} 个）**：\n", files.len()));
    for f in files {
        s.push_str(&format!("- `{}`\n", f.path));
    }
    s.push('\n');
    s
}

/// 专家类型的中文标签。
fn expert_type_label(t: &ExpertType) -> &'static str {
    match t {
        ExpertType::Agent => "agent（单个专家）",
        ExpertType::Team => "team（专家团队）",
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;

    /// 在临时目录构造一个最小专家目录，返回构造好的 ExpertMetadata。
    fn build_sample_expert(dir: &Path) -> ExpertMetadata {
        let plugin_dir = dir.join(".codebuddy-plugin");
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::write(
            plugin_dir.join("plugin.json"),
            r#"{"name":"demo","version":"1.0.0","expertType":"agent"}"#,
        )
        .unwrap();

        let agents_dir = dir.join("agents");
        fs::create_dir_all(&agents_dir).unwrap();
        fs::write(agents_dir.join("demo.md"), "# 我是 demo 专家").unwrap();

        let skill_dir = dir.join("skills").join("my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# 我是技能").unwrap();

        ExpertMetadata {
            name: "demo".to_string(),
            expert_type: ExpertType::Agent,
            version: "1.0.0".to_string(),
            source: crate::expert::ExpertSource::User,
            display_name_zh: None,
            display_name_en: None,
            profession_zh: None,
            profession_en: None,
            description_zh: Some("示例专家".to_string()),
            description_en: None,
            avatar_path: None,
            category_id: None,
            definition_dir: dir.to_string_lossy().to_string(),
            plugin_json_path: plugin_dir.join("plugin.json").to_string_lossy().to_string(),
            agent_name: Some("demo".to_string()),
            lead_agent: None,
            member_agents: vec![],
            members: vec![],
            skills: vec!["skills/my-skill".to_string()],
            default_init_prompt_zh: None,
            default_init_prompt_en: None,
            tags: vec![],
            loaded_at: "now".to_string(),
            is_active: true,
        }
    }

    #[test]
    fn collect_expert_files_includes_plugin_agent_skill_and_binary() {
        let dir = tempfile::tempdir().unwrap();
        // 追加一个二进制头像文件，验证非文本文件也被收集。
        let avatar_dir = dir.path().join("avatars");
        fs::create_dir_all(&avatar_dir).unwrap();
        fs::write(avatar_dir.join("logo.png"), [0x89, 0x50, 0x4e, 0x47]).unwrap();

        let expert = build_sample_expert(dir.path());
        let files = collect_expert_files(&expert).unwrap();

        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&".codebuddy-plugin/plugin.json"));
        assert!(paths.contains(&"agents/demo.md"));
        assert!(paths.contains(&"skills/my-skill/SKILL.md"));
        assert!(paths.contains(&"avatars/logo.png"));
        // 二进制文件字节被完整读取。
        let png = files.iter().find(|f| f.path == "avatars/logo.png").unwrap();
        assert_eq!(png.content, vec![0x89, 0x50, 0x4e, 0x47]);
    }

    #[test]
    fn build_issue_draft_has_table_and_file_list() {
        let dir = tempfile::tempdir().unwrap();
        let expert = build_sample_expert(dir.path());
        let draft = build_issue_draft(&expert).unwrap();

        assert!(draft.title.starts_with("[专家贡献] demo v1.0.0"));
        // 正文含元信息表格与文件清单。
        assert!(draft.body.contains("| 名称 | `demo` |"));
        assert!(draft.body.contains("**提交文件（"));
        // files 清单包含各文件路径。
        assert!(draft.files.iter().any(|f| f == ".codebuddy-plugin/plugin.json"));
        assert!(draft.files.iter().any(|f| f == "agents/demo.md"));
        assert!(draft.files.iter().any(|f| f == "skills/my-skill/SKILL.md"));
    }
}
