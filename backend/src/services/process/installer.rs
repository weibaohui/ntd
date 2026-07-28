//! 工艺模板实例化：把 `process_templates` 中的 YAML 定义转换为可执行的 Loop。

use std::collections::HashMap;

use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter};

use crate::db::entity::{loop_phases, loop_steps, loops, process_templates};
use crate::db::Database;
use crate::models::utc_timestamp;

use super::{
    ExpectedArtifact, GateDefinition, InstallError, InstallResult, LinkDefinition, PhaseDefinition,
    ProcessDefinition,
};

/// 安装工艺模板到指定工作空间，返回生成的 Loop ID。
///
/// 流程：
/// 1. 解析 `process_templates.definition` YAML
/// 2. 创建 Loop（status=paused，记录来源模板与版本快照）
/// 3. 创建 Phase
/// 4. 第一遍：为每个 link 创建 todo + loop_step（goto 暂空）
/// 5. 第二遍：解析 `goto:<link_id>`，把模板 link id 映射到真实 loop_step id
pub async fn install_process_template(
    db: &Database,
    template: &process_templates::Model,
    workspace_id: i64,
    workspace_path: &str,
) -> Result<InstallResult, InstallError> {
    let mut definition: ProcessDefinition = serde_yaml::from_str(&template.definition)?;

    // skill 名称是自由文本（executor 在运行时注入），不做强制校验，仅记 warn。
    // step_template 已转为 spec 引用，不再做原型存在性校验。
    check_skill_warnings(db, &definition.phases).await?;

    // 解析 spec_ref / acceptance_criteria_ref 外部引用，覆盖 inline 文本。
    resolve_phase_spec_refs(&mut definition.phases);

    let loop_name = if definition.process.display_name.is_empty() {
        definition.process.name.clone()
    } else {
        definition.process.display_name.clone()
    };

    let limits_config = build_limits_config(&definition.limits);
    let abnormal_handler_trigger_on = definition
        .abnormal_handler
        .as_ref()
        .map(|h| serde_json::to_string(&h.trigger_on).unwrap_or_else(|_| "[]".to_string()))
        .unwrap_or_else(|| "[\"capped_step\",\"capped_token\",\"failed\"]".to_string());

    let loop_model = create_loop_from_template(
        db,
        &loop_name,
        &definition.process.description,
        workspace_id,
        workspace_path,
        &limits_config,
        &abnormal_handler_trigger_on,
        template.id,
        &template.version,
    )
    .await?;

    let (template_link_to_step, phase_count, step_count) = create_phases_and_steps(
        db,
        loop_model.id,
        workspace_id,
        workspace_path,
        &definition.phases,
    )
    .await?;

    resolve_goto_targets(
        db,
        loop_model.id,
        &definition.phases,
        &template_link_to_step,
    )
    .await?;

    Ok(InstallResult {
        loop_id: loop_model.id,
        loop_name,
        phase_count,
        step_count,
    })
}

/// 根据模板创建 Loop。
#[allow(clippy::too_many_arguments)]
async fn create_loop_from_template(
    db: &Database,
    name: &str,
    description: &str,
    workspace_id: i64,
    workspace_path: &str,
    limits_config: &str,
    abnormal_handler_trigger_on: &str,
    process_template_id: i64,
    process_template_version: &str,
) -> Result<loops::Model, sea_orm::DbErr> {
    let now = utc_timestamp();
    let am = loops::ActiveModel {
        name: ActiveValue::Set(name.to_string()),
        description: ActiveValue::Set(description.to_string()),
        workspace_id: ActiveValue::Set(Some(workspace_id)),
        workspace_path: ActiveValue::Set(Some(workspace_path.to_string())),
        webhook_enabled: ActiveValue::Set(false),
        icon: ActiveValue::Set("loop".to_string()),
        status: ActiveValue::Set("paused".to_string()),
        limits_config: ActiveValue::Set(limits_config.to_string()),
        abnormal_handler_trigger_on: ActiveValue::Set(abnormal_handler_trigger_on.to_string()),
        process_template_id: ActiveValue::Set(Some(process_template_id)),
        process_template_version: ActiveValue::Set(Some(process_template_version.to_string())),
        created_at: ActiveValue::Set(Some(now.clone())),
        updated_at: ActiveValue::Set(Some(now)),
        ..Default::default()
    };
    am.insert(&db.conn).await
}

