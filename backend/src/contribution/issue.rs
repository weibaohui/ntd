//! 读取本地专家目录，组装贡献 Issue 的标题与 Markdown body。

use std::path::Path;

use crate::expert::loader::resolve_within;
use crate::expert::{ExpertMetadata, ExpertType};

/// 贡献 Issue 草稿：标题 + Markdown body + 已打包的文件清单。
#[derive(Debug, Clone, serde::Serialize)]
pub struct IssueDraft {
    /// Issue 标题
    pub title: String,
    /// Issue Markdown 正文
    pub body: String,
    /// 已打包进 body 的文件相对路径清单（用于前端预览展示）
    pub files: Vec<String>,
}

/// 组装专家贡献 Issue 草稿。
///
/// body 依次内嵌 plugin.json、agents/*.md、skills/*/SKILL.md 的完整文本；
/// 每个文件用 <details> 折叠，正文只保留元信息表格 + 文件清单，避免正文冗长。
pub fn build_issue_draft(expert: &ExpertMetadata) -> Result<IssueDraft, String> {
    let dir = Path::new(&expert.definition_dir);
    let title = format!("[专家贡献] {} v{}", expert.name, expert.version);

    // 先收集所有文件并生成各自的折叠块；缺失文件（如专家无 skill）跳过。
    let mut files = Vec::new();
    let mut sections = Vec::new();
    collect_file(dir, ".codebuddy-plugin/plugin.json", &mut files, &mut sections)?;
    for rel in list_agent_md_files(dir) {
        collect_file(dir, &rel, &mut files, &mut sections)?;
    }
    for skill_rel in &expert.skills {
        let skill_md_rel = format!("{skill_rel}/SKILL.md");
        collect_file(dir, &skill_md_rel, &mut files, &mut sections)?;
    }

    let body = build_body(expert, &files, &sections);
    Ok(IssueDraft { title, body, files })
}

/// 组装 body：元信息表格 + 文件清单 + 各文件折叠块。
fn build_body(expert: &ExpertMetadata, files: &[String], sections: &[String]) -> String {
    let mut body = build_header(expert, files);
    for section in sections {
        body.push_str(section);
    }
    body
}

/// 组装 body 头部：元信息表格 + 文件清单。
fn build_header(expert: &ExpertMetadata, files: &[String]) -> String {
    let mut s = String::from("## 专家贡献\n\n> 由 ntd 用户提交，等待官方审核合入。\n\n");
    s.push_str("| 属性 | 值 |\n|---|---|\n");
    s.push_str(&format!("| 名称 | `{}` |\n", expert.name));
    s.push_str(&format!("| 类型 | {} |\n", expert_type_label(&expert.expert_type)));
    s.push_str(&format!("| 版本 | `{}` |\n", expert.version));
    if let Some(desc) = expert.description_zh.as_ref().or(expert.description_en.as_ref()) {
        s.push_str(&format!("| 说明 | {} |\n", desc));
    }
    s.push('\n');
    s.push_str(&format!("**包含文件（{} 个）**：\n", files.len()));
    for f in files {
        s.push_str(&format!("- `{f}`\n"));
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

/// 读取单个文件，生成折叠块并记录到文件清单。
fn collect_file(
    dir: &Path,
    rel: &str,
    files: &mut Vec<String>,
    sections: &mut Vec<String>,
) -> Result<(), String> {
    // resolve_within 校验 rel 仍在专家目录内，防 plugin.json 里的 .. 逃逸。
    let Some(full) = resolve_within(dir, rel) else {
        // 文件缺失（如专家无 skill）不视为错误：跳过即可。
        return Ok(());
    };
    let content = std::fs::read_to_string(&full).map_err(|e| format!("读取 {rel} 失败: {e}"))?;
    files.push(rel.to_string());
    sections.push(build_details_block(rel, &content));
    Ok(())
}

/// 生成单个文件的 <details> 折叠块，内嵌带语言标签的 markdown 代码块。
fn build_details_block(rel: &str, content: &str) -> String {
    // 按扩展名选语言标签，便于 issue 页面语法高亮。
    let lang = match rel.rsplit('.').next() {
        Some("json") => "json",
        Some("md") => "markdown",
        _ => "",
    };
    format!(
        "<details>\n<summary>📄 {rel}（点击展开）</summary>\n\n```{lang}\n{content}\n```\n\n</details>\n\n"
    )
}

/// 扫描 `agents/` 目录，返回相对专家根目录的 .md 文件路径（已排序）。
fn list_agent_md_files(expert_dir: &Path) -> Vec<String> {
    let agents_dir = expert_dir.join("agents");
    let mut result = Vec::new();
    let Ok(entries) = std::fs::read_dir(&agents_dir) else {
        return result;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        if let Ok(rel) = path.strip_prefix(expert_dir) {
            result.push(rel.to_string_lossy().to_string());
        }
    }
    result.sort();
    result
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
    fn build_issue_draft_contains_plugin_agent_and_skill() {
        let dir = tempfile::tempdir().unwrap();
        let expert = build_sample_expert(dir.path());
        let draft = build_issue_draft(&expert).unwrap();

        // 标题以固定前缀开头并带版本号。
        assert!(draft.title.starts_with("[专家贡献] demo v1.0.0"));
        // body 含元信息表格、<details> 折叠块与三个文件内容。
        assert!(draft.body.contains("| 名称 | `demo` |"));
        assert!(draft.body.contains("<details>"));
        assert!(draft.body.contains("我是 demo 专家"));
        assert!(draft.body.contains("我是技能"));
        // files 清单包含 plugin.json 与 agent md 与 skill md。
        assert!(draft.files.iter().any(|f| f == ".codebuddy-plugin/plugin.json"));
        assert!(draft.files.iter().any(|f| f == "agents/demo.md"));
        assert!(draft.files.iter().any(|f| f == "skills/my-skill/SKILL.md"));
    }

    #[test]
    fn build_issue_draft_skips_missing_skill() {
        let dir = tempfile::tempdir().unwrap();
        let mut expert = build_sample_expert(dir.path());
        // 指向一个不存在的 skill，应被跳过而非报错。
        expert.skills = vec!["skills/ghost".to_string()];
        let draft = build_issue_draft(&expert).unwrap();
        assert!(!draft.files.iter().any(|f| f.contains("ghost")));
    }
}
