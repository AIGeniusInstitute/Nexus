//! File-backed event store for persisting the app-server event stream.
//!
//! Each event is identified by the triple `(thread_id, turn_id, item_seq)`.
//! The store uses a JSON file as the persistence layer with an in-memory
//! `HashSet` for O(1) idempotency checking: re-delivering the same event
//! after a resume does not create duplicate entries.
//!
//! In M1 this will be replaced by PostgreSQL with `INSERT ... ON CONFLICT
//! DO NOTHING` for production-grade idempotency.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Composite key: (thread_id, turn_id, item_seq).
#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
struct EventKey {
    thread_id: String,
    turn_id: String,
    item_seq: i64,
}

/// One persisted event row.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct EventRecord {
    key: EventKey,
    event_type: String,
    payload: String,
    ts: i64,
}

/// File-backed store for app-server events. One record per event notification.
pub struct EventStore {
    path: PathBuf,
    records: Vec<EventRecord>,
    seen: HashSet<EventKey>,
}

impl EventStore {
    /// Open (or create) the event store at the given path. Loads existing
    /// records from the file if it exists.
    pub fn open(path: &Path) -> Result<Self> {
        let records: Vec<EventRecord> = if path.exists() {
            let data = std::fs::read_to_string(path)
                .with_context(|| format!("failed to read event store at {}", path.display()))?;
            if data.trim().is_empty() {
                Vec::new()
            } else {
                serde_json::from_str(&data)
                    .with_context(|| format!("failed to parse event store at {}", path.display()))?
            }
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            Vec::new()
        };

        let seen = records.iter().map(|r| r.key.clone()).collect();

        Ok(Self {
            path: path.to_path_buf(),
            records,
            seen,
        })
    }

    /// Insert an event. Idempotent: re-inserting the same
    /// `(thread_id, turn_id, item_seq)` triple is a no-op.
    pub fn upsert_event(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        seq: i64,
        etype: &str,
        payload: &str,
    ) -> Result<bool> {
        let key = EventKey {
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
            item_seq: seq,
        };

        if self.seen.contains(&key) {
            return Ok(false); // duplicate — idempotent skip
        }

        let record = EventRecord {
            key: key.clone(),
            event_type: etype.to_string(),
            payload: payload.to_string(),
            ts: unix_millis(),
        };
        self.records.push(record);
        self.seen.insert(key);
        self.flush()?;
        Ok(true)
    }

    /// Return the maximum `item_seq` for the given thread/turn pair, or 0 if
    /// no events exist.
    pub fn max_seq(&self, thread_id: &str, turn_id: &str) -> i64 {
        self.records
            .iter()
            .filter(|r| r.key.thread_id == thread_id && r.key.turn_id == turn_id)
            .map(|r| r.key.item_seq)
            .max()
            .unwrap_or(0)
    }

    /// Total number of events in the store.
    pub fn count(&self) -> usize {
        self.records.len()
    }

    /// Persist the current state to the backing file.
    fn flush(&self) -> Result<()> {
        let data = serde_json::to_string_pretty(&self.records)
            .context("failed to serialize event store")?;
        std::fs::write(&self.path, data)
            .with_context(|| format!("failed to write event store to {}", self.path.display()))?;
        Ok(())
    }
}

/// Current Unix time in milliseconds.
fn unix_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    duration.as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> EventStore {
        let dir = std::env::temp_dir().join("nexus-control-test");
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join(format!(
            "test-{}.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        EventStore::open(&path).expect("open store")
    }

    /// AC2.2: Re-delivering the same event triple must not create a duplicate
    /// record.
    #[test]
    fn test_idempotent_insert() {
        let mut store = temp_store();

        let inserted = store
            .upsert_event("thr_1", "turn_1", 1, "item/started", "{}")
            .unwrap();
        assert!(inserted);
        assert_eq!(store.count(), 1);

        // Re-insert the same triple — should be skipped.
        let inserted2 = store
            .upsert_event("thr_1", "turn_1", 1, "item/started", "{}")
            .unwrap();
        assert!(!inserted2);
        assert_eq!(store.count(), 1, "duplicate insert should be ignored");

        // A different seq is a new record.
        store
            .upsert_event("thr_1", "turn_1", 2, "item/completed", "{}")
            .unwrap();
        assert_eq!(store.count(), 2);
    }

    /// `max_seq` returns the highest seq for a (thread, turn) pair.
    #[test]
    fn test_max_seq() {
        let mut store = temp_store();

        assert_eq!(store.max_seq("thr_1", "turn_1"), 0);

        store.upsert_event("thr_1", "turn_1", 3, "a", "{}").unwrap();
        store.upsert_event("thr_1", "turn_1", 7, "b", "{}").unwrap();
        store.upsert_event("thr_1", "turn_1", 1, "c", "{}").unwrap();

        assert_eq!(store.max_seq("thr_1", "turn_1"), 7);

        // Different turn has no events.
        assert_eq!(store.max_seq("thr_1", "turn_2"), 0);
    }

    /// Persistence: reopening the store should load previously written
    /// records.
    #[test]
    fn test_persistence() {
        let path = std::env::temp_dir().join("nexus-control-test/persist-test.json");
        let _ = std::fs::remove_file(&path);

        {
            let mut store = EventStore::open(&path).expect("open store");
            store
                .upsert_event("thr_p", "turn_p", 5, "test", "{}")
                .unwrap();
            assert_eq!(store.count(), 1);
        }

        // Reopen — data should still be there.
        let store = EventStore::open(&path).expect("reopen store");
        assert_eq!(store.count(), 1);
        assert_eq!(store.max_seq("thr_p", "turn_p"), 5);
    }
}