/// 创建 Phase 与 Step，返回模板 link id 到真实 step id 的映射。
async fn create_phases_and_steps(
    db: &Database,
    loop_id: i64,
    workspace_id: i64,
    workspace_path: &str,
    phases: &[PhaseDefinition],
) -> Result<(HashMap<String, i64>, usize, usize), InstallError> {
    let mut template_link_to_step = HashMap::new();
    let mut step_count = 0;

    for (phase_idx, phase) in phases.iter().enumerate() {
        let phase_model = create_loop_phase(
            db,
            loop_id,
            &phase.name,
            &phase.spec,
            &phase.acceptance_criteria,
            phase_idx as i32,
        )
        .await?;

        for (link_idx, link) in phase.links.iter().enumerate() {
            let todo_id = create_todo_for_link(db, link, workspace_id, workspace_path).await?;
            let step_model = create_loop_step_for_link(
                db,
                loop_id,
                phase_model.id,
                todo_id,
                link,
                (phase_idx * 1000 + link_idx) as i32,
            )
            .await?;
            template_link_to_step.insert(link.id.clone(), step_model.id);
            step_count += 1;
        }
    }

    Ok((template_link_to_step, phases.len(), step_count))
}

/// 创建 Loop Phase。
async fn create_loop_phase(
    db: &Database,
    loop_id: i64,
    name: &str,
    spec: &str,
    acceptance_criteria: &str,
    order_index: i32,
) -> Result<loop_phases::Model, sea_orm::DbErr> {
    let now = utc_timestamp();
    let am = loop_phases::ActiveModel {
        loop_id: ActiveValue::Set(loop_id),
        name: ActiveValue::Set(name.to_string()),
        description: ActiveValue::Set(String::new()),
        order_index: ActiveValue::Set(order_index),
        spec: ActiveValue::Set(spec.to_string()),
        acceptance_criteria: ActiveValue::Set(acceptance_criteria.to_string()),
        enabled: ActiveValue::Set(1),
        created_at: ActiveValue::Set(Some(now)),
        ..Default::default()
    };
    am.insert(&db.conn).await
}

/// 为环节创建 Todo。
async fn create_todo_for_link(
    db: &Database,
    link: &LinkDefinition,
    workspace_id: i64,
    workspace_path: &str,
) -> Result<i64, InstallError> {
    let (title, prompt, executor, expert_name, acceptance_criteria, model) =
        resolve_link_fields(link);

    let todo_id = db
        .create_todo_with_extras(
            &title,
            &prompt,
            executor.as_deref(),
            acceptance_criteria.as_deref(),
            false,
            workspace_id,
            workspace_path,
        )
        .await?;

    // 如果模板指定了 expert_name / model，同步更新 todo 字段
    if expert_name.is_some() || model.is_some() {
        let _ = db
            .update_todo_expert_and_model(todo_id, expert_name.as_deref(), model.as_deref())
            .await;
    }

    // 工艺环节 review_type="ai" 等价于「执行完成后自动派生评审 todo 打分」，
    // 翻译到 todo.auto_review_enabled；human 则保持 false，由环路闸门暂停等待人工审批。
    // 不穿透这层，todo 会永远停在 create_todo_with_extras 硬编码的 false，
    // 导致工艺里选了 AI 评审却从不触发打分（id=8 即此问题）。
    let _ = db
        .update_todo_auto_review_enabled(todo_id, link.review_type == "ai")
        .await;

    Ok(todo_id)
}

/// 解析 link 的内联执行字段。
/// step_template 已转为 spec 引用（不再查原型表），执行配置完全以内联字段为准：
/// title←name、prompt、executor、expert、验收标准（非空）、model。
fn resolve_link_fields(
    link: &LinkDefinition,
) -> (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    // 验收标准为空时不传入，保持与旧版「无验收标准」语义一致。
    let acceptance_criteria = if link.acceptance_criteria.is_empty() {
        None
    } else {
        Some(link.acceptance_criteria.clone())
    };
    (
        link.name.clone(),
        link.prompt.clone(),
        link.executor.clone(),
        link.expert.clone(),
        acceptance_criteria,
        link.model.clone(),
    )
}

