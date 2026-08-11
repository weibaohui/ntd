//! 专家上下文注入的公共核心（同步纯函数层）。
//!
//! 本模块是 096-W1-PR3「跨文件重复收口」的落点：
//! 原 `executor_service::pre_spawn::inject_expert_context`（todo 执行管线）与
//! `services::blackboard::inject_wiki_expert_context`（wiki chat 通路）各自维护了一份
//! 逐字同构的专家注入逻辑（查索引 → 解析主理 agent → 读 Agent MD → 拼技能 → 三段式拼接），
//! 任何注入格式调整都要改两处。这里把「与调用方上下文无关」的核心提取为同步自由函数，
//! 两个调用方只保留各自的前置条件取数（todo.expert_name / 请求参数），再委托本模块。
//!
//! 设计取舍：
//! - 保持同步签名：核心链路（索引查询 + 文件读取 + 字符串拼接）全部同步，
//!   async 包装留给调用方（pre_spawn 的 `inject_expert_context` 保持 async 以兼容既有调用链）。
//! - `expert_name` 取 `Option<&str>` 而非 `&str`：两个调用方的原始输入都是可选值，
//!   把「未指定/空串」的快速回退收进公共层，调用方不必各自判空。

use super::ExpertIndexManager;

/// 为消息注入专家上下文：角色定义 + 技能列表前置到原消息。
///
/// 流程（任一步失败都静默回退原消息——专家注入是增强项，不应阻断主流程）：
/// 1. 专家名缺失或空串 → 原样返回；
/// 2. 索引中找不到专家 → warn 后原样返回；
/// 3. 专家无可用 agent（agent_name/lead_agent 均空）→ warn 后原样返回；
/// 4. Agent MD 文件读取失败 → warn 后原样返回；
/// 5. 全部命中 → 拼接三段式 prompt 返回。
///
/// 注入格式：
/// ```text
/// # 专家角色定义
/// {agent_md_content}
///
/// ## 可用技能
/// {skill_list}        ← 仅当技能非空时存在该段
///
/// # 任务
/// {original_message}
/// ```
///
/// 注意：本函数的 warn 日志通过 `caller_tag` 参数携带调用方前缀（如 "wiki chat: "），
/// 以便运维从聚合日志中区分注入失败来自 wiki chat 还是 todo 执行通路；todo 通路传空串
/// 保持其原有的无前缀文案。这是 096-W1 review 的可观测性修复——合并前 wiki 通路独有
/// "wiki chat:" 前缀，收口时一度丢失，现通过参数显式回传。
pub(crate) fn inject_expert_message(
    expert_manager: &ExpertIndexManager,
    expert_name: Option<&str>,
    message: &str,
    caller_tag: &str,
) -> String {
    // 未指定专家名（或空串）时直接返回原消息——空串判等来自 wiki chat 通路的既有行为，
    // 收进公共层后两条通路语义一致。
    let name = match expert_name {
        Some(n) if !n.is_empty() => n,
        _ => return message.to_string(),
    };
    // 查找专家元数据，找不到则静默回退：专家可能已被卸载，不应因此阻断用户消息。
    let Some(metadata) = expert_manager.get_expert_by_name(name) else {
        tracing::warn!("{}未找到专家 '{}'，跳过专家上下文注入", caller_tag, name);
        return message.to_string();
    };
    // 解析主理 agent：team 用 lead_agent、agent 用 agent_name（resolve_agent_name 统一），
    // 并按 (expert_name, agent_name) 复合键查找，避免不同专家同名 agent 互窜。
    let Some(agent_name) = metadata.resolve_agent_name() else {
        tracing::warn!(
            "{}专家 '{}' 没有可用 agent（agent_name/lead_agent 都为空）",
            caller_tag,
            name
        );
        return message.to_string();
    };
    // 读取 Agent MD 全文；文件缺失/不可读同样静默回退（索引与文件可能不同步）。
    let Ok(agent_md) = expert_manager.get_agent_md_content(name, agent_name) else {
        tracing::warn!(
            "{}未找到专家 '{}' 的 Agent '{}' MD 内容，跳过注入",
            caller_tag,
            name,
            agent_name
        );
        return message.to_string();
    };
    let skills_text = build_expert_skills_text(expert_manager, name);
    build_expert_prompt(&agent_md, &skills_text, message)
}

/// 拼接专家技能列表文本：复用 loader 模块的 build_skills_context，支持中文描述优先。
///
/// 抽出来单独成函数是为了让 `inject_expert_message` 保持在 30 行内。
fn build_expert_skills_text(expert_manager: &ExpertIndexManager, expert_name: &str) -> String {
    // 复用 loader::build_skills_context，它已实现中文优先回退逻辑。
    super::build_skills_context(&expert_manager.get_expert_skills(expert_name))
}

