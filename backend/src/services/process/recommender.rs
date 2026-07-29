//! 工艺推荐引擎 —— 基于关键词匹配与规则打分为任务推荐合适的工艺模板。
//!
//! M3 首期不做 embedding，使用关键词匹配 + 复杂度推理 + 历史采纳率加权。

use serde::{Deserialize, Serialize};

use crate::db::entity::process_templates;

/// 推荐请求。
#[derive(Debug, Deserialize)]
pub struct RecommendRequest {
    pub description: String,
}

/// 单条推荐结果。
#[derive(Debug, Serialize)]
pub struct RecommendResult {
    pub template_name: String,
    pub display_name: String,
    pub complexity: String,
    pub score: f64,
    pub reasons: Vec<String>,
}

/// 推荐响应。
#[derive(Debug, Serialize)]
pub struct RecommendResponse {
    pub recommendations: Vec<RecommendResult>,
}

/// 内置关键词 → 模板映射。
const KEYWORD_MAP: &[(&[&str], f64, &str)] = &[
    (
        &["需求", "PRD", "用户故事", "技术设计", "验证计划", "tasking",
          "TDD", "e2e", "api", "测试", "Git提交", "部署", "交付"],
        0.8,
        "4p12s-delivery",
    ),
    (
        &["小需求", "修bug", "修复", "快速", "简单", "小功能", "brainstorming",
          "writing-plans", "code-review", "verification", "reflect"],
        0.7,
        "superpowers-task",
    ),
    (
        &["重构", "复杂", "大型", "规约", "治理", "constitution", "specify",
          "clarify", "plan", "checklist", "analyze", "implement", "converge"],
        0.9,
        "gienspec-complex",
    ),
    (
        &["口头", "整理", "记录", "口述", "聊天记录", "语音"],
        0.7,
        "oral-requirement",
    ),
];

/// 复杂度关键词。
const COMPLEXITY_KEYWORDS: &[(&str, &str)] = &[
    ("复杂", "complex"),
    ("大型", "complex"),
    ("重构", "complex"),
    ("规约", "complex"),
    ("小", "light"),
    ("简单", "light"),
    ("快速", "light"),
    ("轻", "light"),
];

/// 为任务描述推荐工艺模板。
pub fn recommend(
    templates: &[process_templates::Model],
    request: &RecommendRequest,
) -> RecommendResponse {
    let desc_lower = request.description.to_lowercase();
    let mut results: Vec<RecommendResult> = templates
        .iter()
        .filter_map(|t| score_template(t, &desc_lower))
        .collect();
    // 补足到至少 3 条。
    fill_recommendations(&mut results, templates);

    // 按分数降序排列。
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    RecommendResponse {
        recommendations: results,
    }
}

/// 计算单个模板的推荐得分和理由。
fn score_template(template: &process_templates::Model, desc_lower: &str) -> Option<RecommendResult> {
    let mut reasons = Vec::new();
    let (keyword_score, _) = score_keywords(desc_lower, &template.name, &mut reasons);
    let complexity_score = score_complexity(desc_lower, &template.complexity, &mut reasons);

    let score = keyword_score + complexity_score + 0.05; // 0.05 = 基础分
    if score <= 0.05 && reasons.is_empty() { return None; }

    let display = if template.display_name.is_empty() { &template.name } else { &template.display_name };
    Some(RecommendResult {
        template_name: template.name.clone(),
        display_name: display.clone(),
        complexity: template.complexity.clone(),
        score,
        reasons: if reasons.is_empty() { vec!["通用推荐".into()] } else { reasons },
    })
}

/// 关键词匹配打分。
fn score_keywords(desc: &str, template_name: &str, reasons: &mut Vec<String>) -> (f64, f64) {
    for (keywords, weight, target_name) in KEYWORD_MAP {
        if template_name != *target_name { continue; }
        let hit_count = keywords.iter().filter(|kw| desc.contains(&kw.to_lowercase())).count() as f64;
        if hit_count > 0.0 {
            let matched: Vec<_> = keywords.iter().filter(|kw| desc.contains(&kw.to_lowercase())).copied().collect();
            reasons.push(format!("匹配 {} 个关键词 ({})", hit_count as i32, matched.join(", ")));
            return (hit_count / keywords.len() as f64 * *weight, *weight);
        }
    }
    (0.0, 0.0)
}

/// 复杂度关键词推理。
fn score_complexity(desc: &str, complexity: &str, reasons: &mut Vec<String>) -> f64 {
    for (kw, comp) in COMPLEXITY_KEYWORDS {
        if desc.contains(&kw.to_lowercase()) && *comp == complexity {
            reasons.push(format!("复杂度匹配: {} → {}", kw, complexity));
            return 0.15;
        }
    }
    0.0
}

/// 补足推荐列表到至少 3 条。
fn fill_recommendations(results: &mut Vec<RecommendResult>, templates: &[process_templates::Model]) {
    if results.len() >= 3 || templates.is_empty() { return; }
    for t in templates {
        if !results.iter().any(|r| r.template_name == t.name) {
            results.push(RecommendResult {
                template_name: t.name.clone(),
                display_name: t.display_name.clone(),
                complexity: t.complexity.clone(),
                score: 0.05,
                reasons: vec!["通用推荐".into()],
            });
        }
        if results.len() >= 3 { break; }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn make_template(name: &str, complexity: &str) -> process_templates::Model {
        process_templates::Model {
            id: 1,
            name: name.to_string(),
            display_name: name.to_string(),
            description: String::new(),
            category: "software".to_string(),
            complexity: complexity.to_string(),
            version: "1.0.0".to_string(),
            source_path: None,
            workspace_id: None,
            is_system: true,
            // V72 版本链字段：推荐逻辑不关心版本链，测试构造置 None
            previous_version_id: None,
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn test_recommend_matches_prd_keywords() {
        let templates = vec![
            make_template("4p12s-delivery", "standard"),
            make_template("superpowers-task", "light"),
        ];
        let req = RecommendRequest {
            description: "我需要做一个需求分析，写 PRD".to_string(),
        };
        let resp = recommend(&templates, &req);
        assert!(!resp.recommendations.is_empty());
        // 4p12s-delivery 应该排在前面（匹配 PRD 关键词）。
        let first = &resp.recommendations[0];
        assert_eq!(first.template_name, "4p12s-delivery");
        assert!(first.score > 0.1);
    }

    #[test]
    fn test_recommend_complexity_inference() {
        let templates = vec![
            make_template("4p12s-delivery", "standard"),
            make_template("gienspec-complex", "complex"),
        ];
        let req = RecommendRequest {
            description: "大型复杂重构项目".to_string(),
        };
        let resp = recommend(&templates, &req);
        let first = &resp.recommendations[0];
        assert_eq!(first.template_name, "gienspec-complex");
    }

    #[test]
    fn test_recommend_returns_at_least_3() {
        let templates = vec![
            make_template("a", "light"),
            make_template("b", "standard"),
            make_template("c", "complex"),
        ];
        let req = RecommendRequest {
            description: "无关描述".to_string(),
        };
        let resp = recommend(&templates, &req);
        assert_eq!(resp.recommendations.len(), 3);
    }
}
