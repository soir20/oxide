use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::RangeBounds,
};

pub struct Lock<T> {
    inner: RwLock<T>,
}

impl<T> Lock<T> {
    pub fn new(lock: T) -> Self {
        Lock {
            inner: RwLock::new(lock),
        }
    }

    pub fn read(&self) -> RwLockReadGuard<T> {
        self.inner.read()
    }

    pub fn write(&self) -> RwLockWriteGuard<T> {
        self.inner.write()
    }
}

pub trait Guid<T> {
    fn guid(&self) -> T;
}

pub trait IndexedGuid<T, I1, I2 = (), I3 = ()> {
    fn guid(&self) -> T;

    fn index1(&self) -> I1;

    fn index2(&self) -> Option<I2> {
        None
    }

    fn index3(&self) -> Option<I3> {
        None
    }
}

impl<T, G: Guid<T>> IndexedGuid<T, ()> for G {
    fn guid(&self) -> T {
        self.guid()
    }

    fn index1(&self) {}
}

pub type GuidTableEntry<V, I1, I2, I3> = (Lock<V>, I1, Option<I2>, Option<I3>);

struct GuidTableData<K, V, I1, I2, I3> {
    data: BTreeMap<K, GuidTableEntry<V, I1, I2, I3>>,
    index1: BTreeMap<I1, BTreeSet<K>>,
    index2: BTreeMap<I2, BTreeSet<K>>,
    index3: BTreeMap<I3, BTreeSet<K>>,
}

impl<K, V, I1, I2, I3> GuidTableData<K, V, I1, I2, I3> {
    fn new() -> Self {
        GuidTableData {
            data: BTreeMap::new(),
            index1: BTreeMap::new(),
            index2: BTreeMap::new(),
            index3: BTreeMap::new(),
        }
    }
}

pub trait GuidTableIndexer<'a, K, V: 'a, I1, I2 = (), I3 = ()> {
    fn index1(&self, guid: K) -> Option<I1>;

    fn index2(&self, guid: K) -> Option<I2>;

    fn index3(&self, guid: K) -> Option<I3>;

