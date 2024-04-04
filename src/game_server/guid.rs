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

pub trait Guid {
    fn guid(&self) -> u64;
}

type GuidTableData<T> = BTreeMap<u64, Lock<T>>;

pub struct GuidTableReadHandle<'a, T> {
    guard: RwLockReadGuard<'a, GuidTableData<T>>
}

impl<'a, T> GuidTableReadHandle<'a, T> {
    pub fn get(&self, guid: u64) -> Option<&Lock<T>> {
        self.guard.get(&guid)
    }

    pub fn iter(&self) -> impl Iterator<Item=(u64, &Lock<T>)> {
        self.guard.iter().map(|(guid, item)| (*guid, item))
    }

    pub fn values(&self) -> impl Iterator<Item=&Lock<T>> {
        self.guard.values()
    }
}

pub struct GuidTableWriteHandle<'a, T> {
    guard: RwLockWriteGuard<'a, GuidTableData<T>>
}

impl<'a, T: Guid> GuidTableWriteHandle<'a, T> {
    pub fn insert(&mut self, item: T) -> Option<Lock<T>> {
        self.guard.insert(item.guid(), Lock::new(item))
    }

    pub fn remove(&mut self, guid: u64) -> Option<Lock<T>> {
        self.guard.remove(&guid)
    }
}

pub struct GuidTable<T> {
    data: Lock<GuidTableData<T>>
}

impl<T: Guid> GuidTable<T> {
    pub fn new() -> Self {
        GuidTable {
            data: Lock::new(BTreeMap::new()),
        }
    }

    pub fn read(&self) -> GuidTableReadHandle<T> {
        GuidTableReadHandle {
            guard: self.data.read()
        }
    }

    pub fn write(&self) -> GuidTableWriteHandle<T> {
        GuidTableWriteHandle {
            guard: self.data.write()
        }
    }
}
