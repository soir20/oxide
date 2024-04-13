use std::collections::BTreeMap;
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};

pub struct Lock<T> {
    inner: RwLock<T>
}

impl<T> Lock<T> {
    pub fn new(lock: T) -> Self {
        Lock {
            inner: RwLock::new(lock)
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

type GuidTableData<K, V> = BTreeMap<K, Lock<V>>;

pub struct GuidTableReadHandle<'a, K, V> {
    guard: RwLockReadGuard<'a, GuidTableData<K, V>>
}

impl<'a, K: Copy + Ord, V> GuidTableReadHandle<'a, K, V> {
    pub fn get(&self, guid: K) -> Option<&Lock<V>> {
        self.guard.get(&guid)
    }

    pub fn iter(&self) -> impl Iterator<Item=(K, &Lock<V>)> {
        self.guard.iter().map(|(guid, item)| (*guid, item))
    }

    pub fn values(&self) -> impl Iterator<Item=&Lock<V>> {
        self.guard.values()
    }
}

pub struct GuidTableWriteHandle<'a, K, V> {
    guard: RwLockWriteGuard<'a, GuidTableData<K, V>>
}

impl<'a, K: Ord, V: Guid<K>> GuidTableWriteHandle<'a, K, V> {
    pub fn insert(&mut self, item: V) -> Option<Lock<V>> {
        self.guard.insert(item.guid(), Lock::new(item))
    }

    pub fn insert_lock(&mut self, guid: K, lock: Lock<V>) -> Option<Lock<V>> {
        self.guard.insert(guid, lock)
    }

    pub fn remove(&mut self, guid: K) -> Option<Lock<V>> {
        self.guard.remove(&guid)
    }
}

pub struct GuidTable<K, V> {
    data: Lock<GuidTableData<K, V>>
}

impl<K, V: Guid<K>> GuidTable<K, V> {
    pub fn new() -> Self {
        GuidTable {
            data: Lock::new(BTreeMap::new()),
        }
    }

    pub fn read(&self) -> GuidTableReadHandle<K, V> {
        GuidTableReadHandle {
            guard: self.data.read()
        }
    }

    pub fn write(&self) -> GuidTableWriteHandle<K, V> {
        GuidTableWriteHandle {
            guard: self.data.write()
        }
    }
}
