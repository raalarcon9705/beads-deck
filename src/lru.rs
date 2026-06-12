//! A tiny capacity-bounded LRU cache (no external deps). Used for bead details
//! so reopening recent beads is instant while staying memory-bounded.

use std::collections::HashMap;
use std::hash::Hash;

pub(crate) struct Lru<K: Eq + Hash + Clone, V> {
    cap: usize,
    map: HashMap<K, V>,
    /// Keys ordered least-recently-used (front) to most-recently-used (back).
    order: Vec<K>,
}

impl<K: Eq + Hash + Clone, V> Lru<K, V> {
    pub(crate) fn new(cap: usize) -> Self {
        Self { cap: cap.max(1), map: HashMap::new(), order: Vec::new() }
    }

    /// Fetch a value, marking it most-recently-used.
    pub(crate) fn get(&mut self, k: &K) -> Option<&V> {
        if self.map.contains_key(k) {
            self.touch(k);
            self.map.get(k)
        } else {
            None
        }
    }

    /// Insert/replace a value (most-recently-used), evicting the LRU entry if
    /// over capacity.
    pub(crate) fn insert(&mut self, k: K, v: V) {
        self.map.insert(k.clone(), v);
        self.touch(&k);
        while self.order.len() > self.cap {
            let evict = self.order.remove(0);
            self.map.remove(&evict);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }

    fn touch(&mut self, k: &K) {
        if let Some(pos) = self.order.iter().position(|x| x == k) {
            self.order.remove(pos);
        }
        self.order.push(k.clone());
    }
}
