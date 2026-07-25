//! 工艺模板实例化：把 `process_templates` 中的 YAML 定义转换为可执行的 Loop。

use std::collections::HashMap;

use sea_orm::{ActiveModelTrait, ActiveValue};

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
    let definition: ProcessDefinition = serde_yaml::from_str(&template.definition)?;

    // 校验所有 link 引用的 step_template 是否存在。
    // skill 名称是自由文本（executor 在运行时注入），不做强制校验，仅记 warn。
    check_step_template_dependencies(db, &definition.phases).await?;

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
        resolve_step_template_fields(db, link).await?;

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

    Ok(todo_id)
}

/// 解析 link 的环节原型引用或内联字段。
async fn resolve_step_template_fields(
    db: &Database,
    link: &LinkDefinition,
) -> Result<(
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
), InstallError> {
    if let Some(ref template_name) = link.step_template {
        let template = db
            .get_process_step_template_by_name(template_name)
            .await?
            .ok_or_else(|| InstallError::StepTemplateNotFound(template_name.clone()))?;
        let title = if template.title.is_empty() {
            template.name.clone()
        } else {
            template.title.clone()
        };
        return Ok((
            title,
            template.prompt,
            template.executor,
            template.expert_name,
            Some(template.acceptance_criteria).filter(|s| !s.is_empty()),
            template.model,
        ));
    }

    Ok((
        link.name.clone(),
        link.prompt.clone(),
        link.executor.clone(),
        link.expert.clone(),
        None,
        link.model.clone(),
    ))
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
        on_rating_fail: ActiveValue::Set(link.on_rating_fail.clone()),
        fail_goto_step_id: ActiveValue::Set(None),
        review_type: ActiveValue::Set(link.review_type.clone()),
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

/// 校验工艺模板中所有 link 引用的 step_template 是否存在。
/// 期绑定 skill 名称仅记录 warn（skill 是 executor 级别的文件注入，存于 filesystem）。
async fn check_step_template_dependencies(
    db: &Database,
    phases: &[PhaseDefinition],
) -> Result<(), InstallError> {
    for phase in phases {
        for link in &phase.links {
            if let Some(ref template_name) = link.step_template {
                if db.get_process_step_template_by_name(template_name).await?.is_none() {
                    return Err(InstallError::StepTemplateNotFound(template_name.clone()));
                }
            }
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
        step_template: write-prd
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
        )
        .await
        .unwrap();
    }

    async fn seed_workspace(db: &Database) -> i64 {
        db.create_project_directory("/tmp/test-ws", Some("测试空间"), false, false)
            .await
            .unwrap()
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
    }

    #[tokio::test]
    async fn test_install_missing_step_template_returns_error() {
        let db = fresh_db().await;
        let ws_id = seed_workspace(&db).await;

        let template = process_templates::ActiveModel {
            name: ActiveValue::Set("missing-step".to_string()),
            display_name: ActiveValue::Set("缺失环节原型".to_string()),
            description: ActiveValue::Set("".to_string()),
            category: ActiveValue::Set("test".to_string()),
            complexity: ActiveValue::Set("light".to_string()),
            version: ActiveValue::Set("1.0.0".to_string()),
            definition: ActiveValue::Set(
                "process:\n  name: missing-step\nphases:\n  - id: p1\n    name: p1\n    links:\n      - id: l1\n        name: l1\n        step_template: nonexistent\n"
                    .to_string(),
            ),
            is_system: ActiveValue::Set(true),
            ..Default::default()
        }
        .insert(&db.conn)
        .await
        .unwrap();

        let err = install_process_template(&db, &template, ws_id, "/tmp/test-ws",
        )
        .await
        .expect_err("should fail for missing step template");
        assert!(
            err.to_string().contains("nonexistent"),
            "错误信息应包含缺失的环节原型名"
        );
    }
}
