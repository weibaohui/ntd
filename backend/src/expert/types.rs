//! 专家系统类型定义
//!
//! 完全兼容 WorkBuddy 的 plugin.json 格式，不引入新格式。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// PluginJson：直接对应 WorkBuddy 的 plugin.json 结构
// ---------------------------------------------------------------------------

/// WorkBuddy plugin.json 根结构
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginJson {
    /// 专家/团队 ID（全局唯一）
    pub name: String,
    /// 版本号
    pub version: String,
    /// 英文描述
    pub description: Option<String>,
    /// 类型：agent 或 team
    #[serde(rename = "expertType")]
    pub expert_type: ExpertType,

    /// 显示名称（多语言）
    #[serde(rename = "displayName")]
    pub display_name: Option<LocalizedText>,
    /// 职业（多语言）
    pub profession: Option<LocalizedText>,
    /// 显示描述（多语言）
    #[serde(rename = "displayDescription")]
    pub display_description: Option<LocalizedText>,

    /// 头像相对路径
    pub avatar: Option<String>,
    /// 分类 ID
    #[serde(rename = "categoryId")]
    pub category_id: Option<String>,

    /// 标签列表
    pub tags: Option<Vec<LocalizedText>>,

    /// 默认初始提示词（多语言）
    #[serde(rename = "defaultInitPrompt")]
    pub default_init_prompt: Option<LocalizedText>,
    /// 快捷提示词列表
    #[serde(rename = "quickPrompts")]
    pub quick_prompts: Option<Vec<LocalizedText>>,

    /// Agent 定义文件相对路径列表
    /// 
    /// 某些旧版本的 team 类型专家可能没有此字段，
    /// 此时会从 agents 目录扫描所有 .md 文件作为 fallback
    pub agents: Option<Vec<String>>,
    /// 当前激活的 agent name
    #[serde(rename = "agentName")]
    pub agent_name: Option<String>,

    /// 技能列表（相对路径）
    pub skills: Option<Vec<String>>,

    /// 团队信息（仅 team 类型）
    #[serde(rename = "teamInfo")]
    pub team_info: Option<TeamInfoJson>,
    /// 成员详情（仅 team 类型）
    pub members: Option<Vec<MemberJson>>,
}

/// 专家类型
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ExpertType {
    /// 单个专家
    Agent,
    /// 专家团队
    Team,
}

/// 多语言文本
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LocalizedText {
    /// 中文
    pub zh: Option<String>,
    /// 英文
    pub en: Option<String>,
}

/// 团队信息（来自 plugin.json）
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TeamInfoJson {
    /// 主理人 ID
    #[serde(rename = "leadAgent")]
    pub lead_agent: String,
    /// 成员 ID 列表
    #[serde(rename = "memberAgents")]
    pub member_agents: Vec<String>,
}

/// 成员详情（来自 plugin.json）
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemberJson {
    /// 成员 ID
    pub id: String,
    /// 姓名（多语言）
    pub name: Option<LocalizedText>,
    /// 职业（多语言）
    pub profession: Option<LocalizedText>,
    /// 头像相对路径
    pub avatar: Option<String>,
    /// 角色：lead 或 member
    pub role: String,
}

// ---------------------------------------------------------------------------
// 内存索引结构
// ---------------------------------------------------------------------------

/// 专家元数据（内存索引，从 plugin.json 解析）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpertMetadata {
    /// 专家 ID（全局唯一）
    pub name: String,
    /// 类型
    pub expert_type: ExpertType,
    /// 版本号
    pub version: String,

    // 显示信息（中文优先）
    /// 中文名
    pub display_name_zh: Option<String>,
    /// 英文名
    pub display_name_en: Option<String>,
    /// 中文职业
    pub profession_zh: Option<String>,
    /// 英文职业
    pub profession_en: Option<String>,
    /// 中文描述
    pub description_zh: Option<String>,
    /// 英文描述
    pub description_en: Option<String>,

    /// 头像相对路径
    pub avatar_path: Option<String>,
    /// 分类 ID
    pub category_id: Option<String>,

    /// 定义文件所在目录（绝对路径）
    pub definition_dir: String,
    /// plugin.json 绝对路径
    pub plugin_json_path: String,

    /// 单个专家的 agent_name
    pub agent_name: Option<String>,

    // 团队信息（仅 team 类型）
    /// 主理人 ID
    pub lead_agent: Option<String>,
    /// 成员 ID 列表
    pub member_agents: Vec<String>,
    /// 成员详情
    pub members: Vec<ExpertMember>,

    /// 技能路径列表（相对路径）
    pub skills: Vec<String>,

    /// 默认初始提示词（中文）
    pub default_init_prompt_zh: Option<String>,
    /// 默认初始提示词（英文）
    pub default_init_prompt_en: Option<String>,

    /// 标签列表
    pub tags: Vec<ExpertTag>,

    /// 加载时间
    pub loaded_at: String,
    /// 是否激活
    pub is_active: bool,
}

