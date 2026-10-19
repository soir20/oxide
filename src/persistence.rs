use std::collections::BTreeMap;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};

use crate::backend::{Backend, CharacterSnapshot};
use crate::info;

// A snapshot stays cached this long after it is saved, so a player who logs in again right
// away gets it even if the backend's copy was read before the save landed
const KEEP_SAVED_FOR: Duration = Duration::from_secs(30);
const RETRY_DELAY: Duration = Duration::from_secs(1);

pub trait CharacterSink: Send + Sync {
    /// Queues `snapshot` to be saved. Never blocks on I/O.
    fn save(&self, guid: u32, snapshot: CharacterSnapshot);

    /// The newest snapshot of `guid` that is unsaved or was saved within the last few seconds.
    fn recent(&self, guid: u32) -> Option<CharacterSnapshot>;
}

struct Entry {
    snapshot: CharacterSnapshot,
    version: u64,
    saved_at: Option<Instant>,
}

#[derive(Default)]
struct State {
    entries: BTreeMap<u32, Entry>,
    next_version: u64,
}

impl State {
    fn next_unsaved(&self) -> Option<(u32, CharacterSnapshot, u64)> {
        self.entries
            .iter()
            .find(|(_, entry)| entry.saved_at.is_none())
            .map(|(guid, entry)| (*guid, entry.snapshot.clone(), entry.version))
    }

    fn forget_old(&mut self, now: Instant) {
        self.entries.retain(|_, entry| {
            entry
                .saved_at
                .is_none_or(|saved_at| now.duration_since(saved_at) < KEEP_SAVED_FOR)
        });
    }
}

/// Saves character snapshots on a dedicated thread so game threads never wait on the network.
/// Only the newest snapshot of each player is kept; a failed save is retried until a newer
/// snapshot replaces it.
pub struct CharacterSaver {
    state: Mutex<State>,
    changed: Condvar,
}

impl CharacterSaver {
    pub fn spawn(backend: Arc<Backend>) -> (Arc<Self>, JoinHandle<()>) {
        let saver = Arc::new(CharacterSaver {
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
        });
        let worker = saver.clone();
        let handle = thread::spawn(move || loop {
            worker.save_next(|guid, snapshot| backend.save(guid, snapshot));
        });
        (saver, handle)
    }

    /// Waits for an unsaved snapshot and saves it with `save`.
    fn save_next<F: FnOnce(u32, &CharacterSnapshot) -> Result<(), String>>(&self, save: F) {
        let (guid, snapshot, version) = {
            let mut state = self.state.lock();
            loop {
                state.forget_old(Instant::now());
                if let Some(next) = state.next_unsaved() {
                    break next;
                }
                self.changed.wait_for(&mut state, KEEP_SAVED_FOR);
            }
        };

        let result = save(guid, &snapshot);

        let mut state = self.state.lock();
        match result {
            Ok(()) => {
                if let Some(entry) = state.entries.get_mut(&guid) {
                    // A newer snapshot may have arrived while this one was being sent
                    if entry.version == version {
                        entry.saved_at = Some(Instant::now());
                    }
                }
            }
            Err(err) => {
                info!("Unable to save player {}, will retry: {}", guid, err);
                self.changed.wait_for(&mut state, RETRY_DELAY);
            }
        }
    }

    #[cfg(test)]
    fn new_for_test() -> Self {
        CharacterSaver {
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
        }
    }
}

impl CharacterSink for CharacterSaver {
    fn save(&self, guid: u32, snapshot: CharacterSnapshot) {
        let mut state = self.state.lock();
        let version = state.next_version;
        state.next_version += 1;
        state.entries.insert(
            guid,
            Entry {
                snapshot,
                version,
                saved_at: None,
            },
        );
        self.changed.notify_all();
    }

    fn recent(&self, guid: u32) -> Option<CharacterSnapshot> {
        let mut state = self.state.lock();
        state.forget_old(Instant::now());
        state.entries.get(&guid).map(|entry| entry.snapshot.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(credits: u32) -> CharacterSnapshot {
        CharacterSnapshot {
            zone_template: 24,
            pos: [0.0, 0.0, 0.0, 1.0],
            rot: [0.0; 4],
            credits,
            active_battle_class: 1,
            equipment: Vec::new(),
            customizations: Vec::new(),
        }
    }

    #[test]
    fn saves_the_newest_snapshot_only() {
        let saver = CharacterSaver::new_for_test();
        saver.save(7, snapshot(1));
        saver.save(7, snapshot(2));
        let mut saved = Vec::new();
        saver.save_next(|guid, snapshot| {
            saved.push((guid, snapshot.credits));
            Ok(())
        });
        assert_eq!(saved, vec![(7, 2)]);
        // Still available for a quick relog
        assert_eq!(saver.recent(7).unwrap().credits, 2);
    }

    #[test]
    fn retries_failed_saves() {
        let saver = CharacterSaver::new_for_test();
        saver.save(7, snapshot(1));
        saver.save_next(|_, _| Err("backend down".to_string()));
        let mut saved = None;
        saver.save_next(|guid, snapshot| {
            saved = Some((guid, snapshot.credits));
            Ok(())
        });
        assert_eq!(saved, Some((7, 1)));
    }

    #[test]
    fn a_snapshot_queued_during_a_save_is_saved_next() {
        let saver = CharacterSaver::new_for_test();
        saver.save(7, snapshot(1));
        saver.save_next(|guid, _| {
            // The player changes again while the first save is in flight
            saver.save(guid, snapshot(2));
            Ok(())
        });
        let mut saved = None;
        saver.save_next(|_, snapshot| {
            saved = Some(snapshot.credits);
            Ok(())
        });
        assert_eq!(saved, Some(2));
    }

    #[test]
    fn unknown_players_have_no_recent_snapshot() {
        assert_eq!(CharacterSaver::new_for_test().recent(7), None);
    }
}
