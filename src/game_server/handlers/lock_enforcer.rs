use std::{
    collections::{BTreeMap, BTreeSet},
    ops::RangeBounds,
};

use parking_lot::{RwLockReadGuard, RwLockWriteGuard};

use super::{
    character::{
        Character, CharacterLocationIndex, CharacterMatchmakingGroupIndex, CharacterNameIndex,
        CharacterSquadIndex, CharacterSynchronizationIndex, MinigameMatchmakingGroup,
    },
    guid::{
        GuidTable, GuidTableHandle, GuidTableIndexer, GuidTableReadHandle, GuidTableWriteHandle,
        IndexedGuid,
    },
    minigame::{
        SharedMinigameData, SharedMinigameDataMatchmakingIndex, SharedMinigameDataTickableIndex,
    },
    zone::ZoneInstance,
};

pub struct TableReadHandleWrapper<'a, K, V, I1 = (), I2 = (), I3 = (), I4 = (), I5 = ()> {
    handle: GuidTableReadHandle<'a, K, V, I1, I2, I3, I4, I5>,
}

impl<
        'a,
        K: Copy + Ord,
        V,
        I1: Copy + Ord,
        I2: Clone + Ord,
        I3: Clone + Ord,
        I4: Clone + Ord,
        I5: Clone + Ord,
    > GuidTableIndexer<'a, K, V, I1, I2, I3, I4, I5>
    for TableReadHandleWrapper<'a, K, V, I1, I2, I3, I4, I5>
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

    fn index5(&self, guid: K) -> Option<&I5> {
        self.handle.index5(guid)
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

    fn keys_by_index5(&'a self, index: &I5) -> impl Iterator<Item = K> {
        self.handle.keys_by_index5(index)
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

    fn keys_by_index5_range(
        &'a self,
        range: impl RangeBounds<I5>,
    ) -> impl DoubleEndedIterator<Item = K> {
        self.handle.keys_by_index5_range(range)
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

    fn indices5(&'a self) -> impl Iterator<Item = &'a I5> {
        self.handle.indices5()
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

    fn indices5_by_range(
        &'a self,
        range: impl RangeBounds<I5>,
    ) -> impl DoubleEndedIterator<Item = &'a I5> {
        self.handle.indices5_by_range(range)
    }
}

impl<'a, K, V, I1, I2, I3, I4, I5> From<GuidTableReadHandle<'a, K, V, I1, I2, I3, I4, I5>>
    for TableReadHandleWrapper<'a, K, V, I1, I2, I3, I4, I5>
{
    fn from(value: GuidTableReadHandle<'a, K, V, I1, I2, I3, I4, I5>) -> Self {
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
    CharacterSynchronizationIndex,
>;
pub type CharacterTableWriteHandle<'a> = GuidTableWriteHandle<
    'a,
    u64,
    Character,
    CharacterLocationIndex,
    CharacterNameIndex,
    CharacterSquadIndex,
    CharacterMatchmakingGroupIndex,
    CharacterSynchronizationIndex,
>;
pub type CharacterReadGuard<'a> = RwLockReadGuard<'a, Character>;
pub type CharacterWriteGuard<'a> = RwLockWriteGuard<'a, Character>;
pub type ZoneTableReadHandle<'a> = TableReadHandleWrapper<'a, u64, ZoneInstance, u8>;
pub type ZoneTableWriteHandle<'a> = GuidTableWriteHandle<'a, u64, ZoneInstance, u8>;
pub type ZoneReadGuard<'a> = RwLockReadGuard<'a, ZoneInstance>;
pub type ZoneWriteGuard<'a> = RwLockWriteGuard<'a, ZoneInstance>;
pub type MinigameDataTableReadHandle<'a> = TableReadHandleWrapper<
    'a,
    MinigameMatchmakingGroup,
    SharedMinigameData,
    SharedMinigameDataTickableIndex,
    SharedMinigameDataMatchmakingIndex,
>;
pub type MinigameDataTableWriteHandle<'a> = GuidTableWriteHandle<
    'a,
    MinigameMatchmakingGroup,
    SharedMinigameData,
    SharedMinigameDataTickableIndex,
    SharedMinigameDataMatchmakingIndex,
>;
pub type MinigameDataReadGuard<'a> = RwLockReadGuard<'a, SharedMinigameData>;
pub type MinigameDataWriteGuard<'a> = RwLockWriteGuard<'a, SharedMinigameData>;

struct LockRequest<K, F> {
    pub read_guids: Vec<K>,
    pub write_guids: Vec<K>,
    pub consumer: F,
}

struct LockEnforcer<'a, K, V, I1 = (), I2 = (), I3 = (), I4 = (), I5 = ()> {
    table: &'a GuidTable<K, V, I1, I2, I3, I4, I5>,
}

impl<
        K: Copy + Ord,
        I1: Copy + Ord,
        I2: Clone + Ord,
        I3: Clone + Ord,
        I4: Clone + Ord,
        I5: Clone + Ord,
        V: IndexedGuid<K, I1, I2, I3, I4, I5>,
    > LockEnforcer<'_, K, V, I1, I2, I3, I4, I5>
{
    pub fn read<
        R,
        F: FnOnce(
            &TableReadHandleWrapper<'_, K, V, I1, I2, I3, I4, I5>,
            BTreeMap<K, RwLockReadGuard<'_, V>>,
            BTreeMap<K, RwLockWriteGuard<'_, V>>,
        ) -> R,
        T: FnOnce(&TableReadHandleWrapper<'_, K, V, I1, I2, I3, I4, I5>) -> LockRequest<K, F>,
    >(
        &self,
        table_consumer: T,
    ) -> R {
        let table_read_handle = self.table.read().into();
        let lock_request = table_consumer(&table_read_handle);

        let mut combined_guids = BTreeSet::from_iter(lock_request.read_guids);
        combined_guids.extend(lock_request.write_guids.iter());

        let write_set = BTreeSet::from_iter(lock_request.write_guids);

        let mut read_map = BTreeMap::new();
        let mut write_map = BTreeMap::new();
        for guid in combined_guids {
            if write_set.contains(&guid) {
                if let Some(lock) = table_read_handle.handle.get(guid) {
                    write_map.insert(guid, lock.write());
                }
            } else if let Some(lock) = table_read_handle.handle.get(guid) {
                read_map.insert(guid, lock.read());
            }
        }

        (lock_request.consumer)(&table_read_handle, read_map, write_map)
    }

    // This thread can access individual items if and only if it holds the table read or write lock.
    // If this thread holds the table write lock, then no other threads may hold a table lock.
    // Therefore, if this thread holds the table write lock, it is the only thread that can hold any
    // item locks, and we can provide full access to the table without fear of deadlock.
    pub fn write<R, T: FnOnce(&mut GuidTableWriteHandle<'_, K, V, I1, I2, I3, I4, I5>) -> R>(
        &self,
        table_consumer: T,
    ) -> R {
        let mut table_write_handle = self.table.write();
        table_consumer(&mut table_write_handle)
    }
}

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

impl<
        R,
        F: FnOnce(
            &ZoneTableReadHandle<'_>,
            BTreeMap<u64, ZoneReadGuard<'_>>,
            BTreeMap<u64, ZoneWriteGuard<'_>>,
        ) -> R,
    > From<ZoneLockRequest<R, F>> for LockRequest<u64, F>
{
    fn from(value: ZoneLockRequest<R, F>) -> Self {
        LockRequest {
            read_guids: value.read_guids,
            write_guids: value.write_guids,
            consumer: value.zone_consumer,
        }
    }
}

pub struct ZoneLockEnforcer<'a> {
    enforcer: LockEnforcer<'a, u64, ZoneInstance, u8>,
}

impl ZoneLockEnforcer<'_> {
    pub fn read_zones<
        R,
        F: FnOnce(
            &ZoneTableReadHandle<'_>,
            BTreeMap<u64, ZoneReadGuard<'_>>,
            BTreeMap<u64, ZoneWriteGuard<'_>>,
        ) -> R,
        T: FnOnce(&ZoneTableReadHandle<'_>) -> ZoneLockRequest<R, F>,
    >(
        &self,
        table_consumer: T,
    ) -> R {
        self.enforcer
            .read(|table_read_handle| table_consumer(table_read_handle).into())
    }

    pub fn write_zones<R, T: FnOnce(&mut ZoneTableWriteHandle) -> R>(
        &self,
        table_consumer: T,
    ) -> R {
        self.enforcer.write(table_consumer)
    }
}

impl<'a> From<MinigameDataLockEnforcer<'a>> for ZoneLockEnforcer<'a> {
    fn from(val: MinigameDataLockEnforcer<'a>) -> Self {
        ZoneLockEnforcer {
            enforcer: LockEnforcer { table: val.zones },
        }
    }
}

pub struct MinigameDataLockRequest<
    R,
    F: FnOnce(
        &MinigameDataTableReadHandle<'_>,
        BTreeMap<MinigameMatchmakingGroup, MinigameDataReadGuard<'_>>,
        BTreeMap<MinigameMatchmakingGroup, MinigameDataWriteGuard<'_>>,
        ZoneLockEnforcer<'_>,
    ) -> R,
> {
    pub read_guids: Vec<MinigameMatchmakingGroup>,
    pub write_guids: Vec<MinigameMatchmakingGroup>,
    pub minigame_data_consumer: F,
}

impl<
        R,
        F: FnOnce(
            &MinigameDataTableReadHandle<'_>,
            BTreeMap<MinigameMatchmakingGroup, MinigameDataReadGuard<'_>>,
            BTreeMap<MinigameMatchmakingGroup, MinigameDataWriteGuard<'_>>,
            ZoneLockEnforcer<'_>,
        ) -> R,
    > From<MinigameDataLockRequest<R, F>> for LockRequest<MinigameMatchmakingGroup, F>
{
    fn from(value: MinigameDataLockRequest<R, F>) -> Self {
        LockRequest {
            read_guids: value.read_guids,
            write_guids: value.write_guids,
            consumer: value.minigame_data_consumer,
        }
    }
}

pub struct MinigameDataLockEnforcer<'a> {
    enforcer: LockEnforcer<
        'a,
        MinigameMatchmakingGroup,
        SharedMinigameData,
        SharedMinigameDataTickableIndex,
        SharedMinigameDataMatchmakingIndex,
    >,
    zones: &'a GuidTable<u64, ZoneInstance, u8>,
}

impl MinigameDataLockEnforcer<'_> {
    pub fn read_minigame_data<
        R,
        F: FnOnce(
            &MinigameDataTableReadHandle<'_>,
            BTreeMap<MinigameMatchmakingGroup, MinigameDataReadGuard<'_>>,
            BTreeMap<MinigameMatchmakingGroup, MinigameDataWriteGuard<'_>>,
            ZoneLockEnforcer<'_>,
        ) -> R,
        T: FnOnce(&MinigameDataTableReadHandle<'_>) -> MinigameDataLockRequest<R, F>,
    >(
        &self,
        table_consumer: T,
    ) -> R {
        self.enforcer
            .read(|table_read_handle: &MinigameDataTableReadHandle<'_>| {
                let minigame_data_lock_request = table_consumer(table_read_handle);
                LockRequest {
                    read_guids: minigame_data_lock_request.read_guids,
                    write_guids: minigame_data_lock_request.write_guids,
                    consumer: |table_read_handle: &MinigameDataTableReadHandle<'_>,
                               minigame_data_read: BTreeMap<
                        MinigameMatchmakingGroup,
                        MinigameDataReadGuard<'_>,
                    >,
                               minigame_data_write: BTreeMap<
                        MinigameMatchmakingGroup,
                        MinigameDataWriteGuard<'_>,
                    >| {
                        let zones_enforcer = ZoneLockEnforcer {
                            enforcer: LockEnforcer { table: self.zones },
                        };
                        (minigame_data_lock_request.minigame_data_consumer)(
                            table_read_handle,
                            minigame_data_read,
                            minigame_data_write,
                            zones_enforcer,
                        )
                    },
                }
            })
    }

    pub fn write_minigame_data<
        R,
        T: FnOnce(&mut MinigameDataTableWriteHandle, ZoneLockEnforcer) -> R,
    >(
        &self,
        table_consumer: T,
    ) -> R {
        self.enforcer.write(|table_write_handle| {
            let zones_enforcer = ZoneLockEnforcer {
                enforcer: LockEnforcer { table: self.zones },
            };
            table_consumer(table_write_handle, zones_enforcer)
        })
    }
}

pub struct CharacterLockRequest<
    R,
    F: FnOnce(
        &CharacterTableReadHandle<'_>,
        BTreeMap<u64, CharacterReadGuard<'_>>,
        BTreeMap<u64, CharacterWriteGuard<'_>>,
        MinigameDataLockEnforcer,
    ) -> R,
> {
    pub read_guids: Vec<u64>,
    pub write_guids: Vec<u64>,
    pub character_consumer: F,
}

pub struct CharacterLockEnforcer<'a> {
    enforcer: LockEnforcer<
        'a,
        u64,
        Character,
        CharacterLocationIndex,
        CharacterNameIndex,
        CharacterSquadIndex,
        CharacterMatchmakingGroupIndex,
        CharacterSynchronizationIndex,
    >,
    zones: &'a GuidTable<u64, ZoneInstance, u8>,
    minigame_data: &'a GuidTable<
        MinigameMatchmakingGroup,
        SharedMinigameData,
        SharedMinigameDataTickableIndex,
        SharedMinigameDataMatchmakingIndex,
    >,
}

