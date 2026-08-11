//! 专家测试夹具：inject.rs 与 executor_service::pre_spawn 测试共享的构造助手。
//!
//! 096-W1-PR3 多 agent 评审发现：inject.rs 从 pre_spawn.rs 复制了一份逐字相同的
//! `make_minimal_expert_metadata`（22 字段 ExpertMetadata 构造）。ExpertMetadata 一旦加字段，
//! 两份会静默漂移——一处漏改即导致测试行为分歧。提到本模块统一维护，两处测试 import 共用。

use crate::expert::{ExpertMetadata, ExpertSource, ExpertType};

/// 构造最小可用的 ExpertMetadata 供测试使用。
///
/// 只填必要字段（name / agent_name / lead_agent），其余用空值/None，
/// 避免每个测试都重复 22 个字段。inject.rs 与 pre_spawn.rs 的测试共用本函数，
/// 保证两处构造逻辑始终一致。
pub(crate) fn make_minimal_expert_metadata(
    name: &str,
    agent_name: Option<&str>,
    lead_agent: Option<&str>,
) -> ExpertMetadata {
    ExpertMetadata {
        name: name.to_string(),
        expert_type: ExpertType::Agent,
        version: "0.0.1-test".to_string(),
        source: ExpertSource::System,
        display_name_zh: None,
        display_name_en: None,
        profession_zh: None,
        profession_en: None,
        description_zh: None,
        description_en: None,
        avatar_path: None,
        category_id: None,
        definition_dir: "/tmp".to_string(),
        plugin_json_path: "/tmp/plugin.json".to_string(),
        agent_name: agent_name.map(|s| s.to_string()),
        lead_agent: lead_agent.map(|s| s.to_string()),
        member_agents: vec![],
        members: vec![],
        skills: vec![],
        default_init_prompt_zh: None,
        default_init_prompt_en: None,
        tags: vec![],
        loaded_at: "test".to_string(),
        is_active: true,
    }
}
