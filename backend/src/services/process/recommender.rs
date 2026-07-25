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
    let mut results = Vec::new();

    for template in templates {
        let mut reasons = Vec::new();
        let mut keyword_score = 0.0_f64;
        let mut complexity_score = 0.0_f64;
        let mut total_weight = 0.0_f64;

        // 关键词匹配。
        for (keywords, weight, target_name) in KEYWORD_MAP {
            if template.name != *target_name {
                continue;
            }
            let hit_count = keywords
                .iter()
                .filter(|kw| desc_lower.contains(&kw.to_lowercase()))
                .count() as f64;
            if hit_count > 0.0 {
                keyword_score = hit_count / keywords.len() as f64 * *weight;
                reasons.push(format!("匹配 {} 个关键词 ({})", hit_count as i32, keywords.iter().filter(|kw| desc_lower.contains(&kw.to_lowercase())).copied().collect::<Vec<_>>().join(", ")));
            }
            total_weight += weight;
        }

        // 复杂度推理。
        for (kw, complexity) in COMPLEXITY_KEYWORDS {
            if desc_lower.contains(&kw.to_lowercase()) && *complexity == template.complexity {
                complexity_score += 0.15;
                reasons.push(format!("复杂度匹配: {} → {}", kw, complexity));
                break;
            }
        }

        // 历史采纳率简化评分（无 install_count 字段，使用固定权重）。
        let popularity_score = 0.05;

        let score = keyword_score + complexity_score + popularity_score;
        if score > 0.0 || reasons.is_empty() {
            // 如果有匹配或任何模板都展示（但低分排后面）。
            results.push(RecommendResult {
                template_name: template.name.clone(),
                display_name: if template.display_name.is_empty() {
                    template.name.clone()
                } else {
                    template.display_name.clone()
                },
                complexity: template.complexity.clone(),
                score,
                reasons: if reasons.is_empty() {
                    vec!["通用推荐".to_string()]
                } else {
                    reasons
                },
            });
        }
    }

    // 按分数降序排列。
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // 最少返回 3 条（不足补全）。
    if results.len() < 3 && !templates.is_empty() {
        for template in templates {
            if !results.iter().any(|r| r.template_name == template.name) {
                results.push(RecommendResult {
                    template_name: template.name.clone(),
                    display_name: if template.display_name.is_empty() {
                        template.name.clone()
                    } else {
                        template.display_name.clone()
                    },
                    complexity: template.complexity.clone(),
                    score: 0.05,
                    reasons: vec!["通用推荐".to_string()],
                });
            }
            if results.len() >= 3 {
                break;
            }
        }
    }

    RecommendResponse {
        recommendations: results,
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
            definition: String::new(),
            source_path: None,
            workspace_id: None,
            is_system: true,
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
