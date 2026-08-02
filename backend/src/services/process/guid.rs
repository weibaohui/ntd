//! 工艺 YAML 的 guid 行级操作（040）。
//!
//! 系统层文件的 guid 由远端仓库提供（本地不可回写，`git reset --hard` 会抹掉）；
//! 用户层文件（`~/.ntd/processes/`）不受 git 管理，缺失 guid 时导入链路生成并回写。
//!
//! 所有操作都是**行级**的（不用 serde 往返），保住注释与 `description: |` 块标量格式。
//! 行级规则与 ntd-resource 仓库的批量加 guid 脚本一致：
//! 只在 `process:` 顶层块内匹配恰好 2 空格缩进的 `name:` / `guid:` 行。

/// 在 YAML 的 `process:` 块内 `name:` 行后插入 `guid: <guid>` 行。
///
/// 已有 guid 行或找不到插入点时返回 `None`（调用方决定报错还是跳过）。
pub fn insert_guid_after_name(yaml: &str, guid: &str) -> Option<String> {
    let mut lines: Vec<String> = yaml.lines().map(str::to_string).collect();
    let (block_start, block_end) = process_block_range(&lines)?;

    // 已有 guid 行：不重复插入（幂等）。
    if lines[block_start..block_end]
        .iter()
        .any(|l| l.starts_with("  guid:"))
    {
        return None;
    }
    let name_idx = lines[block_start..block_end]
        .iter()
        .position(|l| l.starts_with("  name:"))
        .map(|i| i + block_start)?;

    lines.insert(name_idx + 1, format!("  guid: {guid}"));
    Some(join_lines(&lines))
}

/// 把 YAML 的 `process:` 块内 `guid:` 行替换为新值（复制副本时用）。
///
/// 找不到 guid 行时退化为插入（兼容源文件缺 guid 的边界）。
pub fn replace_or_insert_guid(yaml: &str, new_guid: &str) -> Option<String> {
    let mut lines: Vec<String> = yaml.lines().map(str::to_string).collect();
    let (block_start, block_end) = process_block_range(&lines)?;

    if let Some(idx) = lines[block_start..block_end]
        .iter()
        .position(|l| l.starts_with("  guid:"))
        .map(|i| i + block_start)
    {
        lines[idx] = format!("  guid: {new_guid}");
        return Some(join_lines(&lines));
    }
    insert_guid_after_name(yaml, new_guid)
}

/// 定位 `process:` 顶层块的范围 `[块内首行, 块结束)`。
///
/// 块结束于下一个顶格（非缩进、非空）行——YAML 顶层键的边界。
/// 文件里没有 `process:` 行时返回 `None`。
fn process_block_range(lines: &[String]) -> Option<(usize, usize)> {
    let proc_idx = lines.iter().position(|l| l.trim_end() == "process:")?;
    let end = lines
        .iter()
        .enumerate()
        .skip(proc_idx + 1)
        .find(|(_, l)| {
            let t = l.trim_end();
            !t.is_empty() && !l.starts_with(' ') && !l.starts_with('\t')
        })
        .map(|(i, _)| i)
        .unwrap_or(lines.len());
    Some((proc_idx + 1, end))
}

/// 以 `\n` 重新拼接行；保持结尾换行（YAML 文件惯例，避免 diff 噪音）。
fn join_lines(lines: &[String]) -> String {
    let mut s = lines.join("\n");
    s.push('\n');
    s
}

