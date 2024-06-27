use std::collections::BTreeMap;

use parking_lot::{RwLockReadGuard, RwLockWriteGuard};

use crate::game_server::guid::GuidTable;
use crate::game_server::zone::{Character, CharacterCategory, Zone};

use super::guid::{GuidTableHandle, GuidTableReadHandle, GuidTableWriteHandle};

pub struct TableReadHandleWrapper<'a, K, V, I = ()> {
    handle: GuidTableReadHandle<'a, K, V, I>,
}

impl<K: Copy + Ord, V, I: Copy + Ord> TableReadHandleWrapper<'_, K, V, I> {
    pub fn contains(&self, guid: K) -> bool {
        self.handle.get(guid).is_some()
    }

    pub fn keys(&self) -> impl Iterator<Item = K> + '_ {
        self.handle.keys()
    }

    pub fn keys_by_index(&self, index: I) -> impl Iterator<Item = K> + '_ {
        self.handle.keys_by_index(index)
    }
}

impl<'a, K, V, I> From<GuidTableReadHandle<'a, K, V, I>> for TableReadHandleWrapper<'a, K, V, I> {
    fn from(value: GuidTableReadHandle<'a, K, V, I>) -> Self {
        TableReadHandleWrapper { handle: value }
    }
}

pub type CharacterTableReadHandle<'a> =
    TableReadHandleWrapper<'a, u64, Character, (u64, CharacterCategory)>;
pub type CharacterTableWriteHandle<'a> =
    GuidTableWriteHandle<'a, u64, Character, (u64, CharacterCategory)>;
pub type CharacterReadGuard<'a> = RwLockReadGuard<'a, Character>;
pub type CharacterWriteGuard<'a> = RwLockWriteGuard<'a, Character>;
pub type ZoneTableReadHandle<'a> = TableReadHandleWrapper<'a, u64, Zone>;
pub type ZoneTableWriteHandle<'a> = GuidTableWriteHandle<'a, u64, Zone>;
pub type ZoneReadGuard<'a> = RwLockReadGuard<'a, Zone>;
pub type ZoneWriteGuard<'a> = RwLockWriteGuard<'a, Zone>;

pub struct ZoneLockRequest<
    R,
    F: FnOnce(
        &ZoneTableReadHandle<'_>,
        BTreeMap<u64, ZoneReadGuard<'_>>,
        BTreeMap<u64, ZoneWriteGuard<'_>>,
    ) -> R,
> {
    read_guids: Vec<u64>,
    write_guids: Vec<u64>,
    zone_consumer: F,
}

pub struct ZoneLockEnforcer<'a> {
    zones: &'a GuidTable<u64, Zone>,
}

impl ZoneLockEnforcer<'_> {
    pub fn read_zones<
        R,
        Z: FnOnce(
            &ZoneTableReadHandle<'_>,
            BTreeMap<u64, ZoneReadGuard<'_>>,
            BTreeMap<u64, ZoneWriteGuard<'_>>,
        ) -> R,
        T: FnOnce(&ZoneTableReadHandle<'_>) -> ZoneLockRequest<R, Z>,
    >(
        &self,
        table_consumer: T,
    ) -> R {
        let zones_table_read_handle = self.zones.read().into();
        let mut zone_lock_request = table_consumer(&zones_table_read_handle);
        zone_lock_request.read_guids.sort();
        zone_lock_request.write_guids.sort();

        let mut zones_read_map = BTreeMap::new();
        for guid in zone_lock_request.read_guids {
            if let Some(lock) = zones_table_read_handle.handle.get(guid) {
                zones_read_map.insert(guid, lock.read());
            }
        }

        let mut zones_write_map = BTreeMap::new();
        for guid in zone_lock_request.write_guids {
            if let Some(lock) = zones_table_read_handle.handle.get(guid) {
                zones_write_map.insert(guid, lock.write());
            }
        }

        (zone_lock_request.zone_consumer)(&zones_table_read_handle, zones_read_map, zones_write_map)
    }

    // This thread can access individual zones if and only if it holds the table read or write lock.
    // If this thread holds the table write lock, then no other threads may hold a table lock.
    // Therefore, if this thread holds the table write lock, it is the only thread that can hold any
    // zone locks, and we can provide full access to the table without fear of deadlock.
    pub fn write_zones<R, T: FnOnce(&ZoneTableWriteHandle) -> R>(&self, table_consumer: T) -> R {
        let zones_table_write_handle = self.zones.write();
        table_consumer(&zones_table_write_handle)
    }
}

