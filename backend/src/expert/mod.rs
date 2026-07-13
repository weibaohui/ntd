//! WorkBuddy 专家系统集成模块
//!
//! 完全兼容 WorkBuddy 的 plugin.json + MD 文件格式。
//! 采用纯文件存储 + 内存索引架构：
//! - 专家定义保持文件存储，方便用户直接编辑
//! - 启动时扫描构建内存索引，查询高效
//! - 按需加载 MD 文件内容

pub mod index;
pub mod loader;
pub mod parser;
pub mod types;

pub use types::ExpertIndexManager;
pub use loader::{build_skills_context, experts_dir, load_experts_from_directory};
pub use parser::{
    build_expert_metadata, extract_yaml_frontmatter, parse_agent_md_metadata,
    parse_plugin_json, parse_skill_metadata,
};
pub use types::{
    AgentFileMetadata, ExpertError, ExpertLoadResult, ExpertMember, ExpertMetadata, ExpertTag,
    ExpertType, LoadResult, LocalizedText, MemberJson, MemberRole, PluginJson, SkillMetadata,
    TeamInfoJson,
};