/// 生成一个新的工艺 guid（UUID v4）。
pub fn new_guid() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// 插入 guid 的增强版（056）：先行级插入（保格式），失败后退化 serde 往返。
///
/// 背景：行级插入只认 block 风格 `process:` + 2 空格缩进，遇到 flow 风格
/// （`process: {name: x}`）或非常规缩进时永远失败，且调用方每次导入都重试，
/// 造成「同一条 warn 反复刷屏」。serde 往返会丢注释并标准化格式，
/// 但对这些本就非标准的文件而言，「能修复」比「格式原样」更重要。
///
/// 已有 guid 时返回原文（幂等）；YAML 无法解析或没有 process 映射时返回 None。
pub fn insert_guid_with_serde_fallback(yaml: &str, guid: &str) -> Option<String> {
    if let Some(out) = insert_guid_after_name(yaml, guid) {
        return Some(out);
    }
    let mut value: serde_yaml::Value = serde_yaml::from_str(yaml).ok()?;
    let mapping = value.get_mut("process")?.as_mapping_mut()?;
    let guid_key = serde_yaml::Value::String("guid".to_string());
    // 已有非空 guid：幂等，返回原文避免无谓写盘；
    // `guid: ""` 空值视为缺失，覆盖写入新 guid（否则每次扫描都会重复生成）。
    if let Some(existing) = mapping.get(&guid_key) {
        if existing.as_str().is_some_and(|s| !s.is_empty()) {
            return Some(yaml.to_string());
        }
    }
    mapping.insert(guid_key, serde_yaml::Value::String(guid.to_string()));
    serde_yaml::to_string(&value).ok()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const SAMPLE: &str = "process:\n  name: demo\n  display_name: 演示\nphases:\n  - id: p1\n    name: 阶段一\n";

    #[test]
    fn test_insert_guid_after_name_basic() {
        let out = insert_guid_after_name(SAMPLE, "g-1").unwrap();
        assert!(out.contains("  name: demo\n  guid: g-1\n"), "guid 应紧跟 name 行");
        // phases 里的 name 行不能被误伤。
        assert!(out.contains("    name: 阶段一\n"));
        assert_eq!(out.matches("guid:").count(), 1);
    }

    #[test]
    fn test_insert_guid_idempotent_when_present() {
        let yaml = "process:\n  name: demo\n  guid: old\n";
        assert!(insert_guid_after_name(yaml, "new").is_none());
    }

    #[test]
    fn test_insert_guid_no_process_block() {
        assert!(insert_guid_after_name("foo:\n  bar: 1\n", "g").is_none());
    }

    #[test]
    fn test_replace_or_insert_guid_replaces_only_process_guid() {
        let yaml = "process:\n  name: demo\n  guid: old\nphases:\n  - id: p1\n    name: 阶段一\n";
        let out = replace_or_insert_guid(yaml, "new").unwrap();
        assert!(out.contains("  guid: new\n"));
        assert!(!out.contains("old"));
    }

    #[test]
    fn test_replace_or_insert_guid_inserts_when_missing() {
        let out = replace_or_insert_guid(SAMPLE, "g-2").unwrap();
        assert!(out.contains("  name: demo\n  guid: g-2\n"));
    }

    #[test]
    fn test_new_guid_format() {
        let g = new_guid();
        assert_eq!(g.len(), 36, "UUID v4 字符串应为 36 字符");
        assert_ne!(new_guid(), new_guid(), "两次生成不应相同");
    }

    /// 056：flow 风格 `process: {name: x}` 行级插入失败，serde fallback 应修复。
    /// 这是生产日志刷屏的根因用例（~/.ntd/processes/repro-stdin.yaml）。
    #[test]
    fn test_fallback_handles_flow_style_process() {
        let yaml = "process: {name: x}\n";
        let out = insert_guid_with_serde_fallback(yaml, "g-flow").unwrap();
        // serde 往返后为 block 风格且带 guid
        let value: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(
            value["process"]["guid"].as_str(),
            Some("g-flow"),
            "guid 应被写入: {out}"
        );
        assert_eq!(value["process"]["name"].as_str(), Some("x"), "原字段不丢");
    }

    /// 056：block 风格优先走行级插入（保注释与格式），不走 serde。
    #[test]
    fn test_fallback_prefers_line_based_insert() {
        let yaml = "# 顶部注释\nprocess:\n  name: demo\n";
        let out = insert_guid_with_serde_fallback(yaml, "g-line").unwrap();
        assert!(out.contains("# 顶部注释"), "行级插入应保住注释");
        assert!(out.contains("  name: demo\n  guid: g-line\n"));
    }

    /// 056：已有非空 guid 时幂等返回原文（不覆盖、不写盘）。
    #[test]
    fn test_fallback_idempotent_with_existing_guid() {
        let yaml = "process: {name: x, guid: old-g}\n";
        let out = insert_guid_with_serde_fallback(yaml, "new-g").unwrap();
        assert_eq!(out, yaml, "已有 guid 应原样返回");
    }

    /// 056：`guid: ""` 空值视为缺失，覆盖写入新 guid。
    #[test]
    fn test_fallback_overwrites_empty_guid() {
        let yaml = "process: {name: x, guid: ''}\n";
        let out = insert_guid_with_serde_fallback(yaml, "g-new").unwrap();
        let value: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(value["process"]["guid"].as_str(), Some("g-new"));
    }

    /// 056：无 process 映射的非法输入返回 None（调用方报错）。
    #[test]
    fn test_fallback_returns_none_for_invalid() {
        assert!(insert_guid_with_serde_fallback("foo: bar\n", "g").is_none());
        assert!(insert_guid_with_serde_fallback("process: 42\n", "g").is_none());
    }
}
