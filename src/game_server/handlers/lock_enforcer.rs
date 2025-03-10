use std::{
    collections::{BTreeMap, BTreeSet},
    ops::RangeBounds,
};

use parking_lot::{RwLockReadGuard, RwLockWriteGuard};

use super::{
    character::{
        Character, CharacterLocationIndex, CharacterMatchmakingGroupIndex, CharacterNameIndex,
        CharacterSquadIndex,
    },
    guid::{
        GuidTable, GuidTableHandle, GuidTableIndexer, GuidTableReadHandle, GuidTableWriteHandle,
    },
    zone::ZoneInstance,
};

pub struct TableReadHandleWrapper<'a, K, V, I1 = (), I2 = (), I3 = (), I4 = ()> {
    handle: GuidTableReadHandle<'a, K, V, I1, I2, I3, I4>,
}

impl<'a, K: Copy + Ord, V, I1: Copy + Ord, I2: Clone + Ord, I3: Clone + Ord, I4: Clone + Ord>
    GuidTableIndexer<'a, K, V, I1, I2, I3, I4>
    for TableReadHandleWrapper<'a, K, V, I1, I2, I3, I4>
{
    fn index1(&self, guid: K) -> Option<I1> {
        self.handle.index1(guid)
    }

    fn index2(&self, guid: K) -> Option<&I2> {
        self.handle.index2(guid)
    }

    fn index3(&self, guid: K) -> Option<&I3> {
        self.handle.index3(guid)
    }

    fn index4(&self, guid: K) -> Option<&I4> {
        self.handle.index4(guid)
    }

    fn keys(&'a self) -> impl Iterator<Item = K> {
        self.handle.keys()
    }

    fn keys_by_index1(&'a self, index: I1) -> impl Iterator<Item = K> {
        self.handle.keys_by_index1(index)
    }

    fn keys_by_index2(&'a self, index: &I2) -> impl Iterator<Item = K> {
        self.handle.keys_by_index2(index)
    }

    fn keys_by_index3(&'a self, index: &I3) -> impl Iterator<Item = K> {
        self.handle.keys_by_index3(index)
    }

    fn keys_by_index4(&'a self, index: &I4) -> impl Iterator<Item = K> {
        self.handle.keys_by_index4(index)
    }

    fn keys_by_index1_range(
        &'a self,
        range: impl RangeBounds<I1>,
    ) -> impl DoubleEndedIterator<Item = K> {
        self.handle.keys_by_index1_range(range)
    }

    fn keys_by_index2_range(
        &'a self,
        range: impl RangeBounds<I2>,
    ) -> impl DoubleEndedIterator<Item = K> {
        self.handle.keys_by_index2_range(range)
    }

    fn keys_by_index3_range(
        &'a self,
        range: impl RangeBounds<I3>,
    ) -> impl DoubleEndedIterator<Item = K> {
        self.handle.keys_by_index3_range(range)
    }

    fn keys_by_index4_range(
        &'a self,
        range: impl RangeBounds<I4>,
    ) -> impl DoubleEndedIterator<Item = K> {
        self.handle.keys_by_index4_range(range)
    }

    fn indices1(&'a self) -> impl Iterator<Item = I1> {
        self.handle.indices1()
    }

    fn indices2(&'a self) -> impl Iterator<Item = &'a I2> {
        self.handle.indices2()
    }

    fn indices3(&'a self) -> impl Iterator<Item = &'a I3> {
        self.handle.indices3()
    }

    fn indices4(&'a self) -> impl Iterator<Item = &'a I4> {
        self.handle.indices4()
    }

    fn indices1_by_range(
        &'a self,
        range: impl RangeBounds<I1>,
    ) -> impl DoubleEndedIterator<Item = I1> {
        self.handle.indices1_by_range(range)
    }

    fn indices2_by_range(
        &'a self,
        range: impl RangeBounds<I2>,
    ) -> impl DoubleEndedIterator<Item = &'a I2> {
        self.handle.indices2_by_range(range)
    }

    fn indices3_by_range(
        &'a self,
        range: impl RangeBounds<I3>,
    ) -> impl DoubleEndedIterator<Item = &'a I3> {
        self.handle.indices3_by_range(range)
    }

    fn indices4_by_range(
        &'a self,
        range: impl RangeBounds<I4>,
    ) -> impl DoubleEndedIterator<Item = &'a I4> {
        self.handle.indices4_by_range(range)
    }
}

impl<'a, K, V, I1, I2, I3, I4> From<GuidTableReadHandle<'a, K, V, I1, I2, I3, I4>>
    for TableReadHandleWrapper<'a, K, V, I1, I2, I3, I4>
{
    fn from(value: GuidTableReadHandle<'a, K, V, I1, I2, I3, I4>) -> Self {
        TableReadHandleWrapper { handle: value }
    }
}

pub type CharacterTableReadHandle<'a> = TableReadHandleWrapper<
    'a,
    u64,
    Character,
    CharacterLocationIndex,
    CharacterNameIndex,
    CharacterSquadIndex,
    CharacterMatchmakingGroupIndex,
>;
pub type CharacterTableWriteHandle<'a> = GuidTableWriteHandle<
    'a,
    u64,
    Character,
    CharacterLocationIndex,
    CharacterNameIndex,
    CharacterSquadIndex,
    CharacterMatchmakingGroupIndex,