/// 创建 Loop Step。
async fn create_loop_step_for_link(
    db: &Database,
    loop_id: i64,
    phase_id: i64,
    todo_id: i64,
    link: &LinkDefinition,
    order_index: i32,
) -> Result<loop_steps::Model, sea_orm::DbErr> {
    let now = utc_timestamp();
    let expected_artifacts = serde_json::to_string(&link.expected_artifacts)
        .unwrap_or_else(|_| "[]".to_string());
    let gate_config = serde_json::to_string(&link.gates).unwrap_or_else(|_| "[]".to_string());
    let skill_names = serde_json::to_string(&link.skills).unwrap_or_else(|_| "[]".to_string());

    let am = loop_steps::ActiveModel {
        loop_id: ActiveValue::Set(loop_id),
        name: ActiveValue::Set(link.name.clone()),
        description: ActiveValue::Set(String::new()),
        order_index: ActiveValue::Set(order_index),
        todo_id: ActiveValue::Set(todo_id),
        run_mode: ActiveValue::Set("sequential".to_string()),
        skip_on_source_failed: ActiveValue::Set(0),
        min_rating: ActiveValue::Set(None),
        unrated_policy: ActiveValue::Set("skip".to_string()),
        on_success: ActiveValue::Set(link.on_success.clone()),
        success_goto_step_id: ActiveValue::Set(None),
        // `on_gate_fail` 是设计上优先的失败策略（spec §F4.5）。
        // 运行时只有 `on_rating_fail` 一个字段，所以当模板显式设置了 `on_gate_fail`
        // 时（非默认 "break"），把它写入 `on_rating_fail` 作为有效策略。
        // 若 `on_gate_fail` 为默认值，则回退到 `on_rating_fail` 自身的值。
        on_rating_fail: ActiveValue::Set(if link.on_gate_fail != "break" {
            link.on_gate_fail.clone()
        } else {
            link.on_rating_fail.clone()
        }),
        fail_goto_step_id: ActiveValue::Set(None),
        review_type: ActiveValue::Set(link.review_type.clone()),
        // 环节级评审模板正文（需求 033）：空串视为未设置（NULL），评审时回退环路级/默认
        review_prompt: ActiveValue::Set(if link.review_prompt.trim().is_empty() {
            None
        } else {
            Some(link.review_prompt.clone())
        }),
        phase_id: ActiveValue::Set(Some(phase_id)),
        expected_artifacts: ActiveValue::Set(expected_artifacts),
        gate_config: ActiveValue::Set(gate_config),
        max_rework: ActiveValue::Set(link.max_rework),
        skill_names: ActiveValue::Set(skill_names),
        expert_name: ActiveValue::Set(link.expert.clone()),
        enabled: ActiveValue::Set(1),
        created_at: ActiveValue::Set(Some(now)),
        ..Default::default()
    };
    am.insert(&db.conn).await
}

/// 解析并写入所有 goto 目标。
async fn resolve_goto_targets(
    db: &Database,
    loop_id: i64,
    phases: &[PhaseDefinition],
    template_link_to_step: &HashMap<String, i64>,
) -> Result<(), InstallError> {
    let steps = db.list_loop_steps_by_loop(loop_id).await?;
    let step_id_by_order: HashMap<i64, usize> = steps
        .iter()
        .enumerate()
        .map(|(idx, s)| (s.id, idx))
        .collect();

    for phase in phases {
        for link in &phase.links {
            let step_id = *template_link_to_step
                .get(&link.id)
                .ok_or_else(|| InstallError::GotoTargetNotFound(link.id.clone()))?;

            let success_target = resolve_goto(
                &link.on_success,
                link,
                template_link_to_step,
                &step_id_by_order,
                step_id,
            )?;
            let fail_target = resolve_goto(
                &link.on_gate_fail,
                link,
                template_link_to_step,
                &step_id_by_order,
                step_id,
            )
            .or_else(|_| {
                // 未提供 on_gate_fail 时回退到 on_rating_fail
                resolve_goto(
                    &link.on_rating_fail,
                    link,
                    template_link_to_step,
                    &step_id_by_order,
                    step_id,
                )
            })?;

            db.update_loop_step_goto(step_id, success_target, fail_target)
                .await?;
        }
    }

    Ok(())
}

