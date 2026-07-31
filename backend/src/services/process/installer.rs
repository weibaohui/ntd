//! 工艺模板实例化：把 `process_templates` 中的 YAML 定义转换为可执行的 Loop。

use std::collections::HashMap;

use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter};

use crate::db::entity::{loop_phases, loop_steps, loops, process_templates};
use crate::db::Database;
use crate::models::utc_timestamp;

use super::{
    AbnormalHandlerConfig, ExpectedArtifact, GateDefinition, InstallError, InstallResult,
    LinkDefinition, PhaseDefinition, ProcessDefinition,
};

/// 安装工艺模板到指定工作空间，返回生成的 Loop ID。
///
/// 流程：
/// 1. 解析传入的工艺定义 `definition`（YAML，由调用方按 source_path 从磁盘读取）
/// 2. 创建 Loop（status=paused，记录来源模板与版本快照）
/// 3. 创建 Phase
/// 4. 第一遍：为每个 link 创建 todo + loop_step（goto 暂空）
/// 5. 第二遍：解析 `goto:<link_id>`，把模板 link id 映射到真实 loop_step id
pub async fn install_process_template(
    db: &Database,
    template: &process_templates::Model,
    definition: &str,
    workspace_id: i64,
    workspace_path: &str,
) -> Result<InstallResult, InstallError> {
    // 工艺正文由调用方从磁盘文件读取后传入；此处只负责解析，不再触碰 DB 的 definition。
    let mut definition: ProcessDefinition = serde_yaml::from_str(definition)?;

    // 解析 spec_ref 外部引用，覆盖 inline spec 文本。（阶段级 acceptance_criteria_ref 已随需求 036 移除。）
    resolve_phase_spec_refs(&mut definition.phases);

    let loop_name = if definition.process.display_name.is_empty() {
        definition.process.name.clone()
    } else {
        definition.process.display_name.clone()
    };

    let limits_config = build_limits_config(&definition.limits);
    let abnormal_handler_trigger_on = build_abnormal_trigger_on(&definition.abnormal_handler);
    // 异常处理载体 Todo：prompt 非空时创建，写入 loop 的 todo_id + prompt 快照列
    let (abnormal_handler_todo_id, abnormal_handler_prompt) =
        ensure_abnormal_handler_todo(db, &definition.abnormal_handler, workspace_id, &loop_name)
            .await?;

    let loop_model = create_loop_from_template(
        db,
        &loop_name,
        &definition.process.description,
        workspace_id,
        workspace_path,
        &limits_config,
        &abnormal_handler_trigger_on,
        abnormal_handler_todo_id,
        abnormal_handler_prompt.as_deref(),
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
    abnormal_handler_todo_id: Option<i64>,
    abnormal_handler_prompt: Option<&str>,
    process_template_id: i64,
    process_template_version: &str,
) -> Result<loops::Model, sea_orm::DbErr> {
    let now = utc_timestamp();
    // 044：loops 已移除 webhook_enabled/icon/review_template_id/color，安装时不再写入。
    let am = loops::ActiveModel {
        name: ActiveValue::Set(name.to_string()),
        description: ActiveValue::Set(description.to_string()),
        workspace_id: ActiveValue::Set(Some(workspace_id)),
        workspace_path: ActiveValue::Set(Some(workspace_path.to_string())),
        status: ActiveValue::Set("paused".to_string()),
        limits_config: ActiveValue::Set(limits_config.to_string()),
        abnormal_handler_trigger_on: ActiveValue::Set(abnormal_handler_trigger_on.to_string()),
        // 异常处理：todo_id 指向安装时创建的载体 Todo，prompt 为工艺定义的只读快照
        abnormal_handler_todo_id: ActiveValue::Set(abnormal_handler_todo_id),
        abnormal_handler_prompt: ActiveValue::Set(abnormal_handler_prompt.map(|s| s.to_string())),
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
///
/// 阶段级验收标准已随需求 036 移除——验收标准只归环节，故此处不再写入 acceptance_criteria。
async fn create_loop_phase(
    db: &Database,
    loop_id: i64,
    name: &str,
    spec: &str,
    order_index: i32,
) -> Result<loop_phases::Model, sea_orm::DbErr> {
    let now = utc_timestamp();
    let am = loop_phases::ActiveModel {
        loop_id: ActiveValue::Set(loop_id),
        name: ActiveValue::Set(name.to_string()),
        description: ActiveValue::Set(String::new()),
        order_index: ActiveValue::Set(order_index),
        spec: ActiveValue::Set(spec.to_string()),
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

    // 048：评审是否启用由 gate_config 决定——环节含 ai_criteria_review 门禁才需要 auto_review 打分。
    // 不穿透这层，todo 会停在 create_todo_with_extras 硬编码的 false，AI 评审门禁拿不到评分。
    let has_ai_review_gate = link.gates.iter().any(|g| g.gate_type == "ai_criteria_review");
    let _ = db
        .update_todo_auto_review_enabled(todo_id, has_ai_review_gate)
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
    // 环节 spec 模板引用（需求 054）：序列化为 JSON 数组串，供执行器执行时注入 prompt。
    // as_deref 借用避免 move（link 是共享引用）；None 时 unwrap_or_default 给空切片。
    let step_template_refs = serde_json::to_string(link.step_template.as_deref().unwrap_or_default())
        .unwrap_or_else(|_| "[]".to_string());
    let gate_config = serde_json::to_string(&link.gates).unwrap_or_else(|_| "[]".to_string());
    let skill_names = serde_json::to_string(&link.skills).unwrap_or_else(|_| "[]".to_string());

    let am = loop_steps::ActiveModel {
        loop_id: ActiveValue::Set(loop_id),
        name: ActiveValue::Set(link.name.clone()),
        description: ActiveValue::Set(String::new()),
        order_index: ActiveValue::Set(order_index),
        todo_id: ActiveValue::Set(todo_id),
        // 044：loop_steps 已移除 run_mode/skip_on_source_failed/min_rating/unrated_policy，
        // 评审与流转改由 gate_config + on_success/on_rating_fail 表达。
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
        // 环节级评审模板正文（需求 033）：空串视为未设置（NULL），评审时回退环路级/默认
        review_prompt: ActiveValue::Set(if link.review_prompt.trim().is_empty() {
            None
        } else {
            Some(link.review_prompt.clone())
        }),
        phase_id: ActiveValue::Set(Some(phase_id)),
        expected_artifacts: ActiveValue::Set(expected_artifacts),
        step_template_refs: ActiveValue::Set(step_template_refs),
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
    // 流转策略规则（需求 037：已清除 goto: 前缀）：
    // - 保留字 next/end/break/skip → 无跳转目标（Ok(None)）；
    // - 其他非空值 = 跳转目标环节 id（裸），查模板 link→step 映射解析成数字 step id。
    match policy {
        "next" | "end" | "break" | "skip" => Ok(None),
        target_link_id => {
            // 非保留字即跳转目标环节 id。找不到说明 yaml 引用了不存在的环节，
            // 按错误中断安装（不再容错为 next——裸 id 语义已明确，引用错误应暴露）。
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
    }
}

/// 序列化异常处理触发条件；工艺未声明 abnormal_handler 时回退默认三态全选。
fn build_abnormal_trigger_on(config: &Option<AbnormalHandlerConfig>) -> String {
    config
        .as_ref()
        .map(|h| serde_json::to_string(&h.trigger_on).unwrap_or_else(|_| "[]".to_string()))
        .unwrap_or_else(|| "[\"capped_step\",\"capped_token\",\"failed\"]".to_string())
}

/// 工艺安装时按 abnormal_handler.prompt 创建异常处理载体 Todo。
/// prompt 为空（或未配置）时返回 (None, None)，不创建载体 Todo。
async fn ensure_abnormal_handler_todo(
    db: &Database,
    config: &Option<AbnormalHandlerConfig>,
    workspace_id: i64,
    loop_display_name: &str,
) -> Result<(Option<i64>, Option<String>), InstallError> {
    let prompt = config
        .as_ref()
        .and_then(|c| c.prompt.as_ref())
        .filter(|p| !p.trim().is_empty())
        .cloned();
    let Some(prompt) = prompt else {
        return Ok((None, None));
    };
    let title = format!("[异常处理] {}", loop_display_name);
    let todo_id = db
        .create_abnormal_handler_todo(title, prompt.clone(), workspace_id)
        .await
        .map_err(InstallError::DbError)?;
    Ok((Some(todo_id), Some(prompt)))
}

/// 工艺升级时同步异常处理载体 Todo：
/// - prompt 非空：旧载体 Todo 仍存在则原地更新 prompt（保持 todo_id 稳定，历史执行记录引用不破），否则新建
/// - prompt 空：不删旧载体 Todo，loop 列置空（返回 None），该 loop 不再触发异常处理
async fn handle_upgrade_abnormal_handler(
    db: &Database,
    config: &Option<AbnormalHandlerConfig>,
    existing_todo_id: Option<i64>,
    workspace_id: i64,
    loop_display_name: &str,
) -> Result<(Option<i64>, Option<String>), InstallError> {
    let prompt = config
        .as_ref()
        .and_then(|c| c.prompt.as_ref())
        .filter(|p| !p.trim().is_empty())
        .cloned();
    let Some(prompt) = prompt else {
        // 新版无异常处理 prompt：保留旧载体 Todo 不删，仅 loop 列置空
        return Ok((None, None));
    };
    // 旧载体 Todo 仍存在 → 原地更新 prompt
    if let Some(old_id) = existing_todo_id {
        if db
            .get_todo(old_id)
            .await
            .map_err(InstallError::DbError)?
            .is_some()
        {
            db.update_todo_prompt(old_id, &prompt)
                .await
                .map_err(InstallError::DbError)?;
            return Ok((Some(old_id), Some(prompt)));
        }
    }
    // 旧载体 Todo 不存在或从未创建 → 新建
    let title = format!("[异常处理] {}", loop_display_name);
    let todo_id = db
        .create_abnormal_handler_todo(title, prompt.clone(), workspace_id)
        .await
        .map_err(InstallError::DbError)?;
    Ok((Some(todo_id), Some(prompt)))
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

/// 解析 phase 的 spec_ref 外部引用。
///
/// `bundled://processes/conventions/xxx.md` → 解析为 `~/.ntd/bundled/processes/conventions/xxx.md`，
/// 读取其内容覆盖 `spec`。文件不存在时仅 warn，保留 inline 文本。
/// （阶段级 acceptance_criteria_ref 已随需求 036 移除，验收标准只归环节，不再在此解析。）
fn resolve_phase_spec_refs(phases: &mut [PhaseDefinition]) {
    for phase in phases.iter_mut() {
        // 解析 spec_ref
        if let Some(ref spec_ref) = phase.spec_ref {
            match super::source::read_bundled_markdown(spec_ref) {
                Ok(content) => phase.spec = content,
                Err(e) => {
                    tracing::warn!(
                        "阶段「{}」spec_ref「{}」加载失败: {}，使用 inline spec",
                        phase.name, spec_ref, e
                    );
                }
            }
        }
    }
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
    definition: &str,
    loop_id: i64,
    workspace_id: i64,
    workspace_path: &str,
) -> Result<InstallResult, InstallError> {
    // 工艺正文由调用方按 source_path 从磁盘读取后传入，这里只负责解析，不再读 DB 的 definition。
    let mut definition: ProcessDefinition = serde_yaml::from_str(definition)?;

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
    // 工艺显式声明 abnormal_handler 时取其 trigger_on；未声明则保留 loop 原有值
    let abnormal_handler_trigger_on = match &definition.abnormal_handler {
        Some(h) => serde_json::to_string(&h.trigger_on).unwrap_or_else(|_| "[]".to_string()),
        None => loop_model.abnormal_handler_trigger_on.clone(),
    };

    // 使用模板最新定义的 display_name
    let loop_name = if definition.process.display_name.is_empty() {
        definition.process.name.clone()
    } else {
        definition.process.display_name.clone()
    };

    // 同步异常处理载体 Todo：prompt 变化时更新/新建，prompt 清空则 loop 列置空（不删旧 Todo）
    // 放在 loop_name 之后：载体 Todo 标题需引用 loop_name
    let (abnormal_handler_todo_id, abnormal_handler_prompt) = handle_upgrade_abnormal_handler(
        db,
        &definition.abnormal_handler,
        loop_model.abnormal_handler_todo_id,
        workspace_id,
        &loop_name,
    )
    .await?;

    // 更新 Loop：名称、描述、限制配置（044 后 loops 仅保留这些可更新字段）
    db.update_loop(
        loop_id,
        &loop_name,
        &definition.process.description,
        Some(workspace_id),
        None, /* workspace_path 保留旧路径，不覆盖 */
        Some(&limits_config),
        abnormal_handler_todo_id,
        &abnormal_handler_trigger_on,
    ).await?;

    // 单独更新 process_template_version + abnormal_handler_prompt（update_loop 不覆盖这两列）
    let now = utc_timestamp();
    let existing = loops::Entity::find_by_id(loop_id).one(conn).await?;
    if let Some(c) = existing {
        let mut am: loops::ActiveModel = c.into();
        am.process_template_version = ActiveValue::Set(Some(template.version.clone()));
        am.abnormal_handler_prompt = ActiveValue::Set(abnormal_handler_prompt.clone());
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
    links:
      - id: write-prd
        name: 编写 PRD
        step_template: []
        prompt: 请编写 PRD
        on_success: next
        on_gate_fail: write-prd
        max_rework: 2
        gates:
          - name: AI评审
            type: ai_criteria_review
      - id: confirm-prd
        name: 确认 PRD
        prompt: 请确认 PRD
        gates:
          - name: 人工审批
            type: human_approval
        on_success: next
        on_gate_fail: write-prd
"#
        .to_string()
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
          你是严格评审。输出：{{original_output}}
          最后输出 RATING: 0-100
      - id: without-rp
        name: 无评审模板
        prompt: 执行
"#;
        let template = process_templates::ActiveModel {
            guid: ActiveValue::Set("guid-review-prompt-test".to_string()),
            name: ActiveValue::Set("review-prompt-test".to_string()),
            display_name: ActiveValue::Set("评审模板测试".to_string()),
            description: ActiveValue::Set(String::new()),
            category: ActiveValue::Set("test".to_string()),
            complexity: ActiveValue::Set("light".to_string()),
            version: ActiveValue::Set("0.1.0".to_string()),
            source_path: ActiveValue::Set(Some("bundled://review-prompt-test.yaml".to_string())),
            is_system: ActiveValue::Set(true),
            ..Default::default()
        }
        .insert(&db.conn)
        .await
        .unwrap();

        // 正文由调用方按 source_path 从磁盘读取后传入；测试里直接用示例 YAML 字符串。
        let result = install_process_template(&db, &template, yaml, ws_id, "/tmp/test-ws")
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
        assert!(rp.contains("{{original_output}}"), "占位符应原样保留");
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
        let ws_id = seed_workspace(&db).await;

        let template = process_templates::ActiveModel {
            guid: ActiveValue::Set("guid-test-delivery".to_string()),
            name: ActiveValue::Set("test-delivery".to_string()),
            display_name: ActiveValue::Set("测试交付工艺".to_string()),
            description: ActiveValue::Set("用于单元测试".to_string()),
            category: ActiveValue::Set("test".to_string()),
            complexity: ActiveValue::Set("light".to_string()),
            version: ActiveValue::Set("0.1.0".to_string()),
            source_path: ActiveValue::Set(Some("bundled://processes/test-delivery.yaml".to_string())),
            is_system: ActiveValue::Set(true),
            ..Default::default()
        }
        .insert(&db.conn)
        .await
        .unwrap();

        // 正文由调用方按 source_path 从磁盘读取后传入；测试里直接用示例 YAML 字符串。
        let result = install_process_template(&db, &template, &sample_process_definition_yaml(), ws_id, "/tmp/test-ws",
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
        assert_eq!(confirm_step.on_rating_fail, "write-prd");
    }

    /// 含非空 step_template 的最小工艺 YAML（需求 054 夹具）；spec 名可参数化以便升级用例换值。
    fn sample_yaml_with_step_template(spec_name: &str) -> String {
        format!(
            r#"
process:
  name: tpl-refs-test
  display_name: spec 引用落库测试
  version: 0.1.0
phases:
  - id: p1
    name: 阶段一
    links:
      - id: l1
        name: 环节一
        step_template:
          - name: {spec_name}
            path: bundled://processes/conventions/x.md
        prompt: 执行任务
        on_success: end
        on_gate_fail: break
"#
        )
    }

    /// 校验 G1：非空 step_template 随安装落库到 loop_steps.step_template_refs。
    /// 既有安装测试全用空数组，该写入路径此前无任何断言覆盖。
    #[tokio::test]
    async fn test_install_persists_step_template_refs() {
        let db = fresh_db().await;
        let ws_id = seed_workspace(&db).await;
        let template = process_templates::ActiveModel {
            guid: ActiveValue::Set("guid-tpl-refs".to_string()),
            name: ActiveValue::Set("tpl-refs-test".to_string()),
            display_name: ActiveValue::Set("spec 引用落库测试".to_string()),
            description: ActiveValue::Set(String::new()),
            category: ActiveValue::Set("test".to_string()),
            complexity: ActiveValue::Set("light".to_string()),
            version: ActiveValue::Set("0.1.0".to_string()),
            source_path: ActiveValue::Set(Some("bundled://tpl-refs.yaml".to_string())),
            is_system: ActiveValue::Set(true),
            ..Default::default()
        }
        .insert(&db.conn)
        .await
        .unwrap();

        let result = install_process_template(
            &db,
            &template,
            &sample_yaml_with_step_template("需求规范"),
            ws_id,
            "/tmp/test-ws",
        )
        .await
        .expect("install should succeed");

        let steps = db.list_loop_steps_by_loop(result.loop_id).await.unwrap();
        assert_eq!(steps.len(), 1, "应只有一个环节");
        // 解析回 JSON 逐字段比对，避免依赖序列化的键序/空格。
        let refs: serde_json::Value = serde_json::from_str(&steps[0].step_template_refs)
            .expect("step_template_refs 应为合法 JSON 数组");
        assert_eq!(refs[0]["name"], "需求规范", "落库 spec 名应与 YAML 一致");
        assert_eq!(
            refs[0]["path"], "bundled://processes/conventions/x.md",
            "落库 spec 路径应与 YAML 一致"
        );
    }

    /// 校验验收标准「升级工艺后新列同步重建」：升级换了 spec 引用的 YAML，
    /// 重建的环节必须带新值而不是残留旧值。
    #[tokio::test]
    async fn test_upgrade_rebuilds_step_template_refs() {
        let db = fresh_db().await;
        let ws_id = seed_workspace(&db).await;
        let template = process_templates::ActiveModel {
            guid: ActiveValue::Set("guid-tpl-refs-upg".to_string()),
            name: ActiveValue::Set("tpl-refs-test".to_string()),
            display_name: ActiveValue::Set("spec 引用落库测试".to_string()),
            description: ActiveValue::Set(String::new()),
            category: ActiveValue::Set("test".to_string()),
            complexity: ActiveValue::Set("light".to_string()),
            version: ActiveValue::Set("0.2.0".to_string()),
            source_path: ActiveValue::Set(Some("bundled://tpl-refs.yaml".to_string())),
            is_system: ActiveValue::Set(true),
            ..Default::default()
        }
        .insert(&db.conn)
        .await
        .unwrap();

        let installed = install_process_template(
            &db,
            &template,
            &sample_yaml_with_step_template("旧规范"),
            ws_id,
            "/tmp/test-ws",
        )
        .await
        .expect("install should succeed");

        let upgraded = upgrade_process_template_loop(
            &db,
            &template,
            &sample_yaml_with_step_template("新规范"),
            installed.loop_id,
            ws_id,
            "/tmp/test-ws",
        )
        .await
        .expect("upgrade should succeed");

        let steps = db.list_loop_steps_by_loop(upgraded.loop_id).await.unwrap();
        assert_eq!(steps.len(), 1, "升级后应重建为 1 个环节");
        let refs: serde_json::Value = serde_json::from_str(&steps[0].step_template_refs)
            .expect("升级后 step_template_refs 应为合法 JSON");
        assert_eq!(refs[0]["name"], "新规范", "升级后 spec 引用应同步为新 YAML 的值");
    }

    #[tokio::test]
    async fn test_install_inlined_link_without_step_template_lookup() {
        // step_template 已转为 spec 引用，install 不再查原型表；
        // link 内联 prompt 时即使原型表无对应记录也能安装成功。
        let db = fresh_db().await;
        let ws_id = seed_workspace(&db).await;

        // 缺失环节原型的工艺定义（正文不再存 DB，测试里直接作为入参传入）。
        // step_template 在 feat/recipe-editor 已改为序列类型，空序列 + 内联 prompt 即表示「不查原型表」。
        let missing_step_def = "process:\n  name: missing-step\nphases:\n  - id: p1\n    name: p1\n    links:\n      - id: l1\n        name: l1\n        step_template: []\n        prompt: 请实现 l1\n        on_success: end\n        on_gate_fail: break\n".to_string();
        let template = process_templates::ActiveModel {
            guid: ActiveValue::Set("guid-inlined-link".to_string()),
            name: ActiveValue::Set("inlined-link".to_string()),
            display_name: ActiveValue::Set("内联环节".to_string()),
            description: ActiveValue::Set("".to_string()),
            category: ActiveValue::Set("test".to_string()),
            complexity: ActiveValue::Set("light".to_string()),
            version: ActiveValue::Set("1.0.0".to_string()),
            source_path: ActiveValue::Set(Some("bundled://processes/missing-step.yaml".to_string())),
            is_system: ActiveValue::Set(true),
            ..Default::default()
        }
        .insert(&db.conn)
        .await
        .unwrap();

        let result = install_process_template(&db, &template, &missing_step_def, ws_id, "/tmp/test-ws",
        )
        .await
        .expect("内联配置不应依赖原型表，install 应成功");
        assert_eq!(result.step_count, 1);
    }

    /// installer 应按 gate_config 推导 todo.auto_review_enabled：
    /// 含 ai_criteria_review 门禁 -> true；仅 human_approval -> false。
    /// 048 起 review_type 字段废弃，评审启用改由 gate_config 决定。
    #[tokio::test]
    async fn test_install_link_review_type_enables_auto_review() {
        let db = fresh_db().await;
        let ws_id = seed_workspace(&db).await;

        let template = process_templates::ActiveModel {
            guid: ActiveValue::Set("guid-review-passthrough".to_string()),
            name: ActiveValue::Set("review-passthrough".to_string()),
            display_name: ActiveValue::Set("评审穿透".to_string()),
            description: ActiveValue::Set("".to_string()),
            category: ActiveValue::Set("test".to_string()),
            complexity: ActiveValue::Set("light".to_string()),
            version: ActiveValue::Set("0.1.0".to_string()),
            source_path: ActiveValue::Set(Some("bundled://processes/test.yaml".to_string())),
            is_system: ActiveValue::Set(true),
            ..Default::default()
        }
        .insert(&db.conn)
        .await
        .unwrap();

        let result = install_process_template(&db, &template, &sample_process_definition_yaml(), ws_id, "/tmp/test-ws")
            .await
            .expect("install should succeed");

        let steps = db.list_loop_steps_by_loop(result.loop_id).await.unwrap();
        // write-prd: 含 ai_criteria_review 门禁 -> todo.auto_review_enabled=true
        let write_step = steps
            .iter()
            .find(|s| s.name == "编写 PRD")
            .expect("write-prd exists");
        let write_todo = db.get_todo(write_step.todo_id).await.unwrap().unwrap();
        assert!(
            write_todo.auto_review_enabled,
            "含 ai_criteria_review 门禁的环节 todo 必须自动开启 auto_review_enabled"
        );
        // confirm-prd: 仅 human_approval 门禁 -> auto_review_enabled=false
        let confirm_step = steps
            .iter()
            .find(|s| s.name == "确认 PRD")
            .expect("confirm-prd exists");
        let confirm_todo = db.get_todo(confirm_step.todo_id).await.unwrap().unwrap();
        assert!(
            !confirm_todo.auto_review_enabled,
            "仅 human_approval 门禁的环节 todo 不应开启 auto_review_enabled"
        );
    }

    /// 端到端穿透测试（需求 033，验收标准 1+2）。
    ///
    /// 完整链路：工艺 YAML 定义环节 `review_prompt`（含 PENETRATION_MARKER_033）
    /// → `install_process_template` 安装 → `loop_steps.review_prompt` 落库含 marker
    /// → `resolve_review_template` 三级回退选取环节内联正文（哨兵 id=0）
    /// → `compose_review_prompt` 占位符替换合成最终评审 prompt
    /// → 断言合成后的评审 prompt 仍含 marker，且 `{{original_output}}` 等占位符已被替换。
    ///
    /// 确定性设计：不依赖真实 LLM、不启动 LoopRunner；
    /// 评审 prompt 在调 LLM 前就已合成落库，本测试只验证「穿透」这一段。
    /// 之前的测试只覆盖 `resolve_review_template` 单元层面（直接调函数），
    /// 没有把「工艺定义 → installer 落库 → 模板选取 → prompt 合成」串成一条端到端链路，
    /// 缺口正是这条「是否穿透」的自动红/绿证据。
    #[tokio::test]
    async fn test_review_prompt_penetrates_to_synthesized_review_prompt() {
        use crate::executor_service::auto_review::{
            compose_review_prompt, resolve_review_template, INLINE_REVIEW_TEMPLATE_ID,
        };

        const PENETRATION_MARKER: &str = "PENETRATION_MARKER_033";
        let db = fresh_db().await;
        let ws_id = seed_workspace(&db).await;

        // 工艺定义：单环节，带独特 review_prompt（含 PENETRATION_MARKER）+ 验收标准触发评审。
        // 占位符用 {{double_braces}} 约定，与 compose_review_prompt 的 .replace 目标一致。
        // 用普通 raw string 而非 format!，避免 YAML 里的 {{original_output}} 等占位符
        // 被 format! 当成命名参数；marker 直接硬编码进 YAML，常量留给断言用。
        let yaml = r#"process:
  name: penetration-test
  display_name: 穿透测试工艺
  description: 验证环节 review_prompt 穿透到评审实例 todo 的 prompt
  category: test
  complexity: light
  version: 0.1.0
phases:
  - id: build
    name: 构建
    spec: 构建阶段
    links:
      - id: deliver
        name: 交付
        step_template: []
        prompt: 请交付产物
        on_success: end
        on_gate_fail: break
        acceptance_criteria: 产物完整
        review_prompt: |
          你是评审师。PENETRATION_MARKER_033
          请按以下标准评审 {{original_output}}：
          {{acceptance_criteria}}
          输出 RATING: <0-100>
"#;

        let template = process_templates::ActiveModel {
            guid: ActiveValue::Set("guid-penetration-test".to_string()),
            name: ActiveValue::Set("penetration-test".to_string()),
            display_name: ActiveValue::Set("穿透测试工艺".to_string()),
            description: ActiveValue::Set(String::new()),
            category: ActiveValue::Set("test".to_string()),
            complexity: ActiveValue::Set("light".to_string()),
            version: ActiveValue::Set("0.1.0".to_string()),
            source_path: ActiveValue::Set(Some("bundled://penetration-test.yaml".to_string())),
            is_system: ActiveValue::Set(true),
            ..Default::default()
        }
        .insert(&db.conn)
        .await
        .unwrap();

        let result = install_process_template(&db, &template, yaml, ws_id, "/tmp/test-ws")
            .await
            .expect("install should succeed");

        // 1) 断言环节级 review_prompt 已落库到 loop_steps.review_prompt 且含 marker。
        let steps = db.list_loop_steps_by_loop(result.loop_id).await.unwrap();
        assert_eq!(steps.len(), 1, "穿透工艺应只有 1 个环节");
        let step = &steps[0];
        let persisted_review_prompt = step
            .review_prompt
            .as_ref()
            .expect("环节 review_prompt 必须落库到 loop_steps.review_prompt");
        assert!(
            persisted_review_prompt.contains(PENETRATION_MARKER),
            "loop_steps.review_prompt 必须含 PENETRATION_MARKER，实际: {persisted_review_prompt}"
        );

        // 2) resolve_review_template 三级回退：环节内联非空 → 选取环节正文，哨兵 id=0。
        let (template_prompt, owning_id, owning_name) =
            resolve_review_template(&db, step.review_prompt.as_deref(), None)
                .await
                .expect("resolve should succeed");
        assert_eq!(owning_id, INLINE_REVIEW_TEMPLATE_ID);
        assert_eq!(owning_name, "环节内联评审");
        assert!(
            template_prompt.contains(PENETRATION_MARKER),
            "选取的评审模板正文必须含 marker"
        );

        // 3) compose_review_prompt 占位符替换合成最终评审 prompt。
        //    手动构造最小 original todo（Todo 无 Default impl，需列出全部字段），
        //    只填 compose_review_prompt 实际读取的 prompt + acceptance_criteria，其余给零值。
        let original = crate::models::Todo {
            id: 0,
            title: String::new(),
            prompt: "请交付产物".to_string(),
            status: crate::models::TodoStatus::Pending,
            created_at: String::new(),
            updated_at: String::new(),
            tag_ids: vec![],
            executor: None,
            scheduler_enabled: false,
            scheduler_config: None,
            scheduler_timezone: None,
            scheduler_next_run_at: None,
            task_id: None,
            workspace_path: None,
            workspace_id: None,
            webhook_enabled: false,
            acceptance_criteria: Some("产物完整".to_string()),
            todo_type: 0,
            parent_todo_id: None,
            review_template_id: None,
            auto_review_enabled: true,
            action_type: None,
            action_key: None,
            archived_at: None,
            expert_name: None,
            model: None,
        };
        let final_review_prompt = compose_review_prompt(
            &original,
            &template_prompt,
            Some("DELIVERED_OUTPUT_BODY"),
        );

        // 4) 穿透断言：marker 一路穿到最终评审 prompt，占位符已被替换。
        assert!(
            final_review_prompt.contains(PENETRATION_MARKER),
            "PENETRATION_MARKER 必须穿透到最终评审 prompt，实际: {final_review_prompt}"
        );
        assert!(
            final_review_prompt.contains("DELIVERED_OUTPUT_BODY"),
            "占位符 {{{{original_output}}}} 必须被替换，实际: {final_review_prompt}"
        );
        assert!(
            final_review_prompt.contains("产物完整"),
            "占位符 {{{{acceptance_criteria}}}} 必须被替换，实际: {final_review_prompt}"
        );
        assert!(
            !final_review_prompt.contains("{{original_output}}"),
            "替换后不应残留原始占位符 {{{{original_output}}}}"
        );
    }

    /// 校验：工艺 abnormal_handler.prompt 安装为 todo_type=3 载体 Todo + loop 三列（需求 035）。
    #[tokio::test]
    async fn test_install_writes_abnormal_handler() {
        let db = fresh_db().await;
        let ws_id = seed_workspace(&db).await;
        let yaml = r#"
process:
  name: abnormal-test
  display_name: 异常处理测试工艺
  version: 0.1.0
abnormal_handler:
  prompt: |
    发生异常时执行此提示词。状态：{{abnormal_status}}
  trigger_on: ["capped_token", "failed"]
phases:
  - id: p1
    name: 阶段一
    links:
      - id: l1
        name: l1
        step_template: []
        prompt: 请执行 l1
        on_success: end
        on_gate_fail: break
"#;
        let template = process_templates::ActiveModel {
            guid: ActiveValue::Set("guid-abnormal-test".to_string()),
            name: ActiveValue::Set("abnormal-test".to_string()),
            display_name: ActiveValue::Set("异常处理测试工艺".to_string()),
            description: ActiveValue::Set(String::new()),
            category: ActiveValue::Set("test".to_string()),
            complexity: ActiveValue::Set("light".to_string()),
            version: ActiveValue::Set("0.1.0".to_string()),
            source_path: ActiveValue::Set(Some("bundled://abnormal-test.yaml".to_string())),
            is_system: ActiveValue::Set(true),
            ..Default::default()
        }
        .insert(&db.conn)
        .await
        .unwrap();

        let result = install_process_template(&db, &template, yaml, ws_id, "/tmp/test-ws")
            .await
            .expect("install should succeed");

        let loop_model = db.get_loop(result.loop_id).await.unwrap().unwrap();
        // prompt 快照写入 loop.abnormal_handler_prompt
        let prompt = loop_model
            .abnormal_handler_prompt
            .as_ref()
            .expect("loop.abnormal_handler_prompt 应已写入");
        assert!(
            prompt.contains("发生异常时执行此提示词"),
            "prompt 正文应保留: {prompt}"
        );
        // 载体 Todo 创建为 todo_type=3，并写入 loop.abnormal_handler_todo_id
        let todo_id = loop_model.abnormal_handler_todo_id.expect("应有载体 todo id");
        let todo = db.get_todo(todo_id).await.unwrap().expect("载体 todo 应存在");
        assert_eq!(
            todo.todo_type,
            crate::db::TODO_TYPE_ABNORMAL_HANDLER,
            "载体 todo 应为 type=3"
        );
        assert!(
            todo.title.contains("[异常处理]"),
            "载体 todo 标题应含 [异常处理]: {}",
            todo.title
        );
        // trigger_on 序列化写入
        let trigger_on: Vec<String> =
            serde_json::from_str(&loop_model.abnormal_handler_trigger_on).unwrap();
        assert!(trigger_on.contains(&"capped_token".to_string()));
        assert!(trigger_on.contains(&"failed".to_string()));
    }

    /// 校验：工艺未配 abnormal_handler.prompt 时不创建载体 Todo，三列为空（需求 035）。
    #[tokio::test]
    async fn test_install_no_abnormal_handler_when_prompt_empty() {
        let db = fresh_db().await;
        let ws_id = seed_workspace(&db).await;
        // 无 abnormal_handler 段
        let yaml = "process:\n  name: no-abn\n  version: 0.1.0\nphases:\n  - id: p1\n    name: p1\n    links:\n      - id: l1\n        name: l1\n        step_template: []\n        prompt: x\n        on_success: end\n        on_gate_fail: break\n";
        let template = process_templates::ActiveModel {
            guid: ActiveValue::Set("guid-no-abn".to_string()),
            name: ActiveValue::Set("no-abn".to_string()),
            display_name: ActiveValue::Set("无异常处理".to_string()),
            description: ActiveValue::Set(String::new()),
            category: ActiveValue::Set("test".to_string()),
            complexity: ActiveValue::Set("light".to_string()),
            version: ActiveValue::Set("0.1.0".to_string()),
            is_system: ActiveValue::Set(true),
            ..Default::default()
        }
        .insert(&db.conn)
        .await
        .unwrap();
        let result = install_process_template(&db, &template, yaml, ws_id, "/tmp/test-ws")
            .await
            .unwrap();
        let loop_model = db.get_loop(result.loop_id).await.unwrap().unwrap();
        assert!(
            loop_model.abnormal_handler_prompt.is_none(),
            "无 prompt 时 prompt 列应为 NULL"
        );
        assert!(
            loop_model.abnormal_handler_todo_id.is_none(),
            "无 prompt 时不应创建载体 todo"
        );
    }

    /// 校验：旧工艺 YAML 含已废弃 todo_template 字段时 serde 忽略、不报错（需求 035）。
    #[test]
    fn test_abnormal_handler_config_ignores_legacy_todo_template() {
        let yaml = r#"
prompt: 新版提示词
trigger_on: ["failed"]
todo_template: 旧字段应被忽略
"#;
        let cfg: AbnormalHandlerConfig =
            serde_yaml::from_str(yaml).expect("含旧 todo_template 不应报错");
        assert_eq!(cfg.prompt.as_deref(), Some("新版提示词"));
        assert!(cfg.trigger_on.contains(&"failed".to_string()));
    }
}
