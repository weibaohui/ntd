//! 专家索引管理器
//!
//! 内存缓存，提供专家、Agent、Skill 的快速查询。

use std::collections::HashMap;

use super::types::*;

impl ExpertMetadata {
    /// 解析专家的主理 agent 名称：team 类型用 lead_agent，agent 类型用 agent_name。
    ///
    /// 统一这层是因为三处调用（执行注入 / wiki 注入 / API 查 MD）各写各的曾导致
    /// team 类型在执行路径漏注入——team 的 agent_name 通常为 None，必须用 lead_agent。
    /// lead_agent 优先：team 负责人是最权威的 agent；agent 类型 lead_agent 为 None，
    /// 自然回退到 agent_name。
    pub fn resolve_agent_name(&self) -> Option<&str> {
        self.lead_agent.as_deref().or(self.agent_name.as_deref())
    }
}

impl ExpertIndexManager {
    /// 创建新的索引管理器
    pub fn new() -> Self {
        Self {
            experts: parking_lot::RwLock::new(HashMap::new()),
            agent_files: parking_lot::RwLock::new(HashMap::new()),
            skills: parking_lot::RwLock::new(HashMap::new()),
            category_index: parking_lot::RwLock::new(HashMap::new()),
        }
    }

    /// 更新索引（新增或替换），同时维护专家-Agent-Skill 关联关系
    ///
    /// agent_files 和 skills 按 expert_name 嵌套插入，确保不同专家的同名资源
    /// 完全隔离（修复前全局 HashMap 会互相覆盖、remove_expert 会误删共享资源）。
    ///
    /// # 参数
    /// - `expert`: 专家元数据
    /// - `agent_files`: Agent 文件元数据列表
    /// - `skills`: Skill 元数据列表
    pub fn update_index(
        &self,
        expert: &ExpertMetadata,
        agent_files: &[AgentFileMetadata],
        skills: &[SkillMetadata],
    ) {
        // 更新专家列表
        self.experts.write().insert(expert.name.clone(), expert.clone());

        // 更新 Agent 文件索引（按专家嵌套）
        {
            let mut map = self.agent_files.write();
            let inner = map.entry(expert.name.clone()).or_default();
            for agent_file in agent_files {
                inner.insert(agent_file.agent_name.clone(), agent_file.clone());
            }
        }

        // 更新 Skills 索引（按专家嵌套）
        {
            let mut map = self.skills.write();
            let inner = map.entry(expert.name.clone()).or_default();
            for skill in skills {
                inner.insert(skill.skill_name.clone(), skill.clone());
            }
        }

        // 更新分类索引（去重，避免 update_index 不清旧条目导致重复）
        if let Some(category) = &expert.category_id {
            let mut cat = self.category_index.write();
            let list = cat.entry(category.clone()).or_default();
            if !list.iter().any(|n| n == &expert.name) {
                list.push(expert.name.clone());
            }
        }
    }

    /// 获取所有专家列表
    pub fn get_all_experts(&self) -> Vec<ExpertMetadata> {
        self.experts.read().values().cloned().collect()
    }

    /// 根据 name 获取专家
    pub fn get_expert_by_name(&self, name: &str) -> Option<ExpertMetadata> {
        self.experts.read().get(name).cloned()
    }

    /// 根据分类获取专家
    pub fn get_experts_by_category(&self, category_id: &str) -> Vec<ExpertMetadata> {
        let names = self
            .category_index
            .read()
            .get(category_id)
            .cloned()
            .unwrap_or_default();
        let experts = self.experts.read();
        names
            .iter()
            .filter_map(|name| experts.get(name).cloned())
            .collect()
    }

    /// 获取指定专家的 Agent MD 文件内容（按需加载）
    ///
    /// 按 `(expert_name, agent_name)` 复合键查找，避免不同专家同名 agent 互窜。
    pub fn get_agent_md_content(
        &self,
        expert_name: &str,
        agent_name: &str,
    ) -> Result<String, ExpertError> {
        let agent_file = self
            .agent_files
            .read()
            .get(expert_name)
            .and_then(|m| m.get(agent_name).cloned())
            .ok_or_else(|| ExpertError::AgentNotFound(agent_name.to_string()))?;

        std::fs::read_to_string(&agent_file.md_file_path)
            .map_err(|e| ExpertError::FileReadError(agent_file.md_file_path.clone(), e))
    }

    /// 获取指定专家关联的所有 Skill 元数据
    pub fn get_expert_skills(&self, expert_name: &str) -> Vec<SkillMetadata> {
        self.skills
            .read()
            .get(expert_name)
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default()
    }

    /// 获取指定专家的 Skill 的 SKILL.md 完整内容（按需加载）
    pub fn get_skill_md_content(
        &self,
        expert_name: &str,
        skill_name: &str,
    ) -> Result<String, ExpertError> {
        let skill = self
            .skills
            .read()
            .get(expert_name)
            .and_then(|m| m.get(skill_name).cloned())
            .ok_or_else(|| ExpertError::SkillNotFound(skill_name.to_string()))?;
        std::fs::read_to_string(&skill.skill_md_path)
            .map_err(|e| ExpertError::FileReadError(skill.skill_md_path.clone(), e))
    }

    /// 重新加载指定专家
    ///
    /// 根据专家目录自动判断来源（bundled 目录下为系统专家，否则为用户专家）。
    pub fn reload_expert(&self, expert_dir: &std::path::Path) -> Result<(), ExpertError> {
        use super::loader::load_single_expert;

        let source = Self::detect_source(expert_dir);
        let plugin_json_path = expert_dir.join(".codebuddy-plugin/plugin.json");
        let load_result = load_single_expert(expert_dir, &plugin_json_path, source)?;
        self.update_index(&load_result.expert, &load_result.agent_files, &load_result.skills);
        Ok(())
    }

    /// 根据专家目录路径判断来源
    ///
    /// 位于 ~/.ntd/bundled/experts/ 下的为系统专家，其余为用户专家。
    fn detect_source(expert_dir: &std::path::Path) -> ExpertSource {
        use crate::expert::loader::bundled_experts_dir;

        if let Some(bundled_dir) = bundled_experts_dir() {
            if expert_dir.starts_with(&bundled_dir) {
                return ExpertSource::System;
            }
        }
        ExpertSource::User
    }

    /// 清空所有索引（用于完全重新加载）
    pub fn clear(&self) {
        self.experts.write().clear();
        self.agent_files.write().clear();
        self.skills.write().clear();
        self.category_index.write().clear();
    }

    /// 移除指定专家及其所有关联索引
    ///
    /// 按专家名嵌套删除：agent_files/skills 只移除该专家的子映射，
    /// 不会影响其他专家同名资源。注意：此方法不删磁盘文件。
    pub fn remove_expert(&self, expert_name: &str) -> Option<ExpertMetadata> {
        let removed = self.experts.write().remove(expert_name);

        // 只移除该专家的 agent_files 子映射（不再迭代 agent_name 误删全局）
        self.agent_files.write().remove(expert_name);

        // 只移除该专家的 skills 子映射
        self.skills.write().remove(expert_name);

        // 从分类索引中移除
        if let Some(expert) = &removed {
            if let Some(category) = &expert.category_id {
                if let Some(names) = self.category_index.write().get_mut(category) {
                    names.retain(|n| n != expert_name);
                }
            }
        }

        removed
    }
}

impl Default for ExpertIndexManager {
    fn default() -> Self {
        Self::new()
    }
}
