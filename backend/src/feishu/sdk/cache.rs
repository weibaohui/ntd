use std::time::Duration;

use quick_cache::sync::Cache;
use tokio::time::Instant;

#[derive(Debug)]
pub struct QuickCache<T: Clone> {
    cache: Cache<String, (T, Instant)>,
}

impl<T: Clone> Default for QuickCache<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone> QuickCache<T> {
    pub fn new() -> Self {
        Self::with_capacity(10)
    }

    /// 指定容量构造（106：事件去重需要比默认 10 大得多的 LRU 窗口）。
    pub fn with_capacity(cap: usize) -> Self {
        let cache = Cache::new(cap);
        Self { cache }
    }

    pub fn get(&self, key: &str) -> Option<T> {
        match self.cache.get(key) {
            None => None,
            Some((value, expire_time)) => {
                if expire_time > Instant::now() {
                    Some(value)
                } else {
                    self.cache.remove(key);
                    None
                }
            }
        }
    }

    pub fn set(&mut self, key: &str, value: T, expire_time: i32) {
        let expire_time = Instant::now() + Duration::from_secs(expire_time as u64);
        self.cache.insert(key.to_string(), (value, expire_time));
    }
}