/// 专家成员
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpertMember {
    /// 成员 ID
    pub id: String,
    /// 中文名
    pub name_zh: Option<String>,
    /// 英文名
    pub name_en: Option<String>,
    /// 中文职业
    pub profession_zh: Option<String>,
    /// 英文职业
    pub profession_en: Option<String>,
    /// 头像相对路径
    pub avatar_path: Option<String>,
    /// 角色
    pub role: MemberRole,
}

/// 成员角色
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MemberRole {
    /// 主理人
    Lead,
    /// 成员
    Member,
}

/// 专家标签
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpertTag {
    /// 中文标签
    pub zh: String,
    /// 英文标签
    pub en: String,
}

/// Agent MD 文件元数据（从 YAML frontmatter 解析）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFileMetadata {
    /// Agent ID
    pub agent_name: String,
    /// MD 文件绝对路径
    pub md_file_path: String,

    // YAML frontmatter
    /// YAML name
    pub yaml_name: Option<String>,
    /// YAML description
    pub yaml_description: Option<String>,
    /// YAML color
    pub yaml_color: Option<String>,
    /// YAML emoji
    pub yaml_emoji: Option<String>,
    /// YAML vibe
    pub yaml_vibe: Option<String>,
}

/// Skill 元数据（从 SKILL.md 解析）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    /// Skill ID（目录名）
    pub skill_name: String,
    /// Skill 目录绝对路径
    pub skill_dir: String,
    /// SKILL.md 绝对路径
    pub skill_md_path: String,

    // YAML frontmatter
    /// YAML name
    pub yaml_name: Option<String>,
    /// YAML description
    pub yaml_description: Option<String>,
    /// YAML description_zh
    pub yaml_description_zh: Option<String>,
    /// YAML description_en
    pub yaml_description_en: Option<String>,
    /// YAML version
    pub yaml_version: Option<String>,
    /// allowed-tools 列表
    #[serde(default)]
    pub yaml_allowed_tools: Vec<String>,
    /// YAML emoji
    pub yaml_emoji: Option<String>,
}

// ---------------------------------------------------------------------------
// 错误类型
// ---------------------------------------------------------------------------

/// 专家系统错误
#[derive(Debug, thiserror::Error)]
pub enum ExpertError {
    /// IO 错误
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    /// JSON 解析错误
    #[error("JSON 解析错误: {0}")]
    JsonParse(#[from] serde_json::Error),
    /// YAML 解析错误
    #[error("YAML 解析错误: {0}")]
    YamlParse(String),
    /// Agent 未找到
    #[error("Agent 未找到: {0}")]
    AgentNotFound(String),
    /// Skill 未找到
    #[error("Skill 未找到: {0}")]
    SkillNotFound(String),
    /// 专家未找到
    #[error("专家未找到: {0}")]
    ExpertNotFound(String),
    /// 文件读取错误
    #[error("文件读取错误 {0}: {1}")]
    FileReadError(String, std::io::Error),
    /// Frontmatter 提取错误
    #[error("Frontmatter 提取错误: {0}")]
    FrontmatterError(String),
}

/// 加载结果
#[derive(Debug, Serialize)]
pub struct LoadResult {
    /// 成功加载的专家数量
    pub loaded_count: usize,
    /// 加载错误列表
    pub errors: Vec<String>,
}

/// 专家加载结果（单次）
#[derive(Debug)]
pub struct ExpertLoadResult {
    /// 专家元数据
    pub expert: ExpertMetadata,
    /// Agent 文件元数据列表
    pub agent_files: Vec<AgentFileMetadata>,
    /// Skill 元数据列表
    pub skills: Vec<SkillMetadata>,
}

// ---------------------------------------------------------------------------
// 专家索引管理器
// ---------------------------------------------------------------------------

use parking_lot::RwLock;

/// 专家索引管理器（内存缓存）
pub struct ExpertIndexManager {
    /// name -> ExpertMetadata
    pub(crate) experts: RwLock<HashMap<String, ExpertMetadata>>,
    /// agent_name -> AgentFileMetadata
    pub(crate) agent_files: RwLock<HashMap<String, AgentFileMetadata>>,
    /// skill_name -> SkillMetadata
    pub(crate) skills: RwLock<HashMap<String, SkillMetadata>>,
    /// expert_name -> [skill_name, ...]
    pub(crate) expert_skills: RwLock<HashMap<String, Vec<String>>>,
    /// category_id -> [expert_name, ...]
    pub(crate) category_index: RwLock<HashMap<String, Vec<String>>>,
}