impl CharacterLockEnforcer<'_> {
    pub fn read_characters<
        R,
        C: FnOnce(
            &CharacterTableReadHandle<'_>,
            BTreeMap<u64, CharacterReadGuard<'_>>,
            BTreeMap<u64, CharacterWriteGuard<'_>>,
            MinigameDataLockEnforcer,
        ) -> R,
        T: FnOnce(&CharacterTableReadHandle<'_>) -> CharacterLockRequest<R, C>,
    >(
        &self,
        table_consumer: T,
    ) -> R {
        self.enforcer.read(|table_read_handle: &CharacterTableReadHandle<'_>| {
            let character_lock_request = table_consumer(table_read_handle);
            LockRequest {
                read_guids: character_lock_request.read_guids,
                write_guids: character_lock_request.write_guids,
                consumer: |table_read_handle: &CharacterTableReadHandle<'_>, characters_read: BTreeMap<u64, CharacterReadGuard<'_>>, characters_write: BTreeMap<u64, CharacterWriteGuard<'_>>| {
                    let minigame_data_enforcer = MinigameDataLockEnforcer {
                        enforcer: LockEnforcer { table: self.minigame_data },
                        zones: self.zones,
                    };
                    (character_lock_request.character_consumer)(
                        table_read_handle,
                        characters_read,
                        characters_write,
                        minigame_data_enforcer,
                    )
                },
            }
        })
    }

    pub fn write_characters<
        R,
        T: FnOnce(&mut CharacterTableWriteHandle, MinigameDataLockEnforcer) -> R,
    >(
        &self,
        table_consumer: T,
    ) -> R {
        self.enforcer.write(|table_write_handle| {
            let minigame_data_enforcer = MinigameDataLockEnforcer {
                enforcer: LockEnforcer {
                    table: self.minigame_data,
                },
                zones: self.zones,
            };
            table_consumer(table_write_handle, minigame_data_enforcer)
        })
    }
}

