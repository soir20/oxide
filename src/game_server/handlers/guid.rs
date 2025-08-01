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

pub trait IndexedGuid<T, I1, I2 = (), I3 = (), I4 = (), I5 = ()> {
    fn guid(&self) -> T;

    fn index1(&self) -> I1;

    fn index2(&self) -> Option<I2> {
        None
    }

    fn index3(&self) -> Option<I3> {
        None
    }

    fn index4(&self) -> Option<I4> {
        None
    }

    fn index5(&self) -> Option<I5> {
        None
    }
}

impl<T, G: Guid<T>> IndexedGuid<T, ()> for G {
    fn guid(&self) -> T {
        self.guid()
    }

    fn index1(&self) {}
}

pub type GuidTableEntry<V, I1, I2, I3, I4, I5> =
    (Lock<V>, I1, Option<I2>, Option<I3>, Option<I4>, Option<I5>);

struct GuidTableData<K, V, I1, I2, I3, I4, I5> {
    data: BTreeMap<K, GuidTableEntry<V, I1, I2, I3, I4, I5>>,
    index1: BTreeMap<I1, BTreeSet<K>>,
    index2: BTreeMap<I2, BTreeSet<K>>,
    index3: BTreeMap<I3, BTreeSet<K>>,
    index4: BTreeMap<I4, BTreeSet<K>>,
    index5: BTreeMap<I5, BTreeSet<K>>,
}

impl<K, V, I1, I2, I3, I4, I5> GuidTableData<K, V, I1, I2, I3, I4, I5> {
    fn new() -> Self {
        GuidTableData {
            data: BTreeMap::new(),
            index1: BTreeMap::new(),
            index2: BTreeMap::new(),
            index3: BTreeMap::new(),
            index4: BTreeMap::new(),
            index5: BTreeMap::new(),
        }
    }
}

pub trait GuidTableIndexer<'a, K, V: 'a, I1, I2: 'a = (), I3: 'a = (), I4: 'a = (), I5: 'a = ()> {
    fn index1(&self, guid: K) -> Option<I1>;

    fn index2(&self, guid: K) -> Option<&I2>;

    fn index3(&self, guid: K) -> Option<&I3>;

    fn index4(&self, guid: K) -> Option<&I4>;

    fn index5(&self, guid: K) -> Option<&I5>;

