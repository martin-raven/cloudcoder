use std::collections::HashMap;

use cloudcoder_core::{CacheOptions, CacheStats};

struct Node<T> {
    key: String,
    value: T,
    expires_at: u64,
    prev: Option<usize>,
    next: Option<usize>,
}

/// LRU cache with TTL support
pub struct MemoryCache<T: Clone + Send + Sync + 'static> {
    entries: HashMap<String, usize>,
    nodes: Vec<Node<T>>,
    head: Option<usize>,
    tail: Option<usize>,
    free_list: Vec<usize>,
    max_size: usize,
    ttl_ms: u64,
    stats: CacheStats,
}

impl<T: Clone + Send + Sync + 'static> MemoryCache<T> {
    pub fn new(options: CacheOptions) -> Self {
        Self {
            entries: HashMap::new(),
            nodes: Vec::new(),
            head: None,
            tail: None,
            free_list: Vec::new(),
            max_size: options.max_size,
            ttl_ms: options.ttl_ms,
            stats: CacheStats::default(),
        }
    }

    fn current_time_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    fn allocate_node(&mut self, key: String, value: T, expires_at: u64) -> usize {
        if let Some(idx) = self.free_list.pop() {
            self.nodes[idx] = Node {
                key,
                value,
                expires_at,
                prev: None,
                next: None,
            };
            idx
        } else {
            let idx = self.nodes.len();
            self.nodes.push(Node {
                key,
                value,
                expires_at,
                prev: None,
                next: None,
            });
            idx
        }
    }

    fn remove_from_list(&mut self, idx: usize) {
        let (prev, next) = {
            let node = &self.nodes[idx];
            (node.prev, node.next)
        };

        if let Some(p) = prev {
            self.nodes[p].next = next;
        } else {
            self.head = next;
        }

        if let Some(n) = next {
            self.nodes[n].prev = prev;
        } else {
            self.tail = prev;
        }

        self.nodes[idx].prev = None;
        self.nodes[idx].next = None;
    }

    fn add_to_front(&mut self, idx: usize) {
        self.nodes[idx].prev = None;
        self.nodes[idx].next = self.head;

        if let Some(h) = self.head {
            self.nodes[h].prev = Some(idx);
        }
        self.head = Some(idx);

        if self.tail.is_none() {
            self.tail = Some(idx);
        }
    }

    fn move_to_front(&mut self, idx: usize) {
        self.remove_from_list(idx);
        self.add_to_front(idx);
    }

    fn evict_tail(&mut self) {
        if let Some(tail_idx) = self.tail {
            let key = self.nodes[tail_idx].key.clone();
            self.entries.remove(&key);
            self.remove_from_list(tail_idx);
            self.free_list.push(tail_idx);
            self.stats.evictions += 1;
        }
    }

    /// Insert or update a key-value pair
    pub fn set(&mut self, key: String, value: T) {
        let now = Self::current_time_ms();
        let expires_at = now.saturating_add(self.ttl_ms);

        // If key exists, update and move to front
        if let Some(&idx) = self.entries.get(&key) {
            self.nodes[idx].value = value;
            self.nodes[idx].expires_at = expires_at;
            self.move_to_front(idx);
            return;
        }

        // Check capacity
        while self.entries.len() >= self.max_size {
            self.evict_tail();
        }

        // Insert new node
        let idx = self.allocate_node(key.clone(), value, expires_at);
        self.add_to_front(idx);
        self.entries.insert(key, idx);
    }

    /// Get a value if it exists and hasn't expired
    pub fn get(&mut self, key: &str) -> Option<T> {
        let idx = *self.entries.get(key)?;
        let now = Self::current_time_ms();

        // Check expiration
        let expires_at = self.nodes[idx].expires_at;
        if now > expires_at {
            // Expired - remove it
            self.entries.remove(key);
            self.remove_from_list(idx);
            self.free_list.push(idx);
            self.stats.misses += 1;
            return None;
        }

        // Move to front (LRU)
        self.move_to_front(idx);
        self.stats.hits += 1;
        Some(self.nodes[idx].value.clone())
    }

    /// Check if a key exists without updating LRU or stats
    pub fn has(&self, key: &str) -> bool {
        let now = Self::current_time_ms();
        if let Some(&idx) = self.entries.get(key) {
            let expires_at = self.nodes[idx].expires_at;
            now <= expires_at
        } else {
            false
        }
    }

    /// Remove a key from the cache
    pub fn delete(&mut self, key: &str) -> bool {
        if let Some(idx) = self.entries.remove(key) {
            self.remove_from_list(idx);
            self.free_list.push(idx);
            true
        } else {
            false
        }
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
        self.nodes.clear();
        self.free_list.clear();
        self.head = None;
        self.tail = None;
    }

    /// Get cache statistics
    pub fn get_stats(&self) -> CacheStats {
        CacheStats {
            size: self.entries.len(),
            hits: self.stats.hits,
            misses: self.stats.misses,
            evictions: self.stats.evictions,
        }
    }

    /// Get current cache size
    pub fn size(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        let mut cache: MemoryCache<String> = MemoryCache::new(CacheOptions {
            max_size: 3,
            ttl_ms: 60000,
        });

        cache.set("a".to_string(), "value_a".to_string());
        cache.set("b".to_string(), "value_b".to_string());
        cache.set("c".to_string(), "value_c".to_string());

        assert_eq!(cache.get("a"), Some("value_a".to_string()));
        assert_eq!(cache.get("b"), Some("value_b".to_string()));
        assert_eq!(cache.get("c"), Some("value_c".to_string()));
        assert!(cache.has("a"));
    }

    #[test]
    fn test_lru_eviction() {
        let mut cache: MemoryCache<i32> = MemoryCache::new(CacheOptions {
            max_size: 2,
            ttl_ms: 60000,
        });

        cache.set("a".to_string(), 1);
        cache.set("b".to_string(), 2);
        cache.set("c".to_string(), 3); // Should evict 'a'

        assert!(!cache.has("a"));
        assert!(cache.has("b"));
        assert!(cache.has("c"));
    }
}