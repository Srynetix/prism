//! A small per-key cache with a lifetime, for what the forge and the tracker hand out and that
//! changes little (members, labels, custom emoji): a `static`, read before asking the API again.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

pub struct TtlCache<T> {
    ttl: Duration,
    entries: Mutex<Option<HashMap<String, (Instant, T)>>>,
}

impl<T: Clone> TtlCache<T> {
    pub const fn new(ttl: Duration) -> Self {
        Self { ttl, entries: Mutex::new(None) }
    }

    /// The value kept for `key`, unless older than the lifetime.
    pub fn get(&self, key: &str) -> Option<T> {
        self.entries.lock().unwrap().as_ref()?.get(key).filter(|(at, _)| at.elapsed() < self.ttl).map(|(_, v)| v.clone())
    }

    pub fn put(&self, key: &str, value: T) {
        self.entries.lock().unwrap().get_or_insert_with(HashMap::new).insert(key.to_string(), (Instant::now(), value));
    }

    /// The next read of `key` asks again (after a change made by Prism).
    pub fn forget(&self, key: &str) {
        if let Some(c) = self.entries.lock().unwrap().as_mut() {
            c.remove(key);
        }
    }

    /// Everything asked again (another account or workspace after a config reload).
    pub fn clear(&self) {
        *self.entries.lock().unwrap() = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_per_key_until_its_lifetime() {
        let c = TtlCache::new(Duration::from_secs(60));
        assert_eq!(c.get("a"), None::<u8>);
        c.put("a", 1);
        c.put("b", 2);
        assert_eq!((c.get("a"), c.get("b")), (Some(1), Some(2)));
        c.forget("a");
        assert_eq!((c.get("a"), c.get("b")), (None, Some(2)));
        c.clear();
        assert_eq!(c.get("b"), None);
        let gone = TtlCache::new(Duration::ZERO);
        gone.put("a", 1);
        assert_eq!(gone.get("a"), None);
    }
}