>;
pub type CharacterReadGuard<'a> = RwLockReadGuard<'a, Character>;
pub type CharacterWriteGuard<'a> = RwLockWriteGuard<'a, Character>;
pub type ZoneTableReadHandle<'a> = TableReadHandleWrapper<'a, u64, ZoneInstance, u8>;
pub type ZoneTableWriteHandle<'a> = GuidTableWriteHandle<'a, u64, ZoneInstance, u8>;
pub type ZoneReadGuard<'a> = RwLockReadGuard<'a, ZoneInstance>;
pub type ZoneWriteGuard<'a> = RwLockWriteGuard<'a, ZoneInstance>;

pub struct ZoneLockRequest<
    R,
    F: FnOnce(
        &ZoneTableReadHandle<'_>,
        BTreeMap<u64, ZoneReadGuard<'_>>,
        BTreeMap<u64, ZoneWriteGuard<'_>>,
    ) -> R,
> {
    pub read_guids: Vec<u64>,
    pub write_guids: Vec<u64>,
    pub zone_consumer: F,
}

pub struct ZoneLockEnforcer<'a> {
    zones: &'a GuidTable<u64, ZoneInstance, u8>,
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
        let zone_lock_request = table_consumer(&zones_table_read_handle);

        let mut combined_guids = BTreeSet::from_iter(zone_lock_request.read_guids);
        combined_guids.extend(zone_lock_request.write_guids.iter());

        let write_set = BTreeSet::from_iter(zone_lock_request.write_guids);

        let mut zones_read_map = BTreeMap::new();
        let mut zones_write_map = BTreeMap::new();
        for guid in combined_guids {
            if write_set.contains(&guid) {
                if let Some(lock) = zones_table_read_handle.handle.get(guid) {
                    zones_write_map.insert(guid, lock.write());
                }
            } else if let Some(lock) = zones_table_read_handle.handle.get(guid) {
                zones_read_map.insert(guid, lock.read());
            }
        }

        (zone_lock_request.zone_consumer)(&zones_table_read_handle, zones_read_map, zones_write_map)
    }

    // This thread can access individual zones if and only if it holds the table read or write lock.
    // If this thread holds the table write lock, then no other threads may hold a table lock.
    // Therefore, if this thread holds the table write lock, it is the only thread that can hold any
    // zone locks, and we can provide full access to the table without fear of deadlock.
    pub fn write_zones<R, T: FnOnce(&mut ZoneTableWriteHandle) -> R>(
        &self,
        table_consumer: T,
    ) -> R {
        let mut zones_table_write_handle = self.zones.write();
        table_consumer(&mut zones_table_write_handle)
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
    pub read_guids: Vec<u64>,
    pub write_guids: Vec<u64>,
    pub character_consumer: F,
}

pub struct LockEnforcer<'a> {
    characters: &'a GuidTable<
        u64,
        Character,
        CharacterLocationIndex,
        CharacterNameIndex,
        CharacterSquadIndex,
        CharacterMatchmakingGroupIndex,
    >,
    zones: &'a GuidTable<u64, ZoneInstance, u8>,
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
        let character_lock_request: CharacterLockRequest<R, C> =
            table_consumer(&characters_table_read_handle);

        let mut combined_guids = BTreeSet::from_iter(character_lock_request.read_guids);
        combined_guids.extend(character_lock_request.write_guids.iter());

        let write_set = BTreeSet::from_iter(character_lock_request.write_guids);

        let mut characters_read_map = BTreeMap::new();
        let mut characters_write_map = BTreeMap::new();
        for guid in combined_guids {
            if write_set.contains(&guid) {
                if let Some(lock) = characters_table_read_handle.handle.get(guid) {
                    characters_write_map.insert(guid, lock.write());
                }
            } else if let Some(lock) = characters_table_read_handle.handle.get(guid) {
                characters_read_map.insert(guid, lock.read());
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
    pub fn write_characters<
        R,
        T: FnOnce(&mut CharacterTableWriteHandle, &ZoneLockEnforcer) -> R,
    >(
        &self,
        table_consumer: T,
    ) -> R {
        let mut characters_table_write_handle = self.characters.write();
        let zones_enforcer = ZoneLockEnforcer { zones: self.zones };
        table_consumer(&mut characters_table_write_handle, &zones_enforcer)
    }
}

impl<'a> From<LockEnforcer<'a>> for ZoneLockEnforcer<'a> {
    fn from(val: LockEnforcer<'a>) -> Self {
        ZoneLockEnforcer { zones: val.zones }
    }
}

pub struct LockEnforcerSource {
    characters: GuidTable<
        u64,
        Character,
        CharacterLocationIndex,
        CharacterNameIndex,
        CharacterSquadIndex,
        CharacterMatchmakingGroupIndex,
    >,
    zones: GuidTable<u64, ZoneInstance, u8>,
}

impl LockEnforcerSource {
    pub fn from(
        characters: GuidTable<
            u64,
            Character,
            CharacterLocationIndex,
            CharacterNameIndex,
            CharacterSquadIndex,
            CharacterMatchmakingGroupIndex,
        >,
        zones: GuidTable<u64, ZoneInstance, u8>,
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
