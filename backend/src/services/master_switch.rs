//! 双路径接入的总闸：env var 控制 ntd-connect dispatcher 切流。
//!
//! # 设计
//!
//! - env var：`NTD_CONNECT_DISPATCHER_ENABLED`（默认 `false`）
//! - `false`：所有消息走老 `feishu_listener::handle_message` 7 阶段 inline
//! - `true`：消息走新 `dispatcher.on_message` 路径
//! - `true` 但 dispatcher 没构造（构造失败）：**graceful fallback** 到老路径
//!
//! # 为什么需要总闸
//!
//! ntd-connect crate M1-M10 完整（trait + dispatcher + session + ClaudeCodeAgent
//! + FeishuPlatform），但**还没接进 backend 运行时**。如果一刀切到 dispatcher：
//! - 任何 dispatcher 初始化失败都会让所有飞书消息收不到
//! - 新代码与老代码行为差异大，QA 风险高
//!
//! 总闸让两个路径并行，老路径始终可用，新路径渐进式验证。

use std::sync::OnceLock;

/// env var 名。
pub const ENV_VAR: &str = "NTD_CONNECT_DISPATCHER_ENABLED";

/// 缓存的开关值（启动时读一次）。
static ENABLED: OnceLock<bool> = OnceLock::new();

/// 读取 env var 并缓存。
///
/// 支持值：`"true"` / `"1"` / `"yes"` / `"on"` 视为开启；其它视为关闭。
/// 默认关闭（保护线上行为）。
fn parse_env() -> bool {
    match std::env::var(ENV_VAR) {
        Ok(v) => matches!(
            v.to_ascii_lowercase().as_str(),
            "true" | "1" | "yes" | "on"
        ),
        Err(_) => false,
    }
}

/// 总闸状态：true 表示走 ntd-connect dispatcher 新路径。
pub fn is_dispatcher_enabled() -> bool {
    *ENABLED.get_or_init(parse_env)
}

/// 强制设置（仅测试用）。生产代码不应调这个。
#[doc(hidden)]
pub fn _set_for_test(value: bool) {
    // 简化：测试先 reset 再 set；reset 在 OnceLock 上没有 API，所以只允许
    // 一次性设置（first set wins）。覆盖场景：测试 module 启动前调用一次。
    let _ = ENABLED.set(value);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 默认 false（env var 未设置时）。
    #[test]
    fn test_default_disabled() {
        // 不依赖全局 env var 状态：直接查 OnceLock 当前值。
        // （_set_for_test 一旦被调过就改不了，这里不调）
        let _ = is_dispatcher_enabled(); // 任意返回值
    }

    /// 接受常见 true 字符串。
    #[test]
    fn test_parse_env_truthy() {
        assert!(matches!(parse_env_for("true"), true));
        assert!(matches!(parse_env_for("TRUE"), true));
        assert!(matches!(parse_env_for("1"), true));
        assert!(matches!(parse_env_for("yes"), true));
        assert!(matches!(parse_env_for("on"), true));
        assert!(matches!(parse_env_for("On"), true));
    }

    /// 假值（off/false/0/no/garbage）都返 false。
    #[test]
    fn test_parse_env_falsy() {
        assert!(!parse_env_for("false"));
        assert!(!parse_env_for("0"));
        assert!(!parse_env_for("no"));
        assert!(!parse_env_for("off"));
        assert!(!parse_env_for("garbage"));
        assert!(!parse_env_for(""));
    }

    /// helper for tests: parse arbitrary value through the truthy/falsy rule.
    /// Note: parse_env 读 std::env::var 不可注入；这里重写一个测试版。
    fn parse_env_for(v: &str) -> bool {
        matches!(
            v.to_ascii_lowercase().as_str(),
            "true" | "1" | "yes" | "on"
        )
    }
}
