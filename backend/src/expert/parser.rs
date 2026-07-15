//! 解析器：plugin.json + YAML frontmatter

use std::path::Path;

use super::types::*;

// ---------------------------------------------------------------------------
// plugin.json 解析
// ---------------------------------------------------------------------------

/// 解析 plugin.json 文件
///
/// # 参数
/// - `plugin_json_path`: plugin.json 的绝对路径
///
/// # 返回
/// 解析后的 PluginJson 结构
pub fn parse_plugin_json(plugin_json_path: &Path) -> Result<PluginJson, ExpertError> {
    let content = std::fs::read_to_string(plugin_json_path)
        .map_err(|e| ExpertError::FileReadError(plugin_json_path.display().to_string(), e))?;
    let plugin: PluginJson = serde_json::from_str(&content)?;
    Ok(plugin)
}

/// 从 PluginJson 构建 ExpertMetadata
///
/// # 参数
/// - `plugin`: 解析后的 PluginJson
/// - `expert_dir`: 专家定义目录的绝对路径
/// - `plugin_json_path`: plugin.json 的绝对路径
/// - `source`: 专家来源（系统内置 / 用户自定义）
///
/// # 返回
/// 构建好的 ExpertMetadata
pub fn build_expert_metadata(
    plugin: &PluginJson,
    expert_dir: &Path,
    plugin_json_path: &Path,
    source: ExpertSource,
) -> ExpertMetadata {
    let now = chrono::Utc::now().to_rfc3339();

    ExpertMetadata {
        name: plugin.name.clone(),
        expert_type: plugin.expert_type.clone(),
        version: plugin.version.clone(),
        // 显示名称：优先 displayName.zh → displayName.en → name（ID 兜底，总比 description 好）
        display_name_zh: plugin
            .display_name
            .as_ref()
            .and_then(|d| d.zh.clone())
            .or_else(|| plugin.display_name.as_ref().and_then(|d| d.en.clone()))
            .or_else(|| Some(plugin.name.clone())),
        display_name_en: plugin
            .display_name
            .as_ref()
            .and_then(|d| d.en.clone())
            .or_else(|| Some(plugin.name.clone())),
        // 职业：优先 profession → team 类型用 lead 成员职业兜底
        profession_zh: plugin
            .profession
            .as_ref()
            .and_then(|d| d.zh.clone())
            .or_else(|| fallback_profession_from_members(plugin, "zh")),
        profession_en: plugin
            .profession
            .as_ref()
            .and_then(|d| d.en.clone())
            .or_else(|| fallback_profession_from_members(plugin, "en")),
        // 描述：优先 displayDescription.zh → description_zh（旧格式）→ description（英文兜底）
        description_zh: plugin
            .display_description
            .as_ref()
            .and_then(|d| d.zh.clone())
            .or_else(|| plugin.description_zh.clone())
            .or_else(|| plugin.description.clone()),
        description_en: plugin
            .display_description
            .as_ref()
            .and_then(|d| d.en.clone())
            .or_else(|| plugin.description.clone()),
        avatar_path: plugin.avatar.clone(),
        category_id: plugin.category_id.clone(),
        definition_dir: expert_dir.to_string_lossy().to_string(),
        plugin_json_path: plugin_json_path.to_string_lossy().to_string(),
        agent_name: plugin.agent_name.clone(),
        lead_agent: plugin.team_info.as_ref().map(|t| t.lead_agent.clone()),
        member_agents: plugin
            .team_info
            .as_ref()
            .map(|t| t.member_agents.clone())
            .unwrap_or_default(),
        members: plugin
            .members
            .as_ref()
            .map(|ms| ms.iter().map(member_json_to_expert_member).collect())
            .unwrap_or_default(),
        skills: plugin.skills.clone().unwrap_or_default(),
        default_init_prompt_zh: plugin.default_init_prompt.as_ref().and_then(|d| d.zh.clone()),
        default_init_prompt_en: plugin.default_init_prompt.as_ref().and_then(|d| d.en.clone()),
        tags: plugin
            .tags
            .as_ref()
            .map(|ts| ts.iter().map(localized_text_to_tag).collect())
            .unwrap_or_default(),
        loaded_at: now,
        is_active: true,
        source,
    }
}

/// 将 MemberJson 转换为 ExpertMember
fn member_json_to_expert_member(m: &MemberJson) -> ExpertMember {
    ExpertMember {
        id: m.id.clone(),
        name_zh: m.name.as_ref().and_then(|n| n.zh.clone()),
        name_en: m.name.as_ref().and_then(|n| n.en.clone()),
        profession_zh: m.profession.as_ref().and_then(|p| p.zh.clone()),
        profession_en: m.profession.as_ref().and_then(|p| p.en.clone()),
        avatar_path: m.avatar.clone(),
        role: if m.role == "lead" {
            MemberRole::Lead
        } else {
            MemberRole::Member
        },
    }
}

/// 将 LocalizedText 转换为 ExpertTag
fn localized_text_to_tag(t: &LocalizedText) -> ExpertTag {
    ExpertTag {
        zh: t.zh.clone().unwrap_or_default(),
        en: t.en.clone().unwrap_or_default(),
    }
}