pub struct CharacterLockRequest<
    R,
    F: FnOnce(
        &CharacterTableReadHandle<'_>,
        BTreeMap<u64, CharacterReadGuard<'_>>,
        BTreeMap<u64, CharacterWriteGuard<'_>>,
        &ZoneLockEnforcer,
    ) -> R,
> {
    read_guids: Vec<u64>,
    write_guids: Vec<u64>,
    character_consumer: F,
}

pub struct LockEnforcer<'a> {
    characters: &'a GuidTable<u64, Character, (u64, CharacterCategory)>,
    zones: &'a GuidTable<u64, Zone>,
}

impl LockEnforcer<'_> {
    pub fn read_characters<
        R,
        C: FnOnce(
            &CharacterTableReadHandle<'_>,
            BTreeMap<u64, CharacterReadGuard<'_>>,
            BTreeMap<u64, CharacterWriteGuard<'_>>,
            &ZoneLockEnforcer,
        ) -> R,
        T: FnOnce(&CharacterTableReadHandle<'_>) -> CharacterLockRequest<R, C>,
    >(
        &self,
        table_consumer: T,
    ) -> R {
        let characters_table_read_handle = self.characters.read().into();
        let mut character_lock_request: CharacterLockRequest<R, C> =
            table_consumer(&characters_table_read_handle);
        character_lock_request.read_guids.sort();
        character_lock_request.write_guids.sort();

        let mut characters_read_map = BTreeMap::new();
        for guid in character_lock_request.read_guids {
            if let Some(lock) = characters_table_read_handle.handle.get(guid) {
                characters_read_map.insert(guid, lock.read());
            }
        }

        let mut characters_write_map = BTreeMap::new();
        for guid in character_lock_request.write_guids {
            if let Some(lock) = characters_table_read_handle.handle.get(guid) {
                characters_write_map.insert(guid, lock.write());
            }
        }

        let zones_enforcer = ZoneLockEnforcer { zones: self.zones };
        (character_lock_request.character_consumer)(
            &characters_table_read_handle,
            characters_read_map,
            characters_write_map,
            &zones_enforcer,
        )
    }

    // This thread can access individual characters if and only if it holds the table read or write lock.
    // If this thread holds the table write lock, then no other threads may hold a table lock.
    // Therefore, if this thread holds the table write lock, it is the only thread that can hold any
    // character locks, and we can provide full access to the table without fear of deadlock.
    pub fn write_characters<R, T: FnOnce(&CharacterTableWriteHandle, &ZoneLockEnforcer) -> R>(
        &self,
        table_consumer: T,
    ) -> R {
        let characters_table_write_handle = self.characters.write();
        let zones_enforcer = ZoneLockEnforcer { zones: self.zones };
        table_consumer(&characters_table_write_handle, &zones_enforcer)
    }
}

impl<'a> From<LockEnforcer<'a>> for ZoneLockEnforcer<'a> {
    fn from(val: LockEnforcer<'a>) -> Self {
        ZoneLockEnforcer { zones: val.zones }
    }
}

pub struct LockEnforcerSource {
    characters: GuidTable<u64, Character, (u64, CharacterCategory)>,
    zones: GuidTable<u64, Zone>,
}

impl LockEnforcerSource {
    pub fn from(
        characters: GuidTable<u64, Character, (u64, CharacterCategory)>,
        zones: GuidTable<u64, Zone>,
    ) -> LockEnforcerSource {
        LockEnforcerSource { characters, zones }
    }

    pub fn lock_enforcer(&self) -> LockEnforcer {
        LockEnforcer {
            characters: &self.characters,
            zones: &self.zones,
        }
    }
}