/// 把 Agent MD、技能列表、原 message 拼成最终 prompt。
///
/// 三段式结构：专家角色定义 → 可用技能（由 build_skills_context 生成） → 任务。
/// skills_text 为空时不添加技能段落，避免无谓标题。
fn build_expert_prompt(agent_md: &str, skills_text: &str, original_message: &str) -> String {
    if skills_text.is_empty() {
        format!("# 专家角色定义\n{}\n\n# 任务\n{}", agent_md, original_message)
    } else {
        format!(
            "# 专家角色定义\n{}\n\n{}\n\n# 任务\n{}",
            agent_md, skills_text, original_message
        )
    }
}

#[cfg(test)]
// 测试夹具允许 unwrap/expect：与 pre_spawn 等既有测试模块的 lint 豁免惯例保持一致
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::expert::test_support::make_minimal_expert_metadata;
    use crate::expert::{AgentFileMetadata, SkillMetadata};

    /// RAII 守卫：持有临时 MD 文件路径，Drop 时自动删除。
    /// 断言失败 panic 时仍会触发 Drop（unwind），避免临时文件泄漏到 $TMPDIR。
    struct TmpMdFile(String);

    impl Drop for TmpMdFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    /// 准备临时 Agent MD 文件并注册到索引，返回 (manager, TmpMdFile)；
    /// TmpMdFile 的 Drop 自动删除临时文件，断言失败也不泄漏，调用方无需手动清理。
    fn make_manager_with_agent_md(content: &str, with_skill: bool) -> (ExpertIndexManager, TmpMdFile) {
        use std::io::Write;
        // 以纳秒时间戳命名，保证并发测试互不踩踏
        let mut tmp_path = std::env::temp_dir();
        tmp_path.push(format!(
            "ntd_test_inject_md_{}.md",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        // 测试夹具允许 unwrap（与仓库既有测试风格一致；lint 对 cfg(test) 中的 unwrap 豁免）
        let mut f = std::fs::File::create(&tmp_path).unwrap();
        writeln!(f, "{}", content).unwrap();
        let md_path = tmp_path.to_string_lossy().to_string();

        let manager = ExpertIndexManager::new();
        let agent_file = AgentFileMetadata {
            agent_name: "rust-agent".to_string(),
            md_file_path: md_path.clone(),
            yaml_name: None,
            yaml_description: None,
            yaml_color: None,
            yaml_emoji: None,
            yaml_vibe: None,
        };
        // 按 with_skill 决定是否注册技能，用于分别锁定两段式/三段式两种拼接格式
        let skills: Vec<SkillMetadata> = if with_skill {
            vec![SkillMetadata {
                skill_name: "code-review".to_string(),
                skill_dir: "/tmp".to_string(),
                skill_md_path: "/tmp/SKILL.md".to_string(),
                yaml_name: None,
                yaml_description: Some("代码评审".to_string()),
                yaml_description_zh: None,
                yaml_description_en: None,
                yaml_version: None,
                yaml_allowed_tools: vec![],
                yaml_emoji: None,
            }]
        } else {
            vec![]
        };
        let expert = make_minimal_expert_metadata("rust-expert", Some("rust-agent"), None);
        manager.update_index(&expert, &[agent_file], &skills);
        (manager, TmpMdFile(md_path))
    }

    /// 未指定专家名（None 或空串）时原样返回消息——空串判等是 wiki chat 通路的既有行为。
    #[test]
    fn test_inject_expert_message_none_or_empty_name_returns_original() {
        let manager = ExpertIndexManager::new();
        let original = "原始消息";
        assert_eq!(inject_expert_message(&manager, None, original, ""), original);
        assert_eq!(inject_expert_message(&manager, Some(""), original, ""), original);
    }

    /// 专家无技能时应省略「可用技能」段落（两段式），避免出现空标题。
    /// 这是 pre_spawn 既有测试未覆盖的分支，公共层补齐锁定。
    #[test]
    fn test_inject_expert_message_without_skills_omits_skills_section() {
        // _guard 绑定到作用域末尾，Drop 时自动删临时 MD；即使下方断言 panic 也不泄漏
        let (manager, _guard) = make_manager_with_agent_md("你是一个 Rust 专家", false);
        let result = inject_expert_message(&manager, Some("rust-expert"), "请帮我写代码", "");
        // 期望中的三个连续 \n：夹具用 writeln! 写入 MD 使 agent_md 自带尾部换行，
        // 拼接时再补两个 \n——与真实场景（MD 文件通常以换行结尾）一致
        assert_eq!(
            result,
            "# 专家角色定义\n你是一个 Rust 专家\n\n\n# 任务\n请帮我写代码"
        );
    }

    /// 专家有技能时输出三段式：角色定义 → 可用技能 → 任务，锁定拼接格式契约。
    #[test]
    fn test_inject_expert_message_with_skills_full_three_section_prompt() {
        // _guard 绑定到作用域末尾，Drop 时自动删临时 MD；即使下方断言 panic 也不泄漏
        let (manager, _guard) = make_manager_with_agent_md("你是一个 Rust 专家", true);
        let result = inject_expert_message(&manager, Some("rust-expert"), "请帮我写代码", "");
        assert!(result.starts_with("# 专家角色定义\n你是一个 Rust 专家\n"));
        assert!(result.contains("## 可用技能"));
        // build_skills_context 将技能名渲染为 Markdown 链接指向 SKILL.md 路径
        assert!(result.contains("- **[code-review]("));
        assert!(result.ends_with("# 任务\n请帮我写代码"));
    }

    /// `build_expert_prompt` 把 Agent MD、技能列表、原 message 拼成三段式 prompt。
    /// 有技能时保留技能段落（由 build_skills_context 生成，含标题行）。
    #[test]
    fn test_build_expert_prompt_three_sections() {
        let agent_md = "你是一个 Rust 专家";
        let skills = "## 可用技能\n你可以使用以下技能来辅助完成任务：\n- **code-review**: 代码评审技能\n";
        let original = "请帮我写一个函数";
        let result = build_expert_prompt(agent_md, skills, original);
        // 三段标题按顺序出现，且原 message 在末尾
        assert!(result.contains("# 专家角色定义\n你是一个 Rust 专家"));
        assert!(result.contains("## 可用技能"));
        assert!(result.contains("# 任务\n请帮我写一个函数"));
    }

    /// `build_expert_prompt` 技能列表为空时省略技能段落。
    #[test]
    fn test_build_expert_prompt_empty_skills_omits_section() {
        let result = build_expert_prompt("agent", "", "do something");
        // 空技能时不出现技能段落，直接从角色定义跳到任务
        assert!(!result.contains("可用技能"));
        assert!(result.contains("# 专家角色定义\nagent"));
        assert!(result.contains("# 任务\ndo something"));
    }

    /// `build_expert_skills_text` 复用 loader::build_skills_context，
    /// 返回 Markdown 格式的技能列表（含标题行和项目符号）。
    #[test]
    fn test_build_expert_skills_text_formats_each_skill() {
        let manager = ExpertIndexManager::new();
        // 准备两个 skill 的元数据并更新到索引
        let skills = vec![
            SkillMetadata {
                skill_name: "code-review".to_string(),
                skill_dir: "/tmp/skills/code-review".to_string(),
                skill_md_path: "/tmp/skills/code-review/SKILL.md".to_string(),
                yaml_name: None,
                yaml_description: Some("代码评审".to_string()),
                yaml_description_zh: None,
                yaml_description_en: None,
                yaml_version: None,
                yaml_allowed_tools: vec![],
                yaml_emoji: None,
            },
            SkillMetadata {
                skill_name: "test-gen".to_string(),
                skill_dir: "/tmp/skills/test-gen".to_string(),
                skill_md_path: "/tmp/skills/test-gen/SKILL.md".to_string(),
                yaml_name: None,
                // description 为 None 时回退到 "(无描述)"
                yaml_description: None,
                yaml_description_zh: None,
                yaml_description_en: None,
                yaml_version: None,
                yaml_allowed_tools: vec![],
                yaml_emoji: None,
            },
        ];
        // 借用一个最小 ExpertMetadata 把 skill 绑到 "test-expert"
        let expert = make_minimal_expert_metadata("test-expert", Some("test-agent"), None);
        manager.update_index(&expert, &[], &skills);
        let text = build_expert_skills_text(&manager, "test-expert");
        // build_skills_context 输出 Markdown 格式：标题 + 项目符号列表
        assert!(text.contains("## 可用技能"));
        // build_skills_context 将技能名渲染为 Markdown 链接指向 SKILL.md 路径
        assert!(text.contains("- **[code-review]("));
        assert!(text.contains("**: 代码评审"));
        assert!(text.contains("- **[test-gen]("));
        assert!(text.contains("**: (无描述)"));
    }

    /// `build_expert_skills_text` 查询不存在的专家时返回空串（get_expert_skills 容错）。
    #[test]
    fn test_build_expert_skills_text_unknown_expert_returns_empty() {
        let manager = ExpertIndexManager::new();
        let text = build_expert_skills_text(&manager, "non-existent-expert");
        assert!(text.is_empty());
    }
}