/// 从成员列表中提取 lead 成员的职业作为 team 类型的职业兜底
///
/// 当 plugin.json 没有 profession 字段时，用 lead 成员的职业来描述整个团队。
/// 例如 software-company 的 lead 是「交付总监」，就用这个作为团队职业。
///
/// # 参数
/// - `plugin`: 解析后的 PluginJson
/// - `lang`: 语言标识，"zh" 或 "en"
///
/// # 返回
/// lead 成员职业文本，如果找不到则返回 None
fn fallback_profession_from_members(plugin: &PluginJson, lang: &str) -> Option<String> {
    // 仅 team 类型适用
    if plugin.expert_type != ExpertType::Team {
        return None;
    }

    // 先确定 lead 的 ID
    let lead_id = plugin.team_info.as_ref().map(|t| &t.lead_agent)?;
    let members = plugin.members.as_ref()?;

    // 在 members 中查找 lead 成员
    let lead_member = members.iter().find(|m| m.id == *lead_id)?;

    // 根据语言返回职业
    match lang {
        "zh" => lead_member.profession.as_ref().and_then(|p| p.zh.clone()),
        "en" => lead_member.profession.as_ref().and_then(|p| p.en.clone()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// YAML frontmatter 解析
// ---------------------------------------------------------------------------

/// 从 MD 文件中提取 YAML frontmatter（--- 之间的内容）
///
/// # 参数
/// - `content`: MD 文件完整内容
///
/// # 返回
/// YAML 字符串（不含分隔符），如果没有 frontmatter 则返回空字符串
pub fn extract_yaml_frontmatter(content: &str) -> Result<String, ExpertError> {
    // 逐行扫描，仅当独立行（trim 后等于 "---"）才算分隔符。
    // 之前用 find("---") 会把 YAML 值里的 foo---bar 误判为结束符，截断合法 frontmatter。
    let mut lines = content.lines();
    // 首行必须是独立的 --- 开始标记
    match lines.next() {
        Some(line) if line.trim() == "---" => {}
        _ => return Ok(String::new()),
    }
    let mut yaml_lines = Vec::new();
    for line in lines {
        if line.trim() == "---" {
            return Ok(yaml_lines.join("\n").trim().to_string());
        }
        yaml_lines.push(line);
    }
    Err(ExpertError::FrontmatterError(
        "找不到 frontmatter 结束标记 ---".to_string(),
    ))
}

/// 解析 YAML frontmatter 为通用结构
///
/// # 参数
/// - `yaml_str`: YAML 字符串
///
/// # 返回
/// 解析后的 HashMap
pub fn parse_yaml_frontmatter(yaml_str: &str) -> Result<serde_yaml::Value, ExpertError> {
    if yaml_str.is_empty() {
        return Ok(serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
    }
    let value: serde_yaml::Value = serde_yaml::from_str(yaml_str)
        .map_err(|e| ExpertError::YamlParse(e.to_string()))?;
    Ok(value)
}

/// 从 Agent MD 文件解析元数据
///
/// # 参数
/// - `md_path`: MD 文件绝对路径
///
/// # 返回
/// AgentFileMetadata
pub fn parse_agent_md_metadata(md_path: &Path) -> Result<AgentFileMetadata, ExpertError> {
    let content = std::fs::read_to_string(md_path)
        .map_err(|e| ExpertError::FileReadError(md_path.display().to_string(), e))?;

    let yaml_str = extract_yaml_frontmatter(&content)?;
    let yaml = parse_yaml_frontmatter(&yaml_str)?;

    let agent_name = yaml
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            // 从文件名兜底
            md_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string()
        });

    Ok(AgentFileMetadata {
        agent_name,
        md_file_path: md_path.to_string_lossy().to_string(),
        yaml_name: yaml.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()),
        yaml_description: yaml
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        yaml_color: yaml
            .get("color")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        yaml_emoji: yaml
            .get("emoji")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        yaml_vibe: yaml
            .get("vibe")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    })
}

/// 从 Skill 目录解析元数据
///
/// # 参数
/// - `skill_dir`: Skill 目录绝对路径
/// - `skill_md_path`: SKILL.md 绝对路径
///
/// # 返回
/// SkillMetadata
pub fn parse_skill_metadata(
    skill_dir: &Path,
    skill_md_path: &Path,
) -> Result<SkillMetadata, ExpertError> {
    let content = std::fs::read_to_string(skill_md_path)
        .map_err(|e| ExpertError::FileReadError(skill_md_path.display().to_string(), e))?;

    let yaml_str = extract_yaml_frontmatter(&content)?;
    let yaml = parse_yaml_frontmatter(&yaml_str)?;

    // 从目录名兜底获取 skill_name
    let dir_name = skill_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let skill_name = yaml
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or(dir_name);

    // 解析 allowed-tools
    let allowed_tools = yaml
        .get("allowed-tools")
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|item| item.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    Ok(SkillMetadata {
        skill_name,
        skill_dir: skill_dir.to_string_lossy().to_string(),
        skill_md_path: skill_md_path.to_string_lossy().to_string(),
        yaml_name: yaml.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()),
        yaml_description: yaml
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        yaml_description_zh: yaml
            .get("description_zh")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        yaml_description_en: yaml
            .get("description_en")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        yaml_version: yaml
            .get("version")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        yaml_allowed_tools: allowed_tools,
        yaml_emoji: yaml
            .get("emoji")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    })
}
