//! 多 bot 飞书 channel 注册表。
//!
//! # 与 cc-connect / ntd-connect 的对应
//!
//! 对应 `ntd-connect::Dispatcher` 期望的多 channel 输入。Dispatcher
//! 通过 `ChannelRegistry::get(&bot_id)` 拿到对应 bot 的 `Arc<FeishuPlatform>`，
//! 调 `channel.reply / send / start_typing`。
//!
//! 当前实现是「过渡期」：保留 `FeishuListener` 作为旧路径，`ChannelRegistry`
//! 是新 dispatcher 的 channel 入口。切流完成（步骤 11）后删 FeishuListener。
//!
//! # 设计
//!
//! - `DashMap<i64, Arc<FeishuPlatform>>` 支持多 reader / 多 writer，
//!   HTTP handler 查 bot_id 与后台 start bot 是典型多 writer 场景。
//! - platform 由 `register()` 接管所有权，外部不再持有裸 Arc。
//! - `take(bot_id)` 返回 Option 给「停止 bot 时取出并 stop」用。

use std::sync::Arc;

use dashmap::DashMap;
use ntd_connect::platform::feishu::FeishuPlatform;

/// `bot_id` → 已构造的 FeishuPlatform。
///
/// `Clone` 是 cheap（内部 Arc）。
#[derive(Clone, Default)]
pub struct ChannelRegistry {
    inner: Arc<DashMap<i64, Arc<FeishuPlatform>>>,
}

impl ChannelRegistry {
    /// 构造空 registry。
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个 bot 的 FeishuPlatform。重复注册同一 bot_id 视为覆盖
    /// （v1 简单方案；v2 可加日志或拒绝）。
    pub fn register(&self, bot_id: i64, platform: Arc<FeishuPlatform>) {
        self.inner.insert(bot_id, platform);
    }

    /// 取出 bot_id 对应的 platform（move 出 Option）。返回 None 表示未注册。
    pub fn take(&self, bot_id: i64) -> Option<Arc<FeishuPlatform>> {
        self.inner.remove(&bot_id).map(|(_, p)| p)
    }

    /// 查 bot_id 是否已注册（不取出）。
    pub fn contains(&self, bot_id: i64) -> bool {
        self.inner.contains_key(&bot_id)
    }

    /// 取 bot_id 对应的 platform 引用。返回 None 表示未注册。
    pub fn get(&self, bot_id: i64) -> Option<Arc<FeishuPlatform>> {
        self.inner.get(&bot_id).map(|r| r.clone())
    }

    /// 当前注册的 bot 数。
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntd_connect::http::SharedHttpClient;
    use ntd_connect::platform::feishu::{FeishuConfig, FeishuDomain};

    fn make_platform(app_id: &str) -> Arc<FeishuPlatform> {
        Arc::new(FeishuPlatform::new(
            FeishuConfig {
                app_id: app_id.into(),
                app_secret: "secret".into(),
                domain: FeishuDomain::Feishu,
                bot_open_id: None,
            },
            SharedHttpClient::new(),
        ))
    }

    /// register / get / contains / take / len 全部行为正确。
    #[test]
    fn test_register_get_take() {
        let reg = ChannelRegistry::new();
        assert!(reg.is_empty());

        let p1 = make_platform("a");
        reg.register(1, p1.clone());
        assert!(reg.contains(1));
        assert_eq!(reg.len(), 1);

        // get 返回 Arc（clone 出新引用，不 move）。
        let got = reg.get(1).unwrap();
        assert!(Arc::ptr_eq(&got, &p1));

        // take move 出 Option。
        let taken = reg.take(1).unwrap();
        assert!(Arc::ptr_eq(&taken, &p1));
        assert!(!reg.contains(1));
        assert!(reg.is_empty());

        // 重复 take 返回 None。
        assert!(reg.take(1).is_none());
    }

    /// register 同一 bot_id 视为覆盖。
    #[test]
    fn test_register_overwrites() {
        let reg = ChannelRegistry::new();
        reg.register(1, make_platform("a"));
        reg.register(1, make_platform("b"));
        assert_eq!(reg.len(), 1, "同一 bot_id 二次 register 应覆盖");
    }

    /// 没注册的 bot_id：contains / get / take 都返回 false / None。
    #[test]
    fn test_missing_bot_returns_none() {
        let reg = ChannelRegistry::new();
        assert!(!reg.contains(999));
        assert!(reg.get(999).is_none());
        assert!(reg.take(999).is_none());
    }
}