impl<'a> From<CharacterLockEnforcer<'a>> for MinigameDataLockEnforcer<'a> {
    fn from(val: CharacterLockEnforcer<'a>) -> Self {
        MinigameDataLockEnforcer {
            enforcer: LockEnforcer {
                table: val.minigame_data,
            },
            zones: val.zones,
        }
    }
}

impl<'a> From<CharacterLockEnforcer<'a>> for ZoneLockEnforcer<'a> {
    fn from(val: CharacterLockEnforcer<'a>) -> Self {
        ZoneLockEnforcer {
            enforcer: LockEnforcer { table: val.zones },
        }
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
        CharacterSynchronizationIndex,
    >,
    zones: GuidTable<u64, ZoneInstance, u8>,
    minigame_data: GuidTable<
        MinigameMatchmakingGroup,
        SharedMinigameData,
        SharedMinigameDataTickableIndex,
        SharedMinigameDataMatchmakingIndex,
    >,
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
            CharacterSynchronizationIndex,
        >,
        zones: GuidTable<u64, ZoneInstance, u8>,
        minigame_data: GuidTable<
            MinigameMatchmakingGroup,
            SharedMinigameData,
            SharedMinigameDataTickableIndex,
            SharedMinigameDataMatchmakingIndex,
        >,
    ) -> LockEnforcerSource {
        LockEnforcerSource {
            characters,
            zones,
            minigame_data,
        }
    }

    pub fn lock_enforcer(&self) -> CharacterLockEnforcer {
        CharacterLockEnforcer {
            enforcer: LockEnforcer {
                table: &self.characters,
            },
            zones: &self.zones,
            minigame_data: &self.minigame_data,
        }
    }
}
