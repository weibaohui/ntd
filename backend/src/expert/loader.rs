//! 目录扫描加载器
//!
//! 扫描 ~/.ntd/experts/ 目录，加载所有专家定义。

use std::path::Path;

use super::parser::*;
use super::types::*;

/// 专家定义根目录名称
const EXPERTS_DIR_NAME: &str = "experts";

/// 获取专家定义根目录路径（~/.ntd/experts/）
///
/// # 返回
/// 专家定义根目录的绝对路径，如果无法获取 home 目录则返回 None
pub fn experts_dir() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|home| home.join(".ntd").join(EXPERTS_DIR_NAME))
}

/// 从指定目录加载所有专家定义
///
/// # 参数
/// - `experts_dir`: 专家定义根目录
/// - `manager`: 专家索引管理器
///
/// # 返回
/// 加载结果（成功数量和错误列表）
pub fn load_experts_from_directory(
    experts_dir: &Path,
    manager: &ExpertIndexManager,
) -> LoadResult {
    let mut loaded_count = 0;
    let mut errors = Vec::new();

    // 遍历专家目录
    let entries = match std::fs::read_dir(experts_dir) {
        Ok(e) => e,
        Err(e) => {
            errors.push(format!(
                "无法读取专家目录 {}: {}",
                experts_dir.display(),
                e
            ));
            return LoadResult {
                loaded_count,
                errors,
            };
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                errors.push(format!("目录项读取错误: {}", e));
                continue;
            }
        };

        let expert_dir = entry.path();
        if !expert_dir.is_dir() {
            continue;
        }

        let plugin_json_path = expert_dir.join(".codebuddy-plugin/plugin.json");

        // 检查 plugin.json 是否存在
        if !plugin_json_path.exists() {
            continue;
        }

        // 解析并加载单个专家
        match load_single_expert(&expert_dir, &plugin_json_path) {
            Ok(load_result) => {
                manager.update_index(
                    &load_result.expert,
                    &load_result.agent_files,
                    &load_result.skills,
                );
                loaded_count += 1;
            }
            Err(e) => {
                errors.push(format!("{}: {}", expert_dir.display(), e));
            }
        }
    }

    LoadResult {
        loaded_count,
        errors,
    }
}

/// 加载单个专家定义（含 Agent MD 和 Skills）
///
/// # 参数
/// - `expert_dir`: 专家定义目录
/// - `plugin_json_path`: plugin.json 路径
///
/// # 返回
/// 专家加载结果
pub fn load_single_expert(
    expert_dir: &Path,
    plugin_json_path: &Path,
) -> Result<ExpertLoadResult, ExpertError> {
    // 1. 解析 plugin.json
    let plugin = parse_plugin_json(plugin_json_path)?;

    // 2. 构建 ExpertMetadata
    let expert_meta = build_expert_metadata(&plugin, expert_dir, plugin_json_path);

    // 3. 加载所有 Agent MD 文件元数据（只解析 YAML frontmatter）
    let mut agent_files = Vec::new();
    // 获取 agent 路径列表：优先用 plugin.agents，否则从 agents/ 目录扫描
    let agent_paths = get_agent_paths(&plugin, expert_dir);
    for agent_path in &agent_paths {
        let md_path = expert_dir.join(agent_path);
        if md_path.exists() {
            let metadata = parse_agent_md_metadata(&md_path)?;
            agent_files.push(metadata);
        }
    }

    // 4. 加载专家关联的所有 Skills
    let mut skills = Vec::new();
    for skill_rel_path in &expert_meta.skills {
        let skill_dir = expert_dir.join(skill_rel_path);
        let skill_md_path = skill_dir.join("SKILL.md");
        if skill_md_path.exists() {
            let skill_meta = parse_skill_metadata(&skill_dir, &skill_md_path)?;
            skills.push(skill_meta);
        }
    }

    Ok(ExpertLoadResult {
        expert: expert_meta,
        agent_files,
        skills,
    })
}

/// 构建 Skills 上下文：名称 + 简要描述（prompt 注入用）
///
/// # 参数
/// - `skills`: Skill 元数据列表
///
/// # 返回
/// 拼装好的 Skills 描述文本
pub fn build_skills_context(skills: &[SkillMetadata]) -> String {
    if skills.is_empty() {
        return String::new();
    }

    let mut parts = Vec::new();
    parts.push("## 可用技能\n".to_string());
    parts.push("你可以使用以下技能来辅助完成任务：\n".to_string());

    for skill in skills {
        let desc = skill
            .yaml_description_zh
            .as_ref()
            .or(skill.yaml_description_en.as_ref())
            .or(skill.yaml_description.as_ref())
            .cloned()
            .unwrap_or_else(|| "(无描述)".to_string());

        parts.push(format!("- **{}**: {}\n", skill.skill_name, desc));
    }

    parts.push("\n请根据需要自行调用上述技能。".to_string());
    parts.join("")
}

/// 获取 agent 路径列表
///
/// 优先使用 plugin.agents 字段，若不存在则扫描 agents/ 目录下所有 .md 文件作为 fallback。
/// 这是为了兼容部分旧版本的 team 类型专家，它们的 plugin.json 中没有 agents 字段，
/// 但成员信息存储在 members 和 teamInfo 中，实际的 MD 文件放在 agents/ 目录下。
///
/// # 参数
/// - `plugin`: 解析后的 PluginJson
/// - `expert_dir`: 专家定义目录
///
/// # 返回
/// agent 相对路径列表
fn get_agent_paths(plugin: &PluginJson, expert_dir: &Path) -> Vec<String> {
    // 优先使用 plugin.agents
    if let Some(agents) = &plugin.agents {
        return agents.clone();
    }

    // fallback: 扫描 agents/ 目录下所有 .md 文件
    let agents_dir = expert_dir.join("agents");
    let mut result = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&agents_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "md" {
                        // 构造相对路径，如 "agents/xxx.md"
                        if let Ok(rel_path) = path.strip_prefix(expert_dir) {
                            result.push(rel_path.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
    }

    // 按文件名排序，保证加载顺序稳定
    result.sort();
    result
}