/// 解析单条 goto 策略。
fn resolve_goto(
    policy: &str,
    link: &LinkDefinition,
    template_link_to_step: &HashMap<String, i64>,
    _step_id_by_order: &HashMap<i64, usize>,
    _current_step_id: i64,
) -> Result<Option<i64>, InstallError> {
    match policy {
        "next" => Ok(None),
        "break" | "end" => Ok(None),
        "skip" => Ok(None),
        _ if policy.starts_with("goto:") => {
            let target_link_id = policy.trim_start_matches("goto:");
            let target_step_id = template_link_to_step
                .get(target_link_id)
                .copied()
                .ok_or_else(|| {
                    InstallError::GotoTargetNotFound(format!(
                        "{} -> {}",
                        link.id, target_link_id
                    ))
                })?;
            Ok(Some(target_step_id))
        }
        _ => {
            // 未知策略按 next 处理，不写入具体目标
            tracing::warn!("未知流转策略 '{}', 按 next 处理", policy);
            Ok(None)
        }
    }
}

/// 构建 limits_config JSON。
fn build_limits_config(limits: &super::ProcessLimits) -> String {
    let mut map = serde_json::Map::new();
    if let Some(v) = limits.max_step_executions {
        map.insert(
            "max_step_executions".to_string(),
            serde_json::Value::Number(v.into()),
        );
    }
    if let Some(v) = limits.max_total_tokens {
        map.insert(
            "max_total_tokens".to_string(),
            serde_json::Value::Number(v.into()),
        );
    }
    serde_json::Value::Object(map).to_string()
}

/// 用于序列化 ExpectedArtifact 的辅助：locator 优先取 path，否则取 locator。
impl ExpectedArtifact {
    pub fn locator_string(&self) -> String {
        self.path
            .clone()
            .or_else(|| self.locator.clone())
            .unwrap_or_default()
    }
}

