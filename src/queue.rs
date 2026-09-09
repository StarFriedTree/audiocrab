use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub struct PlaybackQueue {
    ordered_ids: Vec<String>,
    cursor: usize,
    mode: QueueMode,
    seed: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueMode {
    Shuffle,
    Ascending,
    Descending,
}

impl PlaybackQueue {
    /// Create a new queue with shuffled order from weighted pool using a seed
    pub fn new_shuffle(weighted_ids: Vec<(String, f64)>, seed: u64) -> Self {
        let ordered_ids = Self::generate_weighted_shuffle(&weighted_ids, seed);
        Self {
            ordered_ids,
            cursor: 0,
            mode: QueueMode::Shuffle,
            seed: Some(seed),
        }
    }

    /// Create a new queue with sorted IDs
    pub fn new_sorted(ids: Vec<String>, mode: QueueMode) -> Self {
        assert!(
            mode != QueueMode::Shuffle,
            "use new_shuffle for shuffle mode"
        );
        Self {
            ordered_ids: ids,
            cursor: 0,
            mode,
            seed: None,
        }
    }

    /// Reshuffle in place with a new seed (for shuffle mode only)
    pub fn reshuffle(&mut self, weighted_ids: Vec<(String, f64)>, seed: u64) {
        if self.mode == QueueMode::Shuffle {
            self.ordered_ids = Self::generate_weighted_shuffle(&weighted_ids, seed);
            self.cursor = 0;
            self.seed = Some(seed);
        }
    }

    /// Get the ID of the currently playing track
    pub fn current_id(&self) -> Option<&str> {
        self.ordered_ids.get(self.cursor).map(|s| s.as_str())
    }

    /// Peek at the next track without advancing
    pub fn next_id(&self) -> Option<&str> {
        if self.ordered_ids.is_empty() {
            return None;
        }
        let next_cursor = (self.cursor + 1) % self.ordered_ids.len();
        self.ordered_ids.get(next_cursor).map(|s| s.as_str())
    }

    pub fn peek_ahead(&self, offset: usize) -> Option<&str> {
        if self.ordered_ids.is_empty() {
            return None;
        }
        let idx = (self.cursor + offset) % self.ordered_ids.len();
        self.ordered_ids.get(idx).map(|s| s.as_str())
    }

    /// Advance to the next track (wraps around)
    pub fn advance(&mut self) {
        if !self.ordered_ids.is_empty() {
            self.cursor = (self.cursor + 1) % self.ordered_ids.len();
        }
    }

    /// Move to the previous track (wraps around)
    pub fn previous(&mut self) {
        if !self.ordered_ids.is_empty() {
            self.cursor = if self.cursor == 0 {
                self.ordered_ids.len() - 1
            } else {
                self.cursor - 1
            };
        }
    }

    /// Get current cursor position
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Set cursor position
    pub fn set_cursor(&mut self, pos: usize) {
        if pos < self.ordered_ids.len() {
            self.cursor = pos;
        }
    }

    pub fn set_current_id(&mut self, id: &str) {
        if let Some(position) = self
            .ordered_ids
            .iter()
            .position(|candidate| candidate == id)
        {
            self.cursor = position;
        }
    }

    /// Get queue length
    pub fn len(&self) -> usize {
        self.ordered_ids.len()
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.ordered_ids.is_empty()
    }

    /// Get the current mode
    pub fn mode(&self) -> QueueMode {
        self.mode
    }

    pub fn seed(&self) -> Option<u64> {
        self.seed
    }

    /// Generate a weighted shuffle using deterministic hashing
    /// This creates a stable ordering based on (id, seed) hash pairs
    fn generate_weighted_shuffle(weighted_ids: &[(String, f64)], seed: u64) -> Vec<String> {
        if weighted_ids.is_empty() {
            return Vec::new();
        }

        // Create (key, id) pairs using hash-based deterministic scores
        let mut keyed: Vec<(f64, String)> = weighted_ids
            .iter()
            .map(|(id, weight)| {
                let mut hasher = DefaultHasher::new();
                id.hash(&mut hasher);
                seed.hash(&mut hasher);
                let hash = hasher.finish() as f64;
                // Normalize hash to [0, 1) and apply weight
                let normalized = (hash % 1000000000.0) / 1000000000.0;
                let key = -(normalized.max(f64::EPSILON).ln()) / weight.max(0.001);
                (key, id.clone())
            })
            .collect();

        // Sort by key in descending order
        keyed.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        keyed.into_iter().map(|(_, id)| id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shuffle_deterministic() {
        let weighted = vec![
            ("id1".to_string(), 1.0),
            ("id2".to_string(), 2.0),
            ("id3".to_string(), 1.0),
        ];

        let q1 = PlaybackQueue::new_shuffle(weighted.clone(), 42);
        let q2 = PlaybackQueue::new_shuffle(weighted, 42);

        assert_eq!(q1.ordered_ids, q2.ordered_ids);
    }

    #[test]
    fn test_advance_wraps() {
        let ids = vec!["id1".to_string(), "id2".to_string(), "id3".to_string()];
        let mut q = PlaybackQueue::new_sorted(ids, QueueMode::Ascending);

        q.advance();
        assert_eq!(q.current_id(), Some("id2"));
        q.advance();
        assert_eq!(q.current_id(), Some("id3"));
        q.advance();
        assert_eq!(q.current_id(), Some("id1")); // wraps
    }

    #[test]
    fn test_previous_wraps() {
        let ids = vec!["id1".to_string(), "id2".to_string(), "id3".to_string()];
        let mut q = PlaybackQueue::new_sorted(ids, QueueMode::Ascending);

        q.previous();
        assert_eq!(q.current_id(), Some("id3")); // wraps
    }
}
