//! In-memory key-value store with TTL and pub/sub channels.
//! Ephemeral — lost on restart. Use `db` (SQLite) for persistence.

use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

pub struct KvStore {
    data: HashMap<String, KvEntry>,
    channels: HashMap<String, broadcast::Sender<String>>,
}

struct KvEntry {
    value: String,
    expires_at: Option<Instant>,
}

impl KvStore {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            channels: HashMap::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        let entry = self.data.get(key)?;
        if let Some(exp) = entry.expires_at {
            if Instant::now() > exp {
                return None;
            }
        }
        Some(&entry.value)
    }

    pub fn set(&mut self, key: String, value: String, ttl: Option<u64>) {
        let expires_at = ttl.map(|s| Instant::now() + Duration::from_secs(s));
        self.data.insert(key, KvEntry { value, expires_at });
    }

    pub fn delete(&mut self, key: &str) -> bool {
        self.data.remove(key).is_some()
    }

    pub fn keys(&self, prefix: &str) -> Vec<String> {
        let now = Instant::now();
        self.data.iter()
            .filter(|(k, v)| {
                k.starts_with(prefix)
                    && v.expires_at.map_or(true, |exp| now < exp)
            })
            .map(|(k, _)| k.clone())
            .collect()
    }

    pub fn incr(&mut self, key: &str) -> i64 {
        let current = self.get(key)
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);
        let new_val = current + 1;
        // Preserve existing TTL
        let expires_at = self.data.get(key).and_then(|e| e.expires_at);
        self.data.insert(key.to_string(), KvEntry {
            value: new_val.to_string(),
            expires_at,
        });
        new_val
    }

    /// Publish a message to a channel. Returns number of receivers notified.
    pub fn publish(&self, channel: &str, data: &str) -> usize {
        if let Some(tx) = self.channels.get(channel) {
            match tx.send(data.to_string()) {
                Ok(n) => n,
                Err(_) => 0, // no active receivers
            }
        } else {
            0
        }
    }

    /// Get or create a broadcast channel, return a receiver.
    pub fn subscribe(&mut self, channel: &str) -> broadcast::Receiver<String> {
        let tx = self.channels.entry(channel.to_string())
            .or_insert_with(|| broadcast::channel(256).0);
        tx.subscribe()
    }

    /// Evict expired keys.
    pub fn evict_expired(&mut self) {
        let now = Instant::now();
        self.data.retain(|_, v| {
            v.expires_at.map_or(true, |exp| now < exp)
        });
    }
}