    fn keys(&'a self) -> impl Iterator<Item = K>;

    fn keys_by_index1(&'a self, index: I1) -> impl Iterator<Item = K>;

    fn keys_by_index2(&'a self, index: I2) -> impl Iterator<Item = K>;

    fn keys_by_index3(&'a self, index: I3) -> impl Iterator<Item = K>;

    fn keys_by_index1_range(&'a self, range: impl RangeBounds<I1>) -> impl Iterator<Item = K>;

    fn keys_by_index2_range(&'a self, range: impl RangeBounds<I2>) -> impl Iterator<Item = K>;

    fn keys_by_index3_range(&'a self, range: impl RangeBounds<I3>) -> impl Iterator<Item = K>;
}

pub trait GuidTableHandle<'a, K, V: 'a, I1, I2, I3>:
    GuidTableIndexer<'a, K, V, I1, I2, I3>
{
    fn get(&self, guid: K) -> Option<&Lock<V>>;
}

pub struct GuidTableReadHandle<'a, K, V, I1 = (), I2 = (), I3 = ()> {
    guard: RwLockReadGuard<'a, GuidTableData<K, V, I1, I2, I3>>,
}

impl<'a, K: Copy + Ord, V, I1: Copy + Ord, I2: Copy + Ord, I3: Copy + Ord>
    GuidTableIndexer<'a, K, V, I1, I2, I3> for GuidTableReadHandle<'a, K, V, I1, I2, I3>
{
    fn index1(&self, guid: K) -> Option<I1> {
        self.guard.data.get(&guid).map(|(_, index1, _, _)| *index1)
    }

    fn index2(&self, guid: K) -> Option<I2> {
        self.guard
            .data
            .get(&guid)
            .and_then(|(_, _, index2, _)| *index2)
    }

    fn index3(&self, guid: K) -> Option<I3> {
        self.guard
            .data
            .get(&guid)
            .and_then(|(_, _, _, index3)| *index3)
    }

    fn keys(&'a self) -> impl Iterator<Item = K> {
        self.guard.data.keys().cloned()
    }

    fn keys_by_index1(&'a self, index: I1) -> impl Iterator<Item = K> {
        self.guard
            .index1
            .get(&index)
            .map(|index_list| index_list.iter())
            .unwrap_or_default()
            .cloned()
    }

    fn keys_by_index2(&'a self, index: I2) -> impl Iterator<Item = K> {
        self.guard
            .index2
            .get(&index)
            .map(|index_list| index_list.iter())
            .unwrap_or_default()
            .cloned()
    }

    fn keys_by_index3(&'a self, index: I3) -> impl Iterator<Item = K> {
        self.guard
            .index3
            .get(&index)
            .map(|index_list| index_list.iter())
            .unwrap_or_default()
            .cloned()
    }

    fn keys_by_index1_range(&'a self, range: impl RangeBounds<I1>) -> impl Iterator<Item = K> {
        self.guard
            .index1
            .range(range)
            .flat_map(|(_, keys)| keys.iter().copied())
    }

    fn keys_by_index2_range(&'a self, range: impl RangeBounds<I2>) -> impl Iterator<Item = K> {
        self.guard
            .index2
            .range(range)
            .flat_map(|(_, keys)| keys.iter().copied())
    }

    fn keys_by_index3_range(&'a self, range: impl RangeBounds<I3>) -> impl Iterator<Item = K> {
        self.guard
            .index3
            .range(range)
            .flat_map(|(_, keys)| keys.iter().copied())
    }
}

impl<'a, K: Copy + Ord, V, I1: Copy + Ord, I2: Copy + Ord, I3: Copy + Ord>
    GuidTableHandle<'a, K, V, I1, I2, I3> for GuidTableReadHandle<'a, K, V, I1, I2, I3>
{
    fn get(&self, guid: K) -> Option<&Lock<V>> {
        self.guard.data.get(&guid).map(|(item, _, _, _)| item)
    }
}

pub struct GuidTableWriteHandle<'a, K, V, I1 = (), I2 = (), I3 = ()> {
    guard: RwLockWriteGuard<'a, GuidTableData<K, V, I1, I2, I3>>,
}

impl<
        K: Copy + Ord,
        V: IndexedGuid<K, I1, I2, I3>,
        I1: Copy + Ord,
        I2: Copy + Ord,
        I3: Copy + Ord,
    > GuidTableWriteHandle<'_, K, V, I1, I2, I3>
{
    pub fn get(&self, guid: K) -> Option<&Lock<V>> {
        self.guard.data.get(&guid).map(|(lock, _, _, _)| lock)
    }

    pub fn values_by_index(&self, index: I1) -> impl Iterator<Item = &Lock<V>> {
        self.keys_by_index1(index)
            .filter_map(|guid| self.guard.data.get(&guid).map(|(lock, _, _, _)| lock))
    }

    pub fn insert(&mut self, item: V) -> Option<Lock<V>> {
        let key = item.guid();
        let index1 = item.index1();
        let index2 = item.index2();
        let index3 = item.index3();

        self.insert_with_index(key, index1, index2, index3, Lock::new(item))
    }

    pub fn insert_lock(
        &mut self,
        guid: K,
        index1: I1,
        index2: Option<I2>,
        index3: Option<I3>,
        lock: Lock<V>,
    ) -> Option<Lock<V>> {
        self.insert_with_index(guid, index1, index2, index3, lock)
    }

    pub fn remove(&mut self, guid: K) -> Option<GuidTableEntry<V, I1, I2, I3>> {
        let previous = self.guard.data.remove(&guid);
        if let Some((_, previous_index1, previous_index2, previous_index3)) = &previous {
            self.guard
                .index1
                .get_mut(previous_index1)
                .expect("GUID table key was never added to index1")
                .remove(&guid);

            if let Some(index2) = previous_index2 {
                self.guard
                    .index2
                    .get_mut(index2)
                    .expect("GUID table key was never added to index2")
                    .remove(&guid);
            }

            if let Some(index3) = previous_index3 {
                self.guard
                    .index3
                    .get_mut(index3)
                    .expect("GUID table key was never added to index3")
                    .remove(&guid);
            }
        }

        previous
    }

    fn insert_with_index(
        &mut self,
        key: K,
        index1: I1,
        index2: Option<I2>,
        index3: Option<I3>,
        item: Lock<V>,
    ) -> Option<Lock<V>> {
        // Remove from the index before inserting the new key in case the item has the same key
        let previous = self.remove(key);

        self.guard.data.insert(key, (item, index1, index2, index3));
        self.guard.index1.entry(index1).or_default().insert(key);
        if let Some(value) = index2 {
            self.guard.index2.entry(value).or_default().insert(key);
        }
        if let Some(value) = index3 {
            self.guard.index3.entry(value).or_default().insert(key);
        }

        previous.map(|(item, _, _, _)| item)
    }
}

impl<'a, K: Copy + Ord, V, I1: Copy + Ord, I2: Copy + Ord, I3: Copy + Ord>
    GuidTableIndexer<'a, K, V, I1, I2, I3> for GuidTableWriteHandle<'a, K, V, I1, I2, I3>
{
    fn index1(&self, guid: K) -> Option<I1> {
        self.guard.data.get(&guid).map(|(_, index, _, _)| *index)
    }

    fn index2(&self, guid: K) -> Option<I2> {
        self.guard
            .data
            .get(&guid)
            .and_then(|(_, _, index, _)| *index)
    }

    fn index3(&self, guid: K) -> Option<I3> {
        self.guard
            .data
            .get(&guid)
            .and_then(|(_, _, _, index)| *index)
    }

    fn keys(&'a self) -> impl Iterator<Item = K> {
        self.guard.data.keys().cloned()
    }

    fn keys_by_index1(&'a self, index: I1) -> impl Iterator<Item = K> {
        self.guard
            .index1
            .get(&index)
            .map(|index_list| index_list.iter())
            .unwrap_or_default()
            .cloned()
    }

    fn keys_by_index2(&'a self, index: I2) -> impl Iterator<Item = K> {
        self.guard
            .index2
            .get(&index)
            .map(|index_list| index_list.iter())
            .unwrap_or_default()
            .cloned()
    }

    fn keys_by_index3(&'a self, index: I3) -> impl Iterator<Item = K> {
        self.guard
            .index3
            .get(&index)
            .map(|index_list| index_list.iter())
            .unwrap_or_default()
            .cloned()
    }

    fn keys_by_index1_range(&'a self, range: impl RangeBounds<I1>) -> impl Iterator<Item = K> {
        self.guard
            .index1
            .range(range)
            .flat_map(|(_, keys)| keys.iter().copied())
    }

    fn keys_by_index2_range(&'a self, range: impl RangeBounds<I2>) -> impl Iterator<Item = K> {
        self.guard
            .index2
            .range(range)
            .flat_map(|(_, keys)| keys.iter().copied())
    }

    fn keys_by_index3_range(&'a self, range: impl RangeBounds<I3>) -> impl Iterator<Item = K> {
        self.guard
            .index3
            .range(range)
            .flat_map(|(_, keys)| keys.iter().copied())
    }
}

impl<
        'a,
        K: Copy + Ord,
        I1: Copy + Ord,
        I2: Copy + Ord,
        I3: Copy + Ord,
        V: IndexedGuid<K, I1, I2, I3>,
    > GuidTableHandle<'a, K, V, I1, I2, I3> for GuidTableWriteHandle<'a, K, V, I1, I2, I3>
{
    fn get(&self, guid: K) -> Option<&Lock<V>> {
        self.guard.data.get(&guid).map(|(item, _, _, _)| item)
    }
}

pub struct GuidTable<K, V, I1 = (), I2 = (), I3 = ()> {
    data: Lock<GuidTableData<K, V, I1, I2, I3>>,
}

impl<K, I1, I2, I3, V: IndexedGuid<K, I1, I2, I3>> GuidTable<K, V, I1, I2, I3> {
    pub fn new() -> Self {
        GuidTable {
            data: Lock::new(GuidTableData::new()),
        }
    }

    pub fn read(&self) -> GuidTableReadHandle<K, V, I1, I2, I3> {
        GuidTableReadHandle {
            guard: self.data.read(),
        }
    }

    pub fn write(&self) -> GuidTableWriteHandle<K, V, I1, I2, I3> {
        GuidTableWriteHandle {
            guard: self.data.write(),
        }
    }
}
