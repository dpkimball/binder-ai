use std::collections::HashMap;
use std::sync::Mutex;

/// Optional caches. Implementation is abstract; this is an in-memory default.
pub trait AiCache: Send + Sync {
    fn get(&self, namespace: &str, key: &str) -> Option<String>;
    fn put(&self, namespace: &str, key: &str, value: String);
}

#[derive(Debug, Default)]
pub struct MemoryCache {
    inner: Mutex<HashMap<(String, String), String>>,
}

impl MemoryCache {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AiCache for MemoryCache {
    fn get(&self, namespace: &str, key: &str) -> Option<String> {
        self.inner
            .lock()
            .ok()?
            .get(&(namespace.to_string(), key.to_string()))
            .cloned()
    }

    fn put(&self, namespace: &str, key: &str, value: String) {
        if let Ok(mut map) = self.inner.lock() {
            map.insert((namespace.to_string(), key.to_string()), value);
        }
    }
}

pub const NS_PROMPT: &str = "prompt";
pub const NS_EMBEDDING: &str = "embedding";
pub const NS_RESPONSE: &str = "response";