/// 用于序列化 GateDefinition 的辅助：转换为门禁配置 JSON。
impl GateDefinition {
    pub fn to_config_json(&self) -> String {
        let mut map = serde_json::Map::new();
        if let Some(v) = &self.artifact {
            map.insert("artifact".to_string(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &self.criteria_ref {
            map.insert(
                "criteria_ref".to_string(),
                serde_json::Value::String(v.clone()),
            );
        }
        if let Some(v) = self.min_score {
            map.insert(
                "min_score".to_string(),
                serde_json::Value::Number(v.into()),
            );
        }
        if let Some(v) = &self.script {
            map.insert("script".to_string(), serde_json::Value::String(v.clone()));
        }
        serde_json::Value::Object(map).to_string()
    }
}

/// 解析 phase 的 spec_ref / acceptance_criteria_ref 外部引用。
///
/// `bundled://processes/conventions/xxx.md` → 解析为 `~/.ntd/bundled/processes/conventions/xxx.md`，
/// 读取其内容覆盖 `spec` / `acceptance_criteria`。文件不存在时仅 warn，保留 inline 文本。
fn resolve_phase_spec_refs(phases: &mut [PhaseDefinition]) {
    for phase in phases.iter_mut() {
        // 解析 spec_ref
        if let Some(ref spec_ref) = phase.spec_ref {
            match load_bundled_markdown(spec_ref) {
                Ok(content) => phase.spec = content,
                Err(e) => {
                    tracing::warn!(
                        "阶段「{}」spec_ref「{}」加载失败: {}，使用 inline spec",
                        phase.name, spec_ref, e
                    );
                }
            }
        }
        // 解析 acceptance_criteria_ref
        if let Some(ref ac_ref) = phase.acceptance_criteria_ref {
            match load_bundled_markdown(ac_ref) {
                Ok(content) => phase.acceptance_criteria = content,
                Err(e) => {
                    tracing::warn!(
                        "阶段「{}」acceptance_criteria_ref「{}」加载失败: {}，使用 inline 验收标准",
                        phase.name, ac_ref, e
                    );
                }
            }
        }
    }
}

/// 加载 `bundled://` 协议的 markdown 文件。
///
/// 将 `bundled://processes/conventions/xxx.md` 转换为 `~/.ntd/bundled/processes/conventions/xxx.md`。
fn load_bundled_markdown(uri: &str) -> Result<String, String> {
    let path_str = uri
        .strip_prefix("bundled://")
        .ok_or_else(|| format!("不支持的协议: {}", uri))?;
    let home = dirs::home_dir()
        .ok_or_else(|| "无法获取 home 目录".to_string())?;
    let file_path = home.join(".ntd").join("bundled").join(path_str);
    std::fs::read_to_string(&file_path)
        .map_err(|e| format!("读取 {} 失败: {}", file_path.display(), e))
}

/// 升级工艺实例环路到模板最新版本。
///
/// 流程：
/// 1. 解析模板最新定义 YAML
/// 2. 收集当前 Loop 的所有步骤和 todo_id
/// 3. 删除旧步骤和阶段（loop_phases 上 FK CASCADE 自动清理 phase_executions）
///    loop_steps 无 FK 约束到 step_executions，故直接删除，
///    step_executions 记录引用旧 step_id 不会报错（无 FK），保留历史
/// 4. 软删除旧步骤关联的 todo
/// 5. 重新安装阶段/步骤/todo
/// 6. 更新 Loop 的 process_template_version
///
/// 注意：这是一个不可逆的操作——旧步骤的 todo 会被软删除，
/// 但 loop_executions / loop_step_executions 等历史执行数据保留在库中。
pub async fn upgrade_process_template_loop(
    db: &Database,
    template: &process_templates::Model,
    loop_id: i64,
    workspace_id: i64,
    workspace_path: &str,
) -> Result<InstallResult, InstallError> {
    let mut definition: ProcessDefinition = serde_yaml::from_str(&template.definition)?;

    check_skill_warnings(db, &definition.phases).await?;
    resolve_phase_spec_refs(&mut definition.phases);

    // 1. 收集旧步骤信息
    let old_steps = db.list_loop_steps_by_loop(loop_id).await?;
    let old_todo_ids: Vec<i64> = old_steps.iter().map(|s| s.todo_id).collect();
    let conn = db._conn_raw();

    // 删除旧步骤和阶段（无 FK 约束问题，直接删除）
    // loop_phases 的 ON DELETE CASCADE 会自动清理 loop_phase_executions
    loop_steps::Entity::delete_many()
        .filter(loop_steps::Column::LoopId.eq(loop_id))
        .exec(conn)
        .await?;
    loop_phases::Entity::delete_many()
        .filter(loop_phases::Column::LoopId.eq(loop_id))
        .exec(conn)
        .await?;

    // 软删除旧步骤关联的 todo（通过设置 deleted_at）
    for todo_id in &old_todo_ids {
        db.delete_todo(*todo_id).await?;
    }

    // 2. 获取 Loop 现有配置
    let loop_model = db.get_loop(loop_id).await?
        .ok_or_else(|| InstallError::DbError(sea_orm::DbErr::Custom(format!("Loop {loop_id} not found"))))?;

    let limits_config = build_limits_config(&definition.limits);
    let abnormal_handler_trigger_on = definition
        .abnormal_handler
        .as_ref()
        .map(|h| serde_json::to_string(&h.trigger_on).unwrap_or_else(|_| "[]".to_string()))
        .unwrap_or_else(|| loop_model.abnormal_handler_trigger_on.clone());

    // 使用模板最新定义的 display_name
    let loop_name = if definition.process.display_name.is_empty() {
        definition.process.name.clone()
    } else {
        definition.process.display_name.clone()
    };

    // 更新 Loop：名称、描述、限制配置（保留原有 icon/review_template 等配置）
    db.update_loop(
        loop_id,
        &loop_name,
        &definition.process.description,
        Some(workspace_id),
        None, /* workspace_path 保留旧路径，不覆盖 */
        loop_model.webhook_enabled,
        &loop_model.icon,
        loop_model.review_template_id,
        Some(&limits_config),
        loop_model.abnormal_handler_todo_id,
        &abnormal_handler_trigger_on,
    ).await?;

    // 单独更新 process_template_version（update_loop 不覆盖该字段）
    let now = utc_timestamp();
    let existing = loops::Entity::find_by_id(loop_id).one(conn).await?;
    if let Some(c) = existing {
        let mut am: loops::ActiveModel = c.into();
        am.process_template_version = ActiveValue::Set(Some(template.version.clone()));
        am.updated_at = ActiveValue::Set(Some(now));
        am.update(conn).await?;
    }

    // 3. 重新创建阶段和步骤（复用现有逻辑）
    let (template_link_to_step, phase_count, step_count) = create_phases_and_steps(
        db,
        loop_id,
        workspace_id,
        workspace_path,
        &definition.phases,
    ).await?;

    // 4. 解析 goto 目标
    resolve_goto_targets(db, loop_id, &definition.phases, &template_link_to_step).await?;

    Ok(InstallResult {
        loop_id,
        loop_name,
        phase_count,
        step_count,
    })
}

/// 校验环节引用的 skill 名称。
/// step_template 已转为 spec 引用（不再是原型名），不再做存在性校验；
/// skill 是 executor 级别的文件注入，仅记 warn，不阻断安装。
async fn check_skill_warnings(
    db: &Database,
    phases: &[PhaseDefinition],
) -> Result<(), InstallError> {
    for phase in phases {
        for link in &phase.links {
            // skill 名称仅 warn，不阻断安装。
            for skill_name in &link.skills {
                if db.get_process_step_template_by_name(skill_name).await?.is_none() {
                    tracing::warn!(
                        "工艺模板安装：环节「{}」引用的 skill「{}」在当前 bundled 仓库中未找到对应环节原型，将在运行时按名称注入",
                        link.name, skill_name
                    );
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_arguments
)]
mod installer_tests {
    use super::*;

    async fn fresh_db() -> Database {
        Database::new(":memory:")
            .await
            .expect(":memory: db must open")
    }

    /// 构造一个最小可运行工艺模板定义 YAML。
    fn sample_process_definition_yaml() -> String {
        r#"
process:
  name: test-delivery
  display_name: 测试交付工艺
  description: 用于单元测试
  category: test
  complexity: light
  version: 0.1.0
limits:
  max_step_executions: 20
  max_total_tokens: 100000
phases:
  - id: req
    name: 需求
    spec: 需求阶段
    acceptance_criteria: PRD 存在
    links:
      - id: write-prd
        name: 编写 PRD
        step_template: []
        prompt: 请编写 PRD
        on_success: next
        on_gate_fail: goto:write-prd
        max_rework: 2
      - id: confirm-prd
        name: 确认 PRD
        prompt: 请确认 PRD
        review_type: human
        on_success: next
        on_gate_fail: goto:write-prd
"#
        .to_string()
    }

    async fn seed_step_template(db: &Database) {
        db.upsert_system_process_step_template(
            "write-prd",
            "编写 PRD",
            "请编写 PRD",
            Some("claudecode"),
            Some("product-manager"),
            "[\"4p12s-prd\"]",
            Some("claude-sonnet-5"),
            "PRD.md 存在",
            "bundled://processes/step-templates/write-prd.yaml",
            "general",
        )
        .await
        .unwrap();
    }

    async fn seed_workspace(db: &Database) -> i64 {
        db.create_project_directory("/tmp/test-ws", Some("测试空间"), false, false)
            .await
            .unwrap()
    }

    /// 校验：环节级 review_prompt 写入 loop_steps（需求 033）。
    /// 有 review_prompt 的环节写入正文（含占位符），无的为 NULL。
    #[tokio::test]
    async fn test_install_writes_review_prompt() {
        let db = fresh_db().await;
        let ws_id = seed_workspace(&db).await;
        // 构造含环节级 review_prompt 的工艺 yaml：一个环节有、一个没有
        let yaml = r#"
process:
  name: review-prompt-test
  display_name: 评审模板测试
  version: 0.1.0
phases:
  - id: p1
    name: 阶段一
    links:
      - id: with-rp
        name: 有评审模板
        prompt: 执行
        review_prompt: |
          你是严格评审。输出：{original_output}
          最后输出 RATING: 0-100
      - id: without-rp
        name: 无评审模板
        prompt: 执行
"#;
        let template = process_templates::ActiveModel {
            name: ActiveValue::Set("review-prompt-test".to_string()),
            display_name: ActiveValue::Set("评审模板测试".to_string()),
            description: ActiveValue::Set(String::new()),
            category: ActiveValue::Set("test".to_string()),
            complexity: ActiveValue::Set("light".to_string()),
            version: ActiveValue::Set("0.1.0".to_string()),
            definition: ActiveValue::Set(yaml.to_string()),
            source_path: ActiveValue::Set(Some("bundled://review-prompt-test.yaml".to_string())),
            is_system: ActiveValue::Set(true),
            ..Default::default()
        }
        .insert(&db.conn)
        .await
        .unwrap();

        let result = install_process_template(&db, &template, ws_id, "/tmp/test-ws")
            .await
            .expect("install should succeed");

        let steps = db.list_loop_steps_by_loop(result.loop_id).await.unwrap();
        // 有 review_prompt 的环节：正文写入，占位符原样保留
        let with_rp = steps
            .iter()
            .find(|s| s.name == "有评审模板")
            .expect("应有「有评审模板」环节");
        let rp = with_rp
            .review_prompt
            .as_ref()
            .expect("review_prompt 应已写入");
        assert!(rp.contains("你是严格评审"), "review_prompt 正文应保留");
        assert!(rp.contains("{original_output}"), "占位符应原样保留");
        // 无 review_prompt 的环节：NULL（评审时回退环路级/默认）
        let without_rp = steps
            .iter()
            .find(|s| s.name == "无评审模板")
            .expect("应有「无评审模板」环节");
        assert!(
            without_rp.review_prompt.is_none(),
            "未设置 review_prompt 应为 NULL"
        );
    }

    #[tokio::test]
    async fn test_install_process_template_creates_loop_phases_steps() {
        let db = fresh_db().await;
        seed_step_template(&db).await;
        let ws_id = seed_workspace(&db).await;

        let template = process_templates::ActiveModel {
            name: ActiveValue::Set("test-delivery".to_string()),
            display_name: ActiveValue::Set("测试交付工艺".to_string()),
            description: ActiveValue::Set("用于单元测试".to_string()),
            category: ActiveValue::Set("test".to_string()),
            complexity: ActiveValue::Set("light".to_string()),
            version: ActiveValue::Set("0.1.0".to_string()),
            definition: ActiveValue::Set(sample_process_definition_yaml()),
            source_path: ActiveValue::Set(Some("bundled://processes/test-delivery.yaml".to_string())),
            is_system: ActiveValue::Set(true),
            ..Default::default()
        }
        .insert(&db.conn)
        .await
        .unwrap();

        let result = install_process_template(&db, &template, ws_id, "/tmp/test-ws",
        )
        .await
        .expect("install should succeed");

        assert_eq!(result.loop_name, "测试交付工艺");
        assert_eq!(result.phase_count, 1);
        assert_eq!(result.step_count, 2);

        let loop_model = db.get_loop(result.loop_id).await.unwrap().unwrap();
        assert_eq!(loop_model.process_template_id, Some(template.id));
        assert_eq!(loop_model.process_template_version, Some("0.1.0".to_string()));

        let phases = db
            .list_loop_phases_by_loop(result.loop_id)
            .await
            .unwrap();
        assert_eq!(phases.len(), 1);
        assert_eq!(phases[0].name, "需求");

        let steps = db.list_loop_steps_by_loop(result.loop_id).await.unwrap();
        assert_eq!(steps.len(), 2);
        assert!(steps.iter().all(|s| s.phase_id == Some(phases[0].id)));

        // 验证 goto 目标已解析：confirm-prd 失败时回退到 write-prd
        let confirm_step = steps
            .iter()
            .find(|s| s.name == "确认 PRD")
            .expect("confirm step exists");
        let write_step = steps
            .iter()
            .find(|s| s.name == "编写 PRD")
            .expect("write step exists");
        assert_eq!(confirm_step.fail_goto_step_id, Some(write_step.id));
        // 验证 on_gate_fail 策略已正确写入 on_rating_fail（而非默认 "break"）。
        // spec §F4.5：门禁失败时按 on_gate_fail 策略流转。
        assert_eq!(confirm_step.on_rating_fail, "goto:write-prd");
    }

    #[tokio::test]
    async fn test_install_inlined_link_without_step_template_lookup() {
        // step_template 已转为 spec 引用，install 不再查原型表；
        // link 内联 prompt 时即使原型表无对应记录也能安装成功。
        let db = fresh_db().await;
        let ws_id = seed_workspace(&db).await;

        let template = process_templates::ActiveModel {
            name: ActiveValue::Set("inlined-link".to_string()),
            display_name: ActiveValue::Set("内联环节".to_string()),
            description: ActiveValue::Set("".to_string()),
            category: ActiveValue::Set("test".to_string()),
            complexity: ActiveValue::Set("light".to_string()),
            version: ActiveValue::Set("1.0.0".to_string()),
            definition: ActiveValue::Set(
                "process:\n  name: inlined-link\nphases:\n  - id: p1\n    name: p1\n    links:\n      - id: l1\n        name: l1\n        step_template: []\n        prompt: 请实现 l1\n        on_success: end\n        on_gate_fail: break\n"
                    .to_string(),
            ),
            is_system: ActiveValue::Set(true),
            ..Default::default()
        }
        .insert(&db.conn)
        .await
        .unwrap();

        let result = install_process_template(&db, &template, ws_id, "/tmp/test-ws",
        )
        .await
        .expect("内联配置不应依赖原型表，install 应成功");
        assert_eq!(result.step_count, 1);
    }

    /// installer 应把 link.review_type 翻译到 todo.auto_review_enabled：
    /// 默认 ai -> true，human -> false。
    /// 修复「工艺选了 AI 评审却从不触发打分」--此前 create_todo_with_extras 硬编码 false。
    #[tokio::test]
    async fn test_install_link_review_type_enables_auto_review() {
        let db = fresh_db().await;
        seed_step_template(&db).await;
        let ws_id = seed_workspace(&db).await;

        let template = process_templates::ActiveModel {
            name: ActiveValue::Set("review-passthrough".to_string()),
            display_name: ActiveValue::Set("评审穿透".to_string()),
            description: ActiveValue::Set("".to_string()),
            category: ActiveValue::Set("test".to_string()),
            complexity: ActiveValue::Set("light".to_string()),
            version: ActiveValue::Set("0.1.0".to_string()),
            definition: ActiveValue::Set(sample_process_definition_yaml()),
            source_path: ActiveValue::Set(Some("bundled://processes/test.yaml".to_string())),
            is_system: ActiveValue::Set(true),
            ..Default::default()
        }
        .insert(&db.conn)
        .await
        .unwrap();

        let result = install_process_template(&db, &template, ws_id, "/tmp/test-ws")
            .await
            .expect("install should succeed");

        let steps = db.list_loop_steps_by_loop(result.loop_id).await.unwrap();
        // write-prd: review_type 默认 ai -> todo.auto_review_enabled=true
        let write_step = steps
            .iter()
            .find(|s| s.name == "编写 PRD")
            .expect("write-prd exists");
        let write_todo = db.get_todo(write_step.todo_id).await.unwrap().unwrap();
        assert_eq!(write_step.review_type, "ai");
        assert!(
            write_todo.auto_review_enabled,
            "review_type=ai 的环节 todo 必须自动开启 auto_review_enabled"
        );
        // confirm-prd: review_type=human -> auto_review_enabled=false
        let confirm_step = steps
            .iter()
            .find(|s| s.name == "确认 PRD")
            .expect("confirm-prd exists");
        let confirm_todo = db.get_todo(confirm_step.todo_id).await.unwrap().unwrap();
        assert_eq!(confirm_step.review_type, "human");
        assert!(
            !confirm_todo.auto_review_enabled,
            "review_type=human 的环节 todo 不应开启 auto_review_enabled"
        );
    }
}