    fn keys(&'a self) -> impl Iterator<Item = K>;

    fn keys_by_index1(&'a self, index: I1) -> impl Iterator<Item = K>;

    fn keys_by_index2<'b>(&'a self, index: &'b I2) -> impl Iterator<Item = K>;

    fn keys_by_index3<'b>(&'a self, index: &'b I3) -> impl Iterator<Item = K>;

    fn keys_by_index4<'b>(&'a self, index: &'b I4) -> impl Iterator<Item = K>;

    fn keys_by_index5<'b>(&'a self, index: &'b I5) -> impl Iterator<Item = K>;

    fn keys_by_index1_range(
        &'a self,
        range: impl RangeBounds<I1>,
    ) -> impl DoubleEndedIterator<Item = K>;

    fn keys_by_index2_range(
        &'a self,
        range: impl RangeBounds<I2>,
    ) -> impl DoubleEndedIterator<Item = K>;

    fn keys_by_index3_range(
        &'a self,
        range: impl RangeBounds<I3>,
    ) -> impl DoubleEndedIterator<Item = K>;

    fn keys_by_index4_range(
        &'a self,
        range: impl RangeBounds<I4>,
    ) -> impl DoubleEndedIterator<Item = K>;

    fn keys_by_index5_range(
        &'a self,
        range: impl RangeBounds<I5>,
    ) -> impl DoubleEndedIterator<Item = K>;

    fn any_by_index1_range(&'a self, range: impl RangeBounds<I1>) -> bool {
        self.keys_by_index1_range(range).next().is_some()
    }

    fn any_by_index2_range(&'a self, range: impl RangeBounds<I2>) -> bool {
        self.keys_by_index2_range(range).next().is_some()
    }

    fn any_by_index3_range(&'a self, range: impl RangeBounds<I3>) -> bool {
        self.keys_by_index3_range(range).next().is_some()
    }

    fn any_by_index4_range(&'a self, range: impl RangeBounds<I4>) -> bool {
        self.keys_by_index4_range(range).next().is_some()
    }

    fn any_by_index5_range(&'a self, range: impl RangeBounds<I5>) -> bool {
        self.keys_by_index5_range(range).next().is_some()
    }

    fn indices1(&'a self) -> impl Iterator<Item = I1>;

    fn indices2(&'a self) -> impl Iterator<Item = &'a I2>;

    fn indices3(&'a self) -> impl Iterator<Item = &'a I3>;

    fn indices4(&'a self) -> impl Iterator<Item = &'a I4>;

    fn indices5(&'a self) -> impl Iterator<Item = &'a I5>;

    fn indices1_by_range(
        &'a self,
        range: impl RangeBounds<I1>,
    ) -> impl DoubleEndedIterator<Item = I1>;

    fn indices2_by_range(
        &'a self,
        range: impl RangeBounds<I2>,
    ) -> impl DoubleEndedIterator<Item = &'a I2>;

    fn indices3_by_range(
        &'a self,
        range: impl RangeBounds<I3>,
    ) -> impl DoubleEndedIterator<Item = &'a I3>;

    fn indices4_by_range(
        &'a self,
        range: impl RangeBounds<I4>,
    ) -> impl DoubleEndedIterator<Item = &'a I4>;

    fn indices5_by_range(
        &'a self,
        range: impl RangeBounds<I5>,
    ) -> impl DoubleEndedIterator<Item = &'a I5>;
}

pub trait GuidTableHandle<'a, K, V: 'a, I1, I2: 'a, I3: 'a, I4: 'a, I5: 'a>:
    GuidTableIndexer<'a, K, V, I1, I2, I3, I4, I5>
{
    fn get(&self, guid: K) -> Option<&Lock<V>>;
}

fn keys_by_index<'a, I: Clone + Ord, K: Copy + Ord>(
    index: &I,
    index_map: &'a BTreeMap<I, BTreeSet<K>>,
) -> impl Iterator<Item = K> + use<'a, I, K> {
    index_map
        .get(index)
        .map(|index_list| index_list.iter())
        .unwrap_or_default()
        .cloned()
}

fn keys_by_index_range<I: Clone + Ord, K: Copy + Ord, R: RangeBounds<I>>(
    range: R,
    index_map: &BTreeMap<I, BTreeSet<K>>,
) -> impl DoubleEndedIterator<Item = K> + use<'_, I, K, R> {
    index_map
        .range(range)
        .flat_map(|(_, keys)| keys.iter().copied())
}

fn indices_by_range<I: Clone + Ord, K: Copy + Ord, R: RangeBounds<I>>(
    range: R,
    index_map: &BTreeMap<I, BTreeSet<K>>,
) -> impl DoubleEndedIterator<Item = &I> + use<'_, I, K, R> {
    index_map.range(range).filter_map(
        |(index, guids)| {
            if guids.is_empty() {
                None
            } else {
                Some(index)
            }
        },
    )
}

pub struct GuidTableReadHandle<'a, K, V, I1 = (), I2 = (), I3 = (), I4 = (), I5 = ()> {
    guard: RwLockReadGuard<'a, GuidTableData<K, V, I1, I2, I3, I4, I5>>,
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
    for GuidTableReadHandle<'a, K, V, I1, I2, I3, I4, I5>
{
    fn index1(&self, guid: K) -> Option<I1> {
        self.guard.data.get(&guid).map(|(_, index1, ..)| *index1)
    }

    fn index2(&self, guid: K) -> Option<&I2> {
        self.guard
            .data
            .get(&guid)
            .and_then(|(_, _, index2, ..)| index2.as_ref())
    }

    fn index3(&self, guid: K) -> Option<&I3> {
        self.guard
            .data
            .get(&guid)
            .and_then(|(_, _, _, index3, ..)| index3.as_ref())
    }

    fn index4(&self, guid: K) -> Option<&I4> {
        self.guard
            .data
            .get(&guid)
            .and_then(|(_, _, _, _, index4, ..)| index4.as_ref())
    }

    fn index5(&self, guid: K) -> Option<&I5> {
        self.guard
            .data
            .get(&guid)
            .and_then(|(_, _, _, _, _, index5, ..)| index5.as_ref())
    }

    fn keys(&'a self) -> impl Iterator<Item = K> {
        self.guard.data.keys().cloned()
    }

    fn keys_by_index1(&'a self, index: I1) -> impl Iterator<Item = K> {
        keys_by_index(&index, &self.guard.index1)
    }

    fn keys_by_index2(&'a self, index: &I2) -> impl Iterator<Item = K> {
        keys_by_index(index, &self.guard.index2)
    }

    fn keys_by_index3(&'a self, index: &I3) -> impl Iterator<Item = K> {
        keys_by_index(index, &self.guard.index3)
    }

    fn keys_by_index4(&'a self, index: &I4) -> impl Iterator<Item = K> {
        keys_by_index(index, &self.guard.index4)
    }

    fn keys_by_index5(&'a self, index: &I5) -> impl Iterator<Item = K> {
        keys_by_index(index, &self.guard.index5)
    }

    fn keys_by_index1_range(
        &'a self,
        range: impl RangeBounds<I1>,
    ) -> impl DoubleEndedIterator<Item = K> {
        keys_by_index_range(range, &self.guard.index1)
    }

    fn keys_by_index2_range(
        &'a self,
        range: impl RangeBounds<I2>,
    ) -> impl DoubleEndedIterator<Item = K> {
        keys_by_index_range(range, &self.guard.index2)
    }

    fn keys_by_index3_range(
        &'a self,
        range: impl RangeBounds<I3>,
    ) -> impl DoubleEndedIterator<Item = K> {
        keys_by_index_range(range, &self.guard.index3)
    }

    fn keys_by_index4_range(
        &'a self,
        range: impl RangeBounds<I4>,
    ) -> impl DoubleEndedIterator<Item = K> {
        keys_by_index_range(range, &self.guard.index4)
    }

    fn keys_by_index5_range(
        &'a self,
        range: impl RangeBounds<I5>,
    ) -> impl DoubleEndedIterator<Item = K> {
        keys_by_index_range(range, &self.guard.index5)
    }

    fn indices1(&'a self) -> impl Iterator<Item = I1> {
        indices_by_range(.., &self.guard.index1).cloned()
    }

    fn indices2(&'a self) -> impl Iterator<Item = &'a I2> {
        indices_by_range(.., &self.guard.index2)
    }

    fn indices3(&'a self) -> impl Iterator<Item = &'a I3> {
        indices_by_range(.., &self.guard.index3)
    }

    fn indices4(&'a self) -> impl Iterator<Item = &'a I4> {
        indices_by_range(.., &self.guard.index4)
    }

    fn indices5(&'a self) -> impl Iterator<Item = &'a I5> {
        indices_by_range(.., &self.guard.index5)
    }

    fn indices1_by_range(
        &'a self,
        range: impl RangeBounds<I1>,
    ) -> impl DoubleEndedIterator<Item = I1> {
        indices_by_range(range, &self.guard.index1).cloned()
    }

    fn indices2_by_range(
        &'a self,
        range: impl RangeBounds<I2>,
    ) -> impl DoubleEndedIterator<Item = &'a I2> {
        indices_by_range(range, &self.guard.index2)
    }

    fn indices3_by_range(
        &'a self,
        range: impl RangeBounds<I3>,
    ) -> impl DoubleEndedIterator<Item = &'a I3> {
        indices_by_range(range, &self.guard.index3)
    }

    fn indices4_by_range(
        &'a self,
        range: impl RangeBounds<I4>,
    ) -> impl DoubleEndedIterator<Item = &'a I4> {
        indices_by_range(range, &self.guard.index4)
    }

    fn indices5_by_range(
        &'a self,
        range: impl RangeBounds<I5>,
    ) -> impl DoubleEndedIterator<Item = &'a I5> {
        indices_by_range(range, &self.guard.index5)
    }
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
    > GuidTableHandle<'a, K, V, I1, I2, I3, I4, I5>
    for GuidTableReadHandle<'a, K, V, I1, I2, I3, I4, I5>
{
    fn get(&self, guid: K) -> Option<&Lock<V>> {
        self.guard.data.get(&guid).map(|(item, ..)| item)
    }
}

pub struct GuidTableWriteHandle<'a, K, V, I1 = (), I2 = (), I3 = (), I4 = (), I5 = ()> {
    guard: RwLockWriteGuard<'a, GuidTableData<K, V, I1, I2, I3, I4, I5>>,
}

impl<
        K: Copy + Ord,
        V: IndexedGuid<K, I1, I2, I3, I4, I5>,
        I1: Copy + Ord,
        I2: Clone + Ord,
        I3: Clone + Ord,
        I4: Clone + Ord,
        I5: Clone + Ord,
    > GuidTableWriteHandle<'_, K, V, I1, I2, I3, I4, I5>
{
    pub fn get(&self, guid: K) -> Option<&Lock<V>> {
        self.guard.data.get(&guid).map(|(lock, ..)| lock)
    }

    pub fn values_by_index1(&self, index: I1) -> impl Iterator<Item = &Lock<V>> {
        self.keys_by_index1(index)
            .filter_map(|guid| self.guard.data.get(&guid).map(|(lock, ..)| lock))
    }

    pub fn insert(&mut self, item: V) -> Option<Lock<V>> {
        let key = item.guid();
        let index1 = item.index1();
        let index2 = item.index2();
        let index3 = item.index3();
        let index4 = item.index4();
        let index5 = item.index5();

        self.insert_with_index(key, index1, index2, index3, index4, index5, Lock::new(item))
    }

    pub fn insert_lock(
        &mut self,
        guid: K,
        index1: I1,
        index2: Option<I2>,
        index3: Option<I3>,
        index4: Option<I4>,
        index5: Option<I5>,
        lock: Lock<V>,
    ) -> Option<Lock<V>> {
        self.insert_with_index(guid, index1, index2, index3, index4, index5, lock)
    }

    pub fn remove(&mut self, guid: K) -> Option<GuidTableEntry<V, I1, I2, I3, I4, I5>> {
        let previous = self.guard.data.remove(&guid);
        if let Some((
            _,
            previous_index1,
            previous_index2,
            previous_index3,
            previous_index4,
            previous_index5,
        )) = &previous
        {
            self.guard
                .index1
                .get_mut(previous_index1)
                .expect("GUID table key was never added to index1")
                .remove(&guid);

            if let Some(index2) = previous_index2 {
                let values_for_index = self
                    .guard
                    .index2
                    .get_mut(index2)
                    .expect("GUID table key was never added to index2");
                values_for_index.remove(&guid);
                if values_for_index.is_empty() {
                    self.guard.index2.remove(index2);
                }
            }

            if let Some(index3) = previous_index3 {
                let values_for_index = self
                    .guard
                    .index3
                    .get_mut(index3)
                    .expect("GUID table key was never added to index3");
                values_for_index.remove(&guid);
                if values_for_index.is_empty() {
                    self.guard.index3.remove(index3);
                }
            }

            if let Some(index4) = previous_index4 {
                let values_for_index = self
                    .guard
                    .index4
                    .get_mut(index4)
                    .expect("GUID table key was never added to index4");
                values_for_index.remove(&guid);
                if values_for_index.is_empty() {
                    self.guard.index4.remove(index4);
                }
            }

            if let Some(index5) = previous_index5 {
                let values_for_index = self
                    .guard
                    .index5
                    .get_mut(index5)
                    .expect("GUID table key was never added to index5");
                values_for_index.remove(&guid);
                if values_for_index.is_empty() {
                    self.guard.index5.remove(index5);
                }
            }
        }

        previous
    }

    pub fn update_value_indices<T>(
        &mut self,
        guid: K,
        mut f: impl FnMut(Option<&mut RwLockWriteGuard<V>>, &Self) -> T,
    ) -> T {
        let entry = self.remove(guid);
        if let Some((lock, ..)) = entry {
            let mut value_write_handle = lock.write();

            let result = f(Some(&mut value_write_handle), self);

            let guid = value_write_handle.guid();
            let index1 = value_write_handle.index1();
            let index2 = value_write_handle.index2();
            let index3 = value_write_handle.index3();
            let index4 = value_write_handle.index4();
            let index5 = value_write_handle.index5();
            drop(value_write_handle);
            self.insert_lock(guid, index1, index2, index3, index4, index5, lock);

            result
        } else {
            f(None, self)
        }
    }

    fn insert_with_index(
        &mut self,
        key: K,
        index1: I1,
        index2: Option<I2>,
        index3: Option<I3>,
        index4: Option<I4>,
        index5: Option<I5>,
        item: Lock<V>,
    ) -> Option<Lock<V>> {
        // Remove from the index before inserting the new key in case the item has the same key
        let previous = self.remove(key);

        if let Some(value) = &index2 {
            self.guard
                .index2
                .entry(value.clone())
                .or_default()
                .insert(key);
        }
        if let Some(value) = &index3 {
            self.guard
                .index3
                .entry(value.clone())
                .or_default()
                .insert(key);
        }
        if let Some(value) = &index4 {
            self.guard
                .index4
                .entry(value.clone())
                .or_default()
                .insert(key);
        }
        if let Some(value) = &index5 {
            self.guard
                .index5
                .entry(value.clone())
                .or_default()
                .insert(key);
        }
        self.guard
            .data
            .insert(key, (item, index1, index2, index3, index4, index5));
        self.guard.index1.entry(index1).or_default().insert(key);

        previous.map(|(item, ..)| item)
    }
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
    for GuidTableWriteHandle<'a, K, V, I1, I2, I3, I4, I5>
{
    fn index1(&self, guid: K) -> Option<I1> {
        self.guard.data.get(&guid).map(|(_, index, ..)| *index)
    }

    fn index2(&self, guid: K) -> Option<&I2> {
        self.guard
            .data
            .get(&guid)
            .and_then(|(_, _, index, ..)| index.as_ref())
    }

    fn index3(&self, guid: K) -> Option<&I3> {
        self.guard
            .data
            .get(&guid)
            .and_then(|(_, _, _, index, ..)| index.as_ref())
    }

    fn index4(&self, guid: K) -> Option<&I4> {
        self.guard
            .data
            .get(&guid)
            .and_then(|(_, _, _, _, index, ..)| index.as_ref())
    }

    fn index5(&self, guid: K) -> Option<&I5> {
        self.guard
            .data
            .get(&guid)
            .and_then(|(_, _, _, _, _, index, ..)| index.as_ref())
    }

    fn keys(&'a self) -> impl Iterator<Item = K> {
        self.guard.data.keys().cloned()
    }

    fn keys_by_index1(&'a self, index: I1) -> impl Iterator<Item = K> {
        keys_by_index(&index, &self.guard.index1)
    }

    fn keys_by_index2(&'a self, index: &I2) -> impl Iterator<Item = K> {
        keys_by_index(index, &self.guard.index2)
    }

    fn keys_by_index3(&'a self, index: &I3) -> impl Iterator<Item = K> {
        keys_by_index(index, &self.guard.index3)
    }

    fn keys_by_index4(&'a self, index: &I4) -> impl Iterator<Item = K> {
        keys_by_index(index, &self.guard.index4)
    }

    fn keys_by_index5(&'a self, index: &I5) -> impl Iterator<Item = K> {
        keys_by_index(index, &self.guard.index5)
    }

    fn keys_by_index1_range(
        &'a self,
        range: impl RangeBounds<I1>,
    ) -> impl DoubleEndedIterator<Item = K> {
        keys_by_index_range(range, &self.guard.index1)
    }

    fn keys_by_index2_range(
        &'a self,
        range: impl RangeBounds<I2>,
    ) -> impl DoubleEndedIterator<Item = K> {
        keys_by_index_range(range, &self.guard.index2)
    }

    fn keys_by_index3_range(
        &'a self,
        range: impl RangeBounds<I3>,
    ) -> impl DoubleEndedIterator<Item = K> {
        keys_by_index_range(range, &self.guard.index3)
    }

    fn keys_by_index4_range(
        &'a self,
        range: impl RangeBounds<I4>,
    ) -> impl DoubleEndedIterator<Item = K> {
        keys_by_index_range(range, &self.guard.index4)
    }

    fn keys_by_index5_range(
        &'a self,
        range: impl RangeBounds<I5>,
    ) -> impl DoubleEndedIterator<Item = K> {
        keys_by_index_range(range, &self.guard.index5)
    }

    fn indices1(&'a self) -> impl Iterator<Item = I1> {
        indices_by_range(.., &self.guard.index1).cloned()
    }

    fn indices2(&'a self) -> impl Iterator<Item = &'a I2> {
        indices_by_range(.., &self.guard.index2)
    }

    fn indices3(&'a self) -> impl Iterator<Item = &'a I3> {
        indices_by_range(.., &self.guard.index3)
    }

    fn indices4(&'a self) -> impl Iterator<Item = &'a I4> {
        indices_by_range(.., &self.guard.index4)
    }

    fn indices5(&'a self) -> impl Iterator<Item = &'a I5> {
        indices_by_range(.., &self.guard.index5)
    }

    fn indices1_by_range(
        &'a self,
        range: impl RangeBounds<I1>,
    ) -> impl DoubleEndedIterator<Item = I1> {
        indices_by_range(range, &self.guard.index1).cloned()
    }

    fn indices2_by_range(
        &'a self,
        range: impl RangeBounds<I2>,
    ) -> impl DoubleEndedIterator<Item = &'a I2> {
        indices_by_range(range, &self.guard.index2)
    }

    fn indices3_by_range(
        &'a self,
        range: impl RangeBounds<I3>,
    ) -> impl DoubleEndedIterator<Item = &'a I3> {
        indices_by_range(range, &self.guard.index3)
    }

    fn indices4_by_range(
        &'a self,
        range: impl RangeBounds<I4>,
    ) -> impl DoubleEndedIterator<Item = &'a I4> {
        indices_by_range(range, &self.guard.index4)
    }

    fn indices5_by_range(
        &'a self,
        range: impl RangeBounds<I5>,
    ) -> impl DoubleEndedIterator<Item = &'a I5> {
        indices_by_range(range, &self.guard.index5)
    }
}

impl<
        'a,
        K: Copy + Ord,
        I1: Copy + Ord,
        I2: Clone + Ord,
        I3: Clone + Ord,
        I4: Clone + Ord,
        I5: Clone + Ord,
        V: IndexedGuid<K, I1, I2, I3, I4, I5>,
    > GuidTableHandle<'a, K, V, I1, I2, I3, I4, I5>
    for GuidTableWriteHandle<'a, K, V, I1, I2, I3, I4, I5>
{
    fn get(&self, guid: K) -> Option<&Lock<V>> {
        self.guard.data.get(&guid).map(|(item, ..)| item)
    }
}

pub struct GuidTable<K, V, I1 = (), I2 = (), I3 = (), I4 = (), I5 = ()> {
    data: Lock<GuidTableData<K, V, I1, I2, I3, I4, I5>>,
}

impl<K, I1, I2, I3, I4, I5, V: IndexedGuid<K, I1, I2, I3, I4, I5>>
    GuidTable<K, V, I1, I2, I3, I4, I5>
{
    pub fn new() -> Self {
        GuidTable {
            data: Lock::new(GuidTableData::new()),
        }
    }

    pub fn read(&self) -> GuidTableReadHandle<K, V, I1, I2, I3, I4, I5> {
        GuidTableReadHandle {
            guard: self.data.read(),
        }
    }

    pub fn write(&self) -> GuidTableWriteHandle<K, V, I1, I2, I3, I4, I5> {
        GuidTableWriteHandle {
            guard: self.data.write(),
        }
    }
}
