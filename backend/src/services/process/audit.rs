//! 工艺审计查询 —— 返回阶段 → 环节 → 产物 → 门禁的完整执行链。

use serde::{Deserialize, Serialize};

use crate::db::entity::{loop_step_artifacts, loop_step_execution_gates, loop_steps};
use crate::db::Database;

/// 审计响应顶级结构。
#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessAudit {
    pub loop_execution: LoopExecutionSummary,
    pub phases: Vec<PhaseAudit>,
}

/// 执行摘要。
#[derive(Debug, Serialize, Deserialize)]
pub struct LoopExecutionSummary {
    pub id: i64,
    pub loop_id: i64,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub total_steps: i32,
    pub completed_steps: i32,
    pub failed_steps: i32,
}

/// 阶段审计。
#[derive(Debug, Serialize, Deserialize)]
pub struct PhaseAudit {
    pub phase_id: i64,
    pub phase_name: String,
    pub execution: Option<PhaseExecutionStatus>,
    pub steps: Vec<StepAudit>,
}

/// 阶段执行状态。
#[derive(Debug, Serialize, Deserialize)]
pub struct PhaseExecutionStatus {
    pub status: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

/// 环节审计。
#[derive(Debug, Serialize, Deserialize)]
pub struct StepAudit {
    pub step_id: i64,
    pub step_name: String,
    pub order_index: i32,
    pub skill_names: Vec<String>,
    pub execution: Option<StepExecutionStatus>,
    pub artifacts: Vec<loop_step_artifacts::Model>,
    pub gates: Vec<loop_step_execution_gates::Model>,
}

/// 环节执行状态。
#[derive(Debug, Serialize, Deserialize)]
pub struct StepExecutionStatus {
    pub sequence_index: i32,
    pub status: String,
    pub rework_count: i32,
    pub rating: Option<i32>,
    pub error_message: Option<String>,
    pub conclusion: Option<String>,
}

impl Database {
    /// 查询工艺实例的完整审计数据。
    ///
    /// 返回阶段 → 环节 → 产物 → 门禁的树形结构。
    pub async fn get_loop_execution_audit(
        &self,
        loop_execution_id: i64,
    ) -> Result<Option<ProcessAudit>, sea_orm::DbErr> {

        // 1. 查询 loop_execution。
        let le = self.get_loop_execution(loop_execution_id).await?;
        let le = match le {
            Some(v) => v,
            None => return Ok(None),
        };

        // 2. 查询 phases。
        let phases = self.list_loop_phases_by_loop(le.loop_id).await?;

        // 3. 所有 step executions 一次拉出。
        let step_execs = self.list_loop_step_executions(loop_execution_id).await.unwrap_or_default();

        // 4. 所有 steps。
        let steps = self.list_loop_steps_by_loop(le.loop_id).await.unwrap_or_default();

        // 5. 所有产物 + 门禁（逐 step_execution 查询比较慢但可接受，审计不是高频）。
        let mut phase_audits = Vec::with_capacity(phases.len());
        for phase in &phases {
            let phase_steps: Vec<&loop_steps::Model> = steps.iter().filter(|s| s.phase_id == Some(phase.id)).collect();
            let phase_exec = self.find_phase_execution(loop_execution_id, phase.id).await?;

            let mut step_audits = Vec::with_capacity(phase_steps.len());
            for step in &phase_steps {
                let step_exec = step_execs.iter().find(|se| se.step_id == step.id);
                let artifacts = if let Some(se) = step_exec {
                    self.list_loop_step_artifacts(se.id).await.unwrap_or_default()
                } else {
                    vec![]
                };
                let gates = if let Some(se) = step_exec {
                    self.list_loop_step_execution_gates(se.id).await.unwrap_or_default()
                } else {
                    vec![]
                };

                let skill_names: Vec<String> = serde_json::from_str(&step.skill_names).unwrap_or_default();

                step_audits.push(StepAudit {
                    step_id: step.id,
                    step_name: step.name.clone(),
                    order_index: step.order_index,
                    skill_names,
                    execution: step_exec.map(|se| StepExecutionStatus {
                        sequence_index: se.sequence_index,
                        status: se.status.clone(),
                        rework_count: se.rework_count,
                        rating: se.rating,
                        error_message: se.error_message.clone(),
                        conclusion: se.conclusion.clone(),
                    }),
                    artifacts,
                    gates,
                });
            }

            phase_audits.push(PhaseAudit {
                phase_id: phase.id,
                phase_name: phase.name.clone(),
                execution: Some(PhaseExecutionStatus {
                    status: phase_exec.as_ref().map(|p| p.status.clone()).unwrap_or_else(|| "pending".to_string()),
                    started_at: phase_exec.as_ref().and_then(|p| p.started_at.clone()),
                    finished_at: phase_exec.as_ref().and_then(|p| p.finished_at.clone()),
                }),
                steps: step_audits,
            });
        }

        Ok(Some(ProcessAudit {
            loop_execution: LoopExecutionSummary {
                id: le.id,
                loop_id: le.loop_id,
                status: le.status,
                started_at: le.started_at,
                finished_at: le.finished_at,
                total_steps: le.total_steps,
                completed_steps: le.completed_steps,
                failed_steps: le.failed_steps,
            },
            phases: phase_audits,
        }))
    }

    /// 查询阶段执行记录。
    async fn find_phase_execution(
        &self,
        loop_execution_id: i64,
        phase_id: i64,
    ) -> Result<Option<crate::db::entity::loop_phase_executions::Model>, sea_orm::DbErr> {
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
        crate::db::entity::loop_phase_executions::Entity::find()
            .filter(crate::db::entity::loop_phase_executions::Column::LoopExecutionId.eq(loop_execution_id))
            .filter(crate::db::entity::loop_phase_executions::Column::PhaseId.eq(phase_id))
            .one(&self.conn)
            .await
    }
}
