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

pub trait IndexedGuid<T, I> {
    fn guid(&self) -> T;

    fn index(&self) -> I;
}

impl<T, G: Guid<T>> IndexedGuid<T, ()> for G {
    fn guid(&self) -> T {
        self.guid()
    }

    fn index(&self) {}
}

struct GuidTableData<K, V, I> {
    data: BTreeMap<K, (Lock<V>, I)>,
    index: BTreeMap<I, BTreeSet<K>>,
}

impl<K, V, I> GuidTableData<K, V, I> {
    fn new() -> Self {
        GuidTableData {
            data: BTreeMap::new(),
            index: BTreeMap::new(),
        }
    }
}

pub trait GuidTableIndexer<'a, K, V: 'a, I> {
    fn index(&self, guid: K) -> Option<I>;

    fn keys(&'a self) -> impl Iterator<Item = K>;

    fn keys_by_index(&'a self, index: I) -> impl Iterator<Item = K>;

    fn keys_by_range(&'a self, range: impl RangeBounds<I>) -> impl Iterator<Item = K>;
}

pub trait GuidTableHandle<'a, K, V: 'a, I>: GuidTableIndexer<'a, K, V, I> {
    fn get(&self, guid: K) -> Option<&Lock<V>>;
}

pub struct GuidTableReadHandle<'a, K, V, I = ()> {
    guard: RwLockReadGuard<'a, GuidTableData<K, V, I>>,
}

impl<'a, K: Copy + Ord, V, I: Copy + Ord> GuidTableIndexer<'a, K, V, I>
    for GuidTableReadHandle<'a, K, V, I>
{
    fn index(&self, guid: K) -> Option<I> {
        self.guard.data.get(&guid).map(|(_, index)| *index)
    }

    fn keys(&'a self) -> impl Iterator<Item = K> {
        self.guard.data.keys().cloned()
    }

    fn keys_by_index(&'a self, index: I) -> impl Iterator<Item = K> {
        self.guard
            .index
            .get(&index)
            .map(|index_list| index_list.iter())
            .unwrap_or_default()
            .cloned()
    }

    fn keys_by_range(&'a self, range: impl RangeBounds<I>) -> impl Iterator<Item = K> {
        self.guard
            .index
            .range(range)
            .flat_map(|(_, keys)| keys.iter().copied())
    }
}

impl<'a, K: Copy + Ord, V, I: Copy + Ord> GuidTableHandle<'a, K, V, I>
    for GuidTableReadHandle<'a, K, V, I>
{
    fn get(&self, guid: K) -> Option<&Lock<V>> {
        self.guard.data.get(&guid).map(|(item, _)| item)
    }
}

pub struct GuidTableWriteHandle<'a, K, V, I = ()> {
    guard: RwLockWriteGuard<'a, GuidTableData<K, V, I>>,
}

impl<K: Copy + Ord, V: IndexedGuid<K, I>, I: Copy + Ord> GuidTableWriteHandle<'_, K, V, I> {
    pub fn get(&self, guid: K) -> Option<&Lock<V>> {
        self.guard.data.get(&guid).map(|(lock, _)| lock)
    }

    pub fn values_by_index(&self, index: I) -> impl Iterator<Item = &Lock<V>> {
        self.keys_by_index(index)
            .filter_map(|guid| self.guard.data.get(&guid).map(|(lock, _)| lock))
    }

    pub fn insert(&mut self, item: V) -> Option<Lock<V>> {
        let key = item.guid();
        let index = item.index();

        self.insert_with_index(key, index, Lock::new(item))
    }

    pub fn insert_lock(&mut self, guid: K, index: I, lock: Lock<V>) -> Option<Lock<V>> {
        self.insert_with_index(guid, index, lock)
    }

    pub fn remove(&mut self, guid: K) -> Option<(Lock<V>, I)> {
        let previous = self.guard.data.remove(&guid);
        if let Some((_, previous_index)) = &previous {
            self.guard
                .index
                .get_mut(previous_index)
                .expect("GUID table key was never added to index")
                .remove(&guid);
        }

        previous
    }

    fn insert_with_index(&mut self, key: K, index: I, item: Lock<V>) -> Option<Lock<V>> {
        // Remove from the index before inserting the new key in case the item has the same key
        let previous = self.guard.data.insert(key, (item, index));
        if let Some((_, previous_index)) = &previous {
            self.guard
                .index
                .get_mut(previous_index)
                .expect("GUID table key was never added to index")
                .remove(&key);
        }

        self.guard.index.entry(index).or_default().insert(key);

        previous.map(|(item, _)| item)
    }
}

impl<'a, K: Copy + Ord, V, I: Copy + Ord> GuidTableIndexer<'a, K, V, I>
    for GuidTableWriteHandle<'a, K, V, I>
{
    fn index(&self, guid: K) -> Option<I> {
        self.guard.data.get(&guid).map(|(_, index)| *index)
    }

    fn keys(&'a self) -> impl Iterator<Item = K> {
        self.guard.data.keys().cloned()
    }

    fn keys_by_index(&'a self, index: I) -> impl Iterator<Item = K> {
        self.guard
            .index
            .get(&index)
            .map(|index_list| index_list.iter())
            .unwrap_or_default()
            .cloned()
    }

    fn keys_by_range(&'a self, range: impl RangeBounds<I>) -> impl Iterator<Item = K> {
        self.guard
            .index
            .range(range)
            .flat_map(|(_, keys)| keys.iter().copied())
    }
}

impl<'a, K: Copy + Ord, I: Copy + Ord, V: IndexedGuid<K, I>> GuidTableHandle<'a, K, V, I>
    for GuidTableWriteHandle<'a, K, V, I>
{
    fn get(&self, guid: K) -> Option<&Lock<V>> {
        self.guard.data.get(&guid).map(|(item, _)| item)
    }
}

pub struct GuidTable<K, V, I = ()> {
    data: Lock<GuidTableData<K, V, I>>,
}

impl<K, I, V: IndexedGuid<K, I>> GuidTable<K, V, I> {
    pub fn new() -> Self {
        GuidTable {
            data: Lock::new(GuidTableData::new()),
        }
    }

    pub fn read(&self) -> GuidTableReadHandle<K, V, I> {
        GuidTableReadHandle {
            guard: self.data.read(),
        }
    }

    pub fn write(&self) -> GuidTableWriteHandle<K, V, I> {
        GuidTableWriteHandle {
            guard: self.data.write(),
        }
    }
}
