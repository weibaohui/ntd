//! 专家索引管理器
//!
//! 内存缓存，提供专家、Agent、Skill 的快速查询。

use std::collections::HashMap;

use super::types::*;

impl ExpertIndexManager {
    /// 创建新的索引管理器
    pub fn new() -> Self {
        Self {
            experts: parking_lot::RwLock::new(HashMap::new()),
            agent_files: parking_lot::RwLock::new(HashMap::new()),
            skills: parking_lot::RwLock::new(HashMap::new()),
            expert_skills: parking_lot::RwLock::new(HashMap::new()),
            category_index: parking_lot::RwLock::new(HashMap::new()),
        }
    }

    /// 更新索引（新增或替换），同时维护专家-Agent-Skill 关联关系
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

        // 更新 Agent 文件索引
        {
            let mut agents = self.agent_files.write();
            for agent_file in agent_files {
                agents.insert(agent_file.agent_name.clone(), agent_file.clone());
            }
        }

        // 更新 Skills 索引 + 专家与 Skill 的绑定关系
        {
            let mut skill_map = self.skills.write();
            let mut skill_names = Vec::new();
            for skill in skills {
                skill_map.insert(skill.skill_name.clone(), skill.clone());
                skill_names.push(skill.skill_name.clone());
            }
            self.expert_skills
                .write()
                .insert(expert.name.clone(), skill_names);
        }

        // 更新分类索引
        if let Some(category) = &expert.category_id {
            self.category_index
                .write()
                .entry(category.clone())
                .or_default()
                .push(expert.name.clone());
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

    /// 获取 Agent MD 文件内容（按需加载）
    pub fn get_agent_md_content(&self, agent_name: &str) -> Result<String, ExpertError> {
        let agent_file = self
            .agent_files
            .read()
            .get(agent_name)
            .cloned()
            .ok_or_else(|| ExpertError::AgentNotFound(agent_name.to_string()))?;

        std::fs::read_to_string(&agent_file.md_file_path)
            .map_err(|e| ExpertError::FileReadError(agent_file.md_file_path.clone(), e))
    }

    /// 获取专家关联的所有 Skill 元数据
    pub fn get_expert_skills(&self, expert_name: &str) -> Vec<SkillMetadata> {
        let names = self
            .expert_skills
            .read()
            .get(expert_name)
            .cloned()
            .unwrap_or_default();
        let skills = self.skills.read();
        names
            .iter()
            .filter_map(|name| skills.get(name).cloned())
            .collect()
    }

    /// 获取 Skill 的 SKILL.md 完整内容（按需加载）
    pub fn get_skill_md_content(&self, skill_name: &str) -> Result<String, ExpertError> {
        let skill = self
            .skills
            .read()
            .get(skill_name)
            .cloned()
            .ok_or_else(|| ExpertError::SkillNotFound(skill_name.to_string()))?;
        std::fs::read_to_string(&skill.skill_md_path)
            .map_err(|e| ExpertError::FileReadError(skill.skill_md_path.clone(), e))
    }

    /// 重新加载指定专家
    pub fn reload_expert(&self, expert_dir: &std::path::Path) -> Result<(), ExpertError> {
        use super::loader::load_single_expert;

        let plugin_json_path = expert_dir.join(".codebuddy-plugin/plugin.json");
        let load_result = load_single_expert(expert_dir, &plugin_json_path)?;
        self.update_index(&load_result.expert, &load_result.agent_files, &load_result.skills);
        Ok(())
    }

    /// 清空所有索引（用于完全重新加载）
    pub fn clear(&self) {
        self.experts.write().clear();
        self.agent_files.write().clear();
        self.skills.write().clear();
        self.expert_skills.write().clear();
        self.category_index.write().clear();
    }
}

impl Default for ExpertIndexManager {
    fn default() -> Self {
        Self::new()
    }
}
