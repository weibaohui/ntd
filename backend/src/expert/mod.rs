//! WorkBuddy 专家系统集成模块
//!
//! 完全兼容 WorkBuddy 的 plugin.json + MD 文件格式。
//! 采用纯文件存储 + 内存索引架构：
//! - 专家定义保持文件存储，方便用户直接编辑
//! - 启动时扫描构建内存索引，查询高效
//! - 按需加载 MD 文件内容

pub mod index;
// 专家上下文注入公共核心：todo 执行管线（pre_spawn）与 wiki chat 通路（blackboard）共用，
// 仅 crate 内可见——注入是内部增强能力，不对外暴露为 API。
pub(crate) mod inject;
// 测试夹具共享模块：仅 cfg(test) 下编译，inject.rs 与 executor_service::pre_spawn 的测试共用
// make_minimal_expert_metadata，避免 22 字段 ExpertMetadata 构造两处重复（096-W1 review 收口）
#[cfg(test)]
pub(crate) mod test_support;
pub mod loader;
pub mod parser;
pub mod types;

pub use types::ExpertIndexManager;
// re-export 让调用方以 `crate::expert::inject_expert_message` 直达，无需感知子模块布局
pub(crate) use inject::inject_expert_message;
pub use loader::{build_skills_context, bundled_experts_dir, experts_dir, load_experts_from_directory};
pub use parser::{
    build_expert_metadata, extract_yaml_frontmatter, parse_agent_md_metadata,
    parse_plugin_json, parse_skill_metadata,
};
pub use types::{
    AgentFileMetadata, ExpertError, ExpertLoadResult, ExpertMember, ExpertMetadata, ExpertSource,
    ExpertTag, ExpertType, LoadResult, LocalizedText, MemberJson, MemberRole, PluginJson,
    SkillMetadata, TeamInfoJson,
};
