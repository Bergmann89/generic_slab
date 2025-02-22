use core::fmt::{Debug, Formatter, Result as FmtResult};
use core::iter::{Enumerate, FromIterator};
use core::mem::{forget, replace};
use core::ops::{Index, IndexMut};
use core::slice::{Iter as SliceIter, IterMut as SliceIterMut};

use crate::builder::Builder;
use crate::entries::{DynamicEntries, Entries, Entry};
use crate::iter::{DrainMapper, GenericIter, KeyMapper, KeyValueMapper, ValueMapper};
use crate::key::Key;

#[cfg(feature = "range")]
use core::ops::{Bound, RangeBounds};

#[cfg(feature = "range")]
use crate::{
    entries::RangeIndices,
    range_iter::{EntriesMutRef, EntriesRef, GenericRangeIter},
};

/// Pre-allocated generic storage for a uniform data type
///
/// See the [module documentation] for more details.
///
/// [module documentation]: index.html
pub struct GenericSlab<T, TKey, TEntries>
where
    TKey: Key<T>,
{
    pub(crate) meta: Meta<T, TKey>,
    pub(crate) entries: TEntries,
}

/// Helper type to store the meta information of a slab
#[derive(Debug)]
pub struct Meta<T, TKey>
where
    TKey: Key<T>,
{
    // Number of elements currently stored in the slab
    pub(crate) len: usize,

    // Context to store key related data in
    pub(crate) key_context: TKey::Context,

    // Index of the first vacant entry
    pub(crate) first_vacant: usize,

    /// Index of the first occupied entry
    #[cfg(feature = "range")]
    pub(crate) first_occupied: usize,

    /// Index of the last occupied entry
    #[cfg(feature = "range")]
    pub(crate) last_occupied: usize,
}

/// Represents a vacant entry in the [`GenericSlab`].
pub struct GenericVacantEntry<'a, T, TKey, TEntries>
where
    TKey: Key<T>,
{
    index: usize,
    slab: &'a mut GenericSlab<T, TKey, TEntries>,
}

/// Iterator that emits the key and the reference to the items stored in the slab.
pub type Iter<'a, T, TKey> =
    GenericIter<Enumerate<SliceIter<'a, Entry<T, TKey>>>, KeyValueMapper<&'a Meta<T, TKey>>>;

/// Iterator that emits the key and the mutable reference to the items stored in the slab.
pub type IterMut<'a, T, TKey> =
    GenericIter<Enumerate<SliceIterMut<'a, Entry<T, TKey>>>, KeyValueMapper<&'a Meta<T, TKey>>>;

/// Iterator that emits the keys an the items stored in the slab by consuming the slab.
pub type IntoIter<T, TKey, TEntries> =
    GenericIter<Enumerate<<TEntries as IntoIterator>::IntoIter>, KeyValueMapper<Meta<T, TKey>>>;

/// Iterator that emits the keys of the items stored in the slab.
pub type Keys<'a, T, TKey> =
    GenericIter<Enumerate<SliceIter<'a, Entry<T, TKey>>>, KeyMapper<&'a Meta<T, TKey>>>;

/// Iterator that emits references to the items stored in the slab.
pub type Values<'a, T, TKey> = GenericIter<SliceIter<'a, Entry<T, TKey>>, ValueMapper>;

/// Iterator that emits mutable references to the items stored in the slab.
pub type ValuesMut<'a, T, TKey> = GenericIter<SliceIterMut<'a, Entry<T, TKey>>, ValueMapper>;

/// Iterator that emits the items stored in the slab by consuming the slab.
pub type IntoValues<TEntries> = GenericIter<<TEntries as IntoIterator>::IntoIter, ValueMapper>;

/// Iterator that emits references to the stored items in insertion order in the selected range.
#[cfg(feature = "range")]
pub type RangeIter<'a, T, TKey, TEntries> = GenericRangeIter<EntriesRef<'a, T, TKey, TEntries>>;

/// Iterator that emits mutable references to the stored items in insertion order in the selected range.
#[cfg(feature = "range")]
pub type RangeIterMut<'a, T, TKey, TEntries> =
    GenericRangeIter<EntriesMutRef<'a, T, TKey, TEntries>>;

/// Iterator that emits the items stored in slab by draining the slab.
pub type Drain<'a, T, TKey> =
    GenericIter<Enumerate<SliceIterMut<'a, Entry<T, TKey>>>, DrainMapper<&'a mut Meta<T, TKey>>>;

/* GenericSlab */

impl<T, TKey, TEntries> GenericSlab<T, TKey, TEntries>
where
    TKey: Key<T>,
    TEntries: Entries<T, TKey> + Default,
{
    /// Construct a new, empty `Slab`.
    ///
    /// The function does not allocate and the returned slab will have no
    /// capacity until `insert` is called or capacity is explicitly reserved.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let slab: Slab<i32> = Slab::new();
    /// ```
    pub fn new() -> Self {
        Self::from_entries(TEntries::default())
    }
}

impl<T, TKey, TEntries> GenericSlab<T, TKey, TEntries>
where
    TKey: Key<T>,
    TEntries: Entries<T, TKey>,
{
    /// Construct a new, empty `Slab` using the provided `entries``.
    ///
    /// Before the slab is created the passed `entries` will be cleared.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let slab: Slab<i32> = Slab::from_entries(Vec::with_capacity(100));
    /// ```
    pub fn from_entries(mut entries: TEntries) -> Self {
        entries.clear();

        let meta = Meta {
            len: 0,
            key_context: TKey::Context::default(),
            first_vacant: 0,
            #[cfg(feature = "range")]
            first_occupied: crate::INVALID_INDEX,
            #[cfg(feature = "range")]
            last_occupied: crate::INVALID_INDEX,
        };

        Self { meta, entries }
    }

    /// Get a reference to the key context.
    pub fn key_context(&self) -> &TKey::Context {
        &self.meta.key_context
    }

    /// Get a reference to the key context.
    pub fn key_context_mut(&mut self) -> &mut TKey::Context {
        &mut self.meta.key_context
    }

    /// Return the number of values the slab can store without reallocating.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let slab: Slab<i32> = Slab::with_capacity(10);
    /// assert_eq!(slab.capacity(), 10);
    /// ```
    pub fn capacity(&self) -> usize {
        self.entries.capacity()
    }

    /// Clear the slab of all values.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    ///
    /// for i in 0..3 {
    ///     slab.insert(i);
    /// }
    ///
    /// slab.clear();
    /// assert!(slab.is_empty());
    /// ```
    pub fn clear(&mut self) {
        self.entries.clear();
        self.meta.len = 0;
        self.meta.first_vacant = 0;

        #[cfg(feature = "range")]
        {
            self.meta.first_occupied = crate::INVALID_INDEX;
            self.meta.last_occupied = crate::INVALID_INDEX;
        }
    }

    /// Return the number of stored values.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    ///
    /// for i in 0..3 {
    ///     slab.insert(i);
    /// }
    ///
    /// assert_eq!(3, slab.len());
    /// ```
    pub fn len(&self) -> usize {
        self.meta.len
    }

    /// Return `true` if there are no values stored in the slab.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    /// assert!(slab.is_empty());
    ///
    /// slab.insert(1);
    /// assert!(!slab.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.meta.len == 0
    }

    /// Return a reference to the value associated with the given key.
    ///
    /// If the given key is not associated with a value, then `None` is
    /// returned.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    /// let key = slab.insert("hello");
    ///
    /// assert_eq!(slab.get(key), Some(&"hello"));
    /// assert_eq!(slab.get(123), None);
    /// ```
    #[inline]
    pub fn get(&self, key: TKey) -> Option<&T> {
        self.get_at(&key)
    }

    /// Return a mutable reference to the value associated with the given key.
    ///
    /// If the given key is not associated with a value, then `None` is
    /// returned.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    /// let key = slab.insert("hello");
    ///
    /// *slab.get_mut(key).unwrap() = "world";
    ///
    /// assert_eq!(slab[key], "world");
    /// assert_eq!(slab.get_mut(123), None);
    /// ```
    #[inline]
    pub fn get_mut(&mut self, key: TKey) -> Option<&mut T> {
        self.get_mut_at(&key)
    }

    /// Return two mutable references to the values associated with the two
    /// given keys simultaneously.
    ///
    /// If any one of the given keys is not associated with a value, then `None`
    /// is returned.
    ///
    /// This function can be used to get two mutable references out of one slab,
    /// so that you can manipulate both of them at the same time, eg. swap them.
    ///
    /// # Panics
    ///
    /// This function will panic if `key1` and `key2` are the same.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// use std::mem;
    ///
    /// let mut slab = Slab::new();
    /// let key1 = slab.insert(1);
    /// let key2 = slab.insert(2);
    /// let (value1, value2) = slab.get2_mut(key1, key2).unwrap();
    /// mem::swap(value1, value2);
    /// assert_eq!(slab[key1], 2);
    /// assert_eq!(slab[key2], 1);
    /// ```
    pub fn get2_mut(&mut self, key1: TKey, key2: TKey) -> Option<(&mut T, &mut T)> {
        let index1 = key1.index(&self.meta.key_context);
        let index2 = key2.index(&self.meta.key_context);

        assert!(index1 != index2);

        let (entry1, entry2) = if index1 > index2 {
            let (slice1, slice2) = self.entries.as_mut().split_at_mut(index1);

            (slice2.get_mut(0), slice1.get_mut(index2))
        } else {
            let (slice1, slice2) = self.entries.as_mut().split_at_mut(index2);

            (slice1.get_mut(index1), slice2.get_mut(0))
        };

        match (entry1, entry2) {
            (
                Some(Entry::Occupied {
                    value: value1,
                    key_data: data1,
                    ..
                }),
                Some(Entry::Occupied {
                    value: value2,
                    key_data: data2,
                    ..
                }),
            ) if key1.verify(&self.meta.key_context, data1)
                && key2.verify(&self.meta.key_context, data2) =>
            {
                Some((value1, value2))
            }
            _ => None,
        }
    }

    /// Return a reference to the value associated with the given key without
    /// performing bounds checking.
    ///
    /// For a safe alternative see [`get`](GenericSlab::get).
    ///
    /// This function should be used with care.
    ///
    /// # Safety
    ///
    /// The key must be within bounds.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    /// let key = slab.insert(2);
    ///
    /// unsafe {
    ///     assert_eq!(slab.get_unchecked(key), &2);
    /// }
    /// ```
    pub unsafe fn get_unchecked(&self, key: TKey) -> &T {
        let index = key.index(&self.meta.key_context);
        match self.entries.as_ref().get_unchecked(index) {
            Entry::Occupied { value, .. } => value,
            _ => unreachable!(),
        }
    }

    /// Return a mutable reference to the value associated with the given key
    /// without performing bounds checking.
    ///
    /// For a safe alternative see [`get_mut`](GenericSlab::get_mut).
    ///
    /// This function should be used with care.
    ///
    /// # Safety
    ///
    /// The key must be within bounds.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    /// let key = slab.insert(2);
    ///
    /// unsafe {
    ///     let val = slab.get_unchecked_mut(key);
    ///     *val = 13;
    /// }
    ///
    /// assert_eq!(slab[key], 13);
    /// ```
    pub unsafe fn get_unchecked_mut(&mut self, key: TKey) -> &mut T {
        let index = key.index(&self.meta.key_context);
        match self.entries.as_mut().get_unchecked_mut(index) {
            Entry::Occupied { value, .. } => value,
            _ => unreachable!(),
        }
    }

    /// Return two mutable references to the values associated with the two
    /// given keys simultaneously without performing bounds checking and safety
    /// condition checking.
    ///
    /// For a safe alternative see [`get2_mut`](GenericSlab::get2_mut).
    ///
    /// This function should be used with care.
    ///
    /// # Safety
    ///
    /// - Both keys must be within bounds.
    /// - The condition `key1 != key2` must hold.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// use std::mem;
    ///
    /// let mut slab = Slab::new();
    /// let key1 = slab.insert(1);
    /// let key2 = slab.insert(2);
    /// let (value1, value2) = unsafe { slab.get2_unchecked_mut(key1, key2) };
    /// mem::swap(value1, value2);
    /// assert_eq!(slab[key1], 2);
    /// assert_eq!(slab[key2], 1);
    /// ```
    pub unsafe fn get2_unchecked_mut(&mut self, key1: TKey, key2: TKey) -> (&mut T, &mut T) {
        let index1 = key1.index(&self.meta.key_context);
        let index2 = key2.index(&self.meta.key_context);

        debug_assert_ne!(index1, index2);

        let ptr = self.entries.as_mut().as_mut_ptr();

        let ptr1 = ptr.add(index1);
        let ptr2 = ptr.add(index2);

        match (&mut *ptr1, &mut *ptr2) {
            (Entry::Occupied { value: value1, .. }, Entry::Occupied { value: value2, .. }) => {
                (value1, value2)
            }
            _ => unreachable!(),
        }
    }

    /// Return an iterator over the slab.
    ///
    /// This function should generally be **avoided** as it is not efficient.
    /// Iterators must iterate over every slot in the slab even if it is
    /// vacant. As such, a slab with a capacity of 1 million but only one
    /// stored value must still iterate the million slots.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    ///
    /// for i in 0..3 {
    ///     slab.insert(i);
    /// }
    ///
    /// let mut iterator = slab.iter();
    ///
    /// assert_eq!(iterator.next(), Some((0, &0)));
    /// assert_eq!(iterator.next(), Some((1, &1)));
    /// assert_eq!(iterator.next(), Some((2, &2)));
    /// assert_eq!(iterator.next(), None);
    /// ```
    pub fn iter(&self) -> Iter<'_, T, TKey> {
        GenericIter::new(
            self.meta.len,
            self.entries.as_ref().iter().enumerate(),
            KeyValueMapper::new(&self.meta),
        )
    }

    /// Return an iterator that allows modifying each value.
    ///
    /// This function should generally be **avoided** as it is not efficient.
    /// Iterators must iterate over every slot in the slab even if it is
    /// vacant. As such, a slab with a capacity of 1 million but only one
    /// stored value must still iterate the million slots.
    ///
    /// If you are interested in an efficient iterator, check [`Self::ordered_iter`].
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    ///
    /// let key1 = slab.insert(0);
    /// let key2 = slab.insert(1);
    ///
    /// for (key, val) in slab.iter_mut() {
    ///     if key == key1 {
    ///         *val += 2;
    ///     }
    /// }
    ///
    /// assert_eq!(slab[key1], 2);
    /// assert_eq!(slab[key2], 1);
    /// ```
    pub fn iter_mut(&mut self) -> IterMut<'_, T, TKey> {
        GenericIter::new(
            self.meta.len,
            self.entries.as_mut().iter_mut().enumerate(),
            KeyValueMapper::new(&self.meta),
        )
    }

    /// Return an iterator over the keys of the slab.
    ///
    /// This function should generally be **avoided** as it is not efficient.
    /// Iterators must iterate over every slot in the slab even if it is
    /// vacant. As such, a slab with a capacity of 1 million but only one
    /// stored value must still iterate the million slots.
    ///
    /// If you are interested in an efficient iterator, check [`Self::ordered_iter`].
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    ///
    /// for i in 0..3 {
    ///     slab.insert(i);
    /// }
    ///
    /// let mut iterator = slab.keys();
    ///
    /// assert_eq!(iterator.next(), Some(0));
    /// assert_eq!(iterator.next(), Some(1));
    /// assert_eq!(iterator.next(), Some(2));
    /// assert_eq!(iterator.next(), None);
    /// ```
    pub fn keys(&self) -> Keys<'_, T, TKey> {
        GenericIter::new(
            self.meta.len,
            self.entries.as_ref().iter().enumerate(),
            KeyMapper::new(&self.meta),
        )
    }

    /// Return an iterator that emits references to the stored items.
    ///
    /// This function should generally be **avoided** as it is not efficient.
    /// Iterators must iterate over every slot in the slab even if it is
    /// vacant. As such, a slab with a capacity of 1 million but only one
    /// stored value must still iterate the million slots.
    ///
    /// If you are interested in an efficient iterator, check [`Self::ordered_iter`].
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    ///
    /// for i in 0..3 {
    ///     slab.insert(i);
    /// }
    ///
    /// let mut iterator = slab.values();
    ///
    /// assert_eq!(iterator.next(), Some(&0));
    /// assert_eq!(iterator.next(), Some(&1));
    /// assert_eq!(iterator.next(), Some(&2));
    /// assert_eq!(iterator.next(), None);
    /// ```
    pub fn values(&self) -> Values<'_, T, TKey> {
        GenericIter::new(self.meta.len, self.entries.as_ref().iter(), ValueMapper)
    }

    /// Return an iterator that emits mutable references to the stored items.
    ///
    /// This function should generally be **avoided** as it is not efficient.
    /// Iterators must iterate over every slot in the slab even if it is
    /// vacant. As such, a slab with a capacity of 1 million but only one
    /// stored value must still iterate the million slots.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    ///
    /// let key1 = slab.insert(0);
    /// let key2 = slab.insert(1);
    ///
    /// for val in slab.values_mut() {
    ///     *val += 2;
    /// }
    ///
    /// assert_eq!(slab[key1], 2);
    /// assert_eq!(slab[key2], 3);
    /// ```
    pub fn values_mut(&mut self) -> ValuesMut<'_, T, TKey> {
        GenericIter::new(self.meta.len, self.entries.as_mut().iter_mut(), ValueMapper)
    }

    /// Return an iterator that emits references to the stored items in insertion order.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    ///
    /// let key1 = slab.insert(0);
    /// let key2 = slab.insert(1);
    /// slab.remove(key1);
    /// let key3 = slab.insert(2);
    ///
    /// // `ordered_iter`` returns items in insertion order
    /// let mut iter = slab.ordered_iter();
    ///
    /// assert_eq!(iter.next(), Some((1, &1)));
    /// assert_eq!(iter.next(), Some((0, &2)));
    /// assert_eq!(iter.next(), None);
    ///
    /// // while `iter`` returns items in storage order
    /// let mut iter = slab.iter();
    ///
    /// assert_eq!(iter.next(), Some((0, &2)));
    /// assert_eq!(iter.next(), Some((1, &1)));
    /// assert_eq!(iter.next(), None);
    /// ```
    #[cfg(feature = "range")]
    pub fn ordered_iter(&self) -> RangeIter<'_, T, TKey, TEntries> {
        GenericRangeIter::new(
            Bound::Included(self.meta.first_occupied),
            Bound::Included(self.meta.last_occupied),
            EntriesRef::new(self),
        )
    }

    /// Return an iterator that emits mutable references to the stored items in insertion order.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    ///
    /// let key1 = slab.insert(0);
    /// let key2 = slab.insert(1);
    /// slab.remove(key1);
    /// let key3 = slab.insert(2);
    ///
    /// // `ordered_iter`` returns items in insertion order
    /// let mut iter = slab.ordered_iter_mut();
    ///
    /// assert_eq!(iter.next(), Some((1, &mut 1)));
    /// assert_eq!(iter.next(), Some((0, &mut 2)));
    /// assert_eq!(iter.next(), None);
    ///
    /// // while `iter`` returns items in storage order
    /// let mut iter = slab.iter_mut();
    ///
    /// assert_eq!(iter.next(), Some((0, &mut 2)));
    /// assert_eq!(iter.next(), Some((1, &mut 1)));
    /// assert_eq!(iter.next(), None);
    /// ```
    #[cfg(feature = "range")]
    pub fn ordered_iter_mut(&mut self) -> RangeIterMut<'_, T, TKey, TEntries> {
        GenericRangeIter::new(
            Bound::Included(self.meta.first_occupied),
            Bound::Included(self.meta.last_occupied),
            EntriesMutRef::new(self),
        )
    }

    /// Return an iterator that emits references to the stored items in
    /// insertion order in the passed range.
    ///
    /// Hint: If the keys that are used in the range are not valid, the iterator
    /// will be empty!
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    ///
    /// let key0 = slab.insert(0);
    /// let key1 = slab.insert(1);
    /// let key2 = slab.insert(2);
    /// let key3 = slab.insert(3);
    /// let key4 = slab.insert(4);
    /// let key5 = slab.insert(5);
    /// let key6 = slab.insert(6);
    ///
    /// // simple range iterator
    /// let mut iter = slab.range(key2..key5);
    /// assert_eq!(iter.next(), Some((2, &2)));
    /// assert_eq!(iter.next(), Some((3, &3)));
    /// assert_eq!(iter.next(), Some((4, &4)));
    /// assert_eq!(iter.next(), None);
    ///
    /// // a reverse range can be used to negate the set of returned items
    /// let mut iter = slab.range(key5..key2);
    /// assert_eq!(iter.next(), Some((5, &5)));
    /// assert_eq!(iter.next(), Some((6, &6)));
    /// assert_eq!(iter.next(), Some((0, &0)));
    /// assert_eq!(iter.next(), Some((1, &1)));
    /// assert_eq!(iter.next(), None);
    /// ```
    #[cfg(feature = "range")]
    pub fn range<R>(&self, range: R) -> RangeIter<'_, T, TKey, TEntries>
    where
        R: RangeBounds<TKey>,
    {
        let (front, back) = self.index_range(range);

        GenericRangeIter::new(front, back, EntriesRef::new(self))
    }

    /// Return an iterator that emits mutable references to the stored items in
    /// insertion order in the passed range.
    ///
    /// Hint: If the keys that are used in the range are not valid, the iterator
    /// will be empty!
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    ///
    /// let key0 = slab.insert(0);
    /// let key1 = slab.insert(1);
    /// let key2 = slab.insert(2);
    /// let key3 = slab.insert(3);
    /// let key4 = slab.insert(4);
    /// let key5 = slab.insert(5);
    /// let key6 = slab.insert(6);
    ///
    /// // simple range iterator
    /// let mut iter = slab.range_mut(key2..key5);
    /// assert_eq!(iter.next(), Some((2, &mut 2)));
    /// assert_eq!(iter.next(), Some((3, &mut 3)));
    /// assert_eq!(iter.next(), Some((4, &mut 4)));
    /// assert_eq!(iter.next(), None);
    ///
    /// // a reverse range can be used to negate the set of returned items
    /// let mut iter = slab.range_mut(key5..key2);
    /// assert_eq!(iter.next(), Some((5, &mut 5)));
    /// assert_eq!(iter.next(), Some((6, &mut 6)));
    /// assert_eq!(iter.next(), Some((0, &mut 0)));
    /// assert_eq!(iter.next(), Some((1, &mut 1)));
    /// assert_eq!(iter.next(), None);
    /// ```
    #[cfg(feature = "range")]
    pub fn range_mut<R>(&mut self, range: R) -> RangeIterMut<'_, T, TKey, TEntries>
    where
        R: RangeBounds<TKey>,
    {
        let (front, back) = self.index_range(range);

        GenericRangeIter::new(front, back, EntriesMutRef::new(self))
    }

    /// Insert a value in the slab, returning key assigned to the value.
    ///
    /// The returned key can later be used to retrieve or remove the value using indexed
    /// slab and `remove`. Additional capacity is allocated if needed. See
    /// [Capacity and reallocation](index.html#capacity-and-reallocation).
    ///
    /// # Panics
    ///
    /// Panics if the new storage in the vector exceeds `isize::MAX` bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    /// let key = slab.insert("hello");
    /// assert_eq!(slab[key], "hello");
    /// ```
    pub fn insert(&mut self, value: T) -> TKey {
        let index = self.meta.first_vacant;

        self.insert_at(index, value);

        self.occupied_key_at(index)
    }

    /// Returns the key of the next vacant entry.
    ///
    /// This function returns the key of the vacant entry which  will be used
    /// for the next insertion. This is equivalent to
    /// `slab.vacant_entry().key()`, but it doesn't require mutable access.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    /// assert_eq!(slab.vacant_key(), 0);
    ///
    /// slab.insert(0);
    /// assert_eq!(slab.vacant_key(), 1);
    ///
    /// slab.insert(1);
    /// slab.remove(0);
    /// assert_eq!(slab.vacant_key(), 0);
    /// ```
    pub fn vacant_key(&self) -> TKey {
        self.vacant_key_at(self.meta.first_vacant)
    }

    /// Return a handle to a vacant entry allowing for further manipulation.
    ///
    /// This function is useful when creating values that must contain their
    /// slab key. The returned `VacantEntry` reserves a slot in the slab and is
    /// able to query the associated key.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    ///
    /// let hello = {
    ///     let entry = slab.vacant_entry();
    ///     let key = entry.key();
    ///
    ///     entry.insert((key, "hello"));
    ///     key
    /// };
    ///
    /// assert_eq!(hello, slab[hello].0);
    /// assert_eq!("hello", slab[hello].1);
    /// ```
    pub fn vacant_entry(&mut self) -> GenericVacantEntry<'_, T, TKey, TEntries> {
        GenericVacantEntry {
            index: self.meta.first_vacant,
            slab: self,
        }
    }

    /// Return `true` if a value is associated with the given key.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    ///
    /// let hello = slab.insert("hello");
    /// assert!(slab.contains(hello));
    ///
    /// slab.remove(hello);
    ///
    /// assert!(!slab.contains(hello));
    /// ```
    pub fn contains(&self, key: TKey) -> bool {
        let index = key.index(&self.meta.key_context);

        match self.entries.as_ref().get(index) {
            Some(Entry::Occupied { key_data, .. })
                if key.verify(&self.meta.key_context, key_data) =>
            {
                true
            }
            _ => false,
        }
    }

    /// Tries to remove the value associated with the given key,
    /// returning the value if the key existed.
    ///
    /// The key is then released and may be associated with future stored
    /// values.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    ///
    /// let hello = slab.insert("hello");
    ///
    /// assert_eq!(slab.try_remove(hello), Some("hello"));
    /// assert!(!slab.contains(hello));
    /// ```
    pub fn try_remove(&mut self, key: TKey) -> Option<T> {
        let index = key.index(&self.meta.key_context);
        let entry = self.entries.as_mut().get_mut(index);

        match entry {
            Some(Entry::Occupied { key_data, .. })
                if key.verify(&self.meta.key_context, key_data) =>
            {
                Some(self.remove_at(index))
            }
            _ => None,
        }
    }

    /// Remove and return the value associated with the given key.
    ///
    /// The key is then released and may be associated with future stored
    /// values.
    ///
    /// # Panics
    ///
    /// Panics if `key` is not associated with a value.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    ///
    /// let hello = slab.insert("hello");
    ///
    /// assert_eq!(slab.remove(hello), "hello");
    /// assert!(!slab.contains(hello));
    /// ```
    pub fn remove(&mut self, key: TKey) -> T {
        self.try_remove(key).expect("invalid key")
    }

    /// Retain only the elements specified by the predicate.
    ///
    /// In other words, remove all elements `e` such that `f(usize, &mut e)`
    /// returns false. This method operates in place and preserves the key
    /// associated with the retained values.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    ///
    /// let k1 = slab.insert(0);
    /// let k2 = slab.insert(1);
    /// let k3 = slab.insert(2);
    ///
    /// slab.retain(|key, val| key == k1 || *val == 1);
    ///
    /// assert!(slab.contains(k1));
    /// assert!(slab.contains(k2));
    /// assert!(!slab.contains(k3));
    ///
    /// assert_eq!(2, slab.len());
    /// ```
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(TKey, &mut T) -> bool,
    {
        for index in 0..self.entries.as_mut().len() {
            let keep = match &mut self.entries.as_mut()[index] {
                Entry::Occupied {
                    value, key_data, ..
                } => f(
                    TKey::new_occupied(&self.meta.key_context, index, key_data),
                    value,
                ),
                _ => true,
            };

            if !keep {
                self.remove_at(index);
            }
        }
    }

    /// Shrink the capacity of the slab as much as possible without invalidating keys.
    ///
    /// Because values cannot be moved to a different index, the slab cannot
    /// shrink past any stored values.
    /// It will drop down as close as possible to the length but the allocator may
    /// still inform the underlying vector that there is space for a few more elements.
    ///
    /// This function can take O(n) time even when the capacity cannot be reduced
    /// or the allocation is shrunk in place. Repeated calls run in O(1) though.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::with_capacity(10);
    ///
    /// for i in 0..3 {
    ///     slab.insert(i);
    /// }
    ///
    /// slab.shrink_to_fit();
    /// assert!(slab.capacity() >= 3 && slab.capacity() < 10);
    /// ```
    ///
    /// The slab cannot shrink past the last present value even if previous
    /// values are removed:
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::with_capacity(10);
    ///
    /// for i in 0..4 {
    ///     slab.insert(i);
    /// }
    ///
    /// slab.remove(0);
    /// slab.remove(3);
    ///
    /// slab.shrink_to_fit();
    /// assert!(slab.capacity() >= 3 && slab.capacity() < 10);
    /// ```
    pub fn shrink_to_fit(&mut self) {
        // Remove all vacant entries after the last occupied one, so that
        // the capacity can be reduced to what is actually needed.
        // If the slab is empty the vector can simply be cleared, but that
        // optimization would not affect time complexity when T: Drop.
        let len_before = self.entries.as_ref().len();
        while let Some(Entry::Vacant { .. }) = self.entries.as_mut().last() {
            self.entries.pop();
        }

        // Removing entries breaks the list of vacant entries,
        // so it must be repaired
        if self.entries.as_ref().len() != len_before {
            // Some vacant entries were removed, so the list now likely¹
            // either contains references to the removed entries, or has an
            // invalid end marker. Fix this by recreating the list.
            self.recreate_vacant_list();
            // ¹: If the removed entries formed the tail of the list, with the
            // most recently popped entry being the head of them, (so that its
            // index is now the end marker) the list is still valid.
            // Checking for that unlikely scenario of this infrequently called
            // is not worth the code complexity.
        }

        self.entries.shrink_to_fit();
    }

    /// Reduce the capacity as much as possible, changing the key for elements when necessary.
    ///
    /// To allow updating references to the elements which must be moved to a new key,
    /// this function takes a closure which is called before moving each element.
    /// The second and third parameters to the closure are the current key and
    /// new key respectively.
    /// In case changing the key for one element turns out not to be possible,
    /// the move can be cancelled by returning `false` from the closure.
    /// In that case no further attempts at relocating elements is made.
    /// If the closure unwinds, the slab will be left in a consistent state,
    /// but the value that the closure panicked on might be removed.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    ///
    /// let mut slab = Slab::with_capacity(10);
    /// let a = slab.insert('a');
    /// slab.insert('b');
    /// slab.insert('c');
    /// slab.remove(a);
    /// slab.compact(|&mut value, from, to| {
    ///     assert_eq!((value, from, to), ('c', 2, 0));
    ///     true
    /// });
    /// assert!(slab.capacity() >= 2 && slab.capacity() < 10);
    /// ```
    ///
    /// The value is not moved when the closure returns `Err`:
    ///
    /// ```
    /// # use generic_slab::*;
    ///
    /// let mut slab = Slab::with_capacity(100);
    /// let a = slab.insert('a');
    /// let b = slab.insert('b');
    /// slab.remove(a);
    /// slab.compact(|&mut value, from, to| false);
    /// assert_eq!(slab.iter().next(), Some((b, &'b')));
    /// ```
    pub fn compact<F>(&mut self, mut rekey: F)
    where
        F: FnMut(&mut T, TKey, TKey) -> bool,
    {
        struct CleanupGuard<'a, T, TKey, TEntries>
        where
            TKey: Key<T>,
            TEntries: Entries<T, TKey>,
        {
            slab: &'a mut GenericSlab<T, TKey, TEntries>,
            decrement: bool,
        }

        impl<T, TKey, TEntries> Drop for CleanupGuard<'_, T, TKey, TEntries>
        where
            TKey: Key<T>,
            TEntries: Entries<T, TKey>,
        {
            fn drop(&mut self) {
                if self.decrement {
                    // Value was popped and not pushed back on
                    self.slab.meta.len -= 1;
                }
                self.slab.recreate_vacant_list();
            }
        }

        let mut occupied_until = 0;
        let mut guard = CleanupGuard {
            slab: self,
            decrement: true,
        };

        // While there are vacant entries
        while guard.slab.entries.as_ref().len() > guard.slab.meta.len {
            // Find a value that needs to be moved,
            // by popping entries until we find an occupied one.
            // (entries cannot be empty because 0 is not greater than anything)
            if let Some(Entry::Occupied {
                mut value,
                key_data,
                #[cfg(feature = "range")]
                range,
            }) = guard.slab.entries.pop()
            {
                // Found one, now find a vacant entry to move it to
                let old_index = guard.slab.entries.as_ref().len();
                let old_key =
                    TKey::new_occupied(&guard.slab.meta.key_context, old_index, &key_data);
                let new_key = loop {
                    match guard.slab.entries.as_ref().get(occupied_until) {
                        Some(Entry::Occupied { .. }) => occupied_until += 1,
                        Some(Entry::Vacant { key_data, .. }) => {
                            break TKey::new_vacant(
                                &guard.slab.meta.key_context,
                                occupied_until,
                                Some(key_data),
                            );
                        }
                        _ => unreachable!(),
                    }
                };

                if !rekey(&mut value, old_key, new_key) {
                    // Changing the key failed, so push the entry back on at its old index.
                    guard.decrement = false;

                    guard.slab.entries.push(Entry::Occupied {
                        value,
                        key_data,
                        #[cfg(feature = "range")]
                        range,
                    });
                    guard.slab.entries.shrink_to_fit();

                    // Guard drop handles cleanup
                    return;
                }

                // Put the value in its new spot
                let entry = &mut guard.slab.entries.as_mut()[occupied_until];
                let key_data = match replace(entry, Entry::Unknown) {
                    Entry::Vacant { key_data, .. } => key_data,
                    _ => unreachable!(),
                };

                *entry = Entry::Occupied {
                    value,
                    key_data: TKey::convert_into_occupied(&guard.slab.meta.key_context, key_data),
                    #[cfg(feature = "range")]
                    range,
                };

                #[cfg(feature = "range")]
                guard
                    .slab
                    .move_occupied_index(old_index, occupied_until, range);

                occupied_until += 1;
            }
        }

        guard.slab.meta.first_vacant = guard.slab.meta.len;
        guard.slab.entries.shrink_to_fit();

        // Normal cleanup is not necessary
        forget(guard);
    }

    /// Return a draining iterator that removes all elements from the slab and
    /// yields the removed items.
    ///
    /// Note: Elements are removed even if the iterator is only partially
    /// consumed or not consumed at all.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    ///
    /// let _ = slab.insert(0);
    /// let _ = slab.insert(1);
    /// let _ = slab.insert(2);
    ///
    /// {
    ///     let mut drain = slab.drain();
    ///
    ///     assert_eq!(Some(0), drain.next());
    ///     assert_eq!(Some(1), drain.next());
    ///     assert_eq!(Some(2), drain.next());
    ///     assert_eq!(None, drain.next());
    /// }
    ///
    /// assert!(slab.is_empty());
    /// ```
    pub fn drain(&mut self) -> Drain<'_, T, TKey> {
        GenericIter::new(
            self.meta.len,
            self.entries.as_mut().iter_mut().enumerate(),
            DrainMapper::new(&mut self.meta),
        )
    }

    fn occupied_key_at(&self, index: usize) -> TKey {
        match &self.entries.as_ref()[index] {
            Entry::Occupied { key_data, .. } => {
                TKey::new_occupied(&self.meta.key_context, index, key_data)
            }
            _ => unreachable!(),
        }
    }

    fn vacant_key_at(&self, index: usize) -> TKey {
        match self.entries.as_ref().get(index) {
            Some(Entry::Vacant { key_data, .. }) => {
                TKey::new_vacant(&self.meta.key_context, index, Some(key_data))
            }
            None => TKey::new_vacant(&self.meta.key_context, index, None),
            _ => unimplemented!(),
        }
    }

    fn get_at(&self, key: &TKey) -> Option<&T> {
        let index = key.index(&self.meta.key_context);
        match self.entries.as_ref().get(index) {
            Some(Entry::Occupied {
                value, key_data, ..
            }) if key.verify(&self.meta.key_context, key_data) => Some(value),
            _ => None,
        }
    }

    fn get_mut_at(&mut self, key: &TKey) -> Option<&mut T> {
        let index = key.index(&self.meta.key_context);
        match self.entries.as_mut().get_mut(index) {
            Some(Entry::Occupied {
                value, key_data, ..
            }) if key.verify(&self.meta.key_context, key_data) => Some(value),
            _ => None,
        }
    }

    fn insert_at(&mut self, index: usize, value: T) {
        self.meta.len += 1;

        #[cfg(feature = "range")]
        let range = self.insert_occupied_index(index);

        if index == self.entries.as_ref().len() {
            let entry = Entry::Occupied {
                value,
                key_data: Default::default(),
                #[cfg(feature = "range")]
                range,
            };

            self.entries.push(entry);
            self.meta.first_vacant = index + 1;
        } else {
            let entry = &mut self.entries.as_mut()[index];

            match replace(entry, Entry::Unknown) {
                Entry::Vacant { next, key_data } => {
                    let key_data = TKey::convert_into_occupied(&self.meta.key_context, key_data);

                    *entry = Entry::Occupied {
                        value,
                        key_data,
                        #[cfg(feature = "range")]
                        range,
                    };

                    self.meta.first_vacant = next;
                }
                _ => unreachable!(),
            }
        }
    }

    fn remove_at(&mut self, index: usize) -> T {
        let entry = &mut self.entries.as_mut()[index];
        match replace(entry, Entry::Unknown) {
            Entry::Occupied {
                value,
                key_data,
                #[cfg(feature = "range")]
                range,
            } => {
                *entry = Entry::Vacant {
                    next: self.meta.first_vacant,
                    key_data: TKey::convert_into_vacant(&self.meta.key_context, key_data),
                };

                #[cfg(feature = "range")]
                self.remove_occupied_index(index, range);

                self.meta.len -= 1;
                self.meta.first_vacant = index;

                value
            }
            _ => unreachable!(),
        }
    }

    /// Iterate through all entries to recreate and repair the vacant list.
    /// self.len must be correct and is not modified.
    pub(crate) fn recreate_vacant_list(&mut self) {
        self.meta.first_vacant = self.entries.as_ref().len();
        // We can stop once we've found all vacant entries
        let mut remaining_vacant = self.entries.as_ref().len() - self.meta.len;
        if remaining_vacant == 0 {
            return;
        }

        // Iterate in reverse order so that lower keys are at the start of
        // the vacant list. This way future shrinks are more likely to be
        // able to remove vacant entries.
        for (index, entry) in self.entries.as_mut().iter_mut().enumerate().rev() {
            if let Entry::Vacant { next, .. } = entry {
                *next = self.meta.first_vacant;

                self.meta.first_vacant = index;

                remaining_vacant -= 1;
                if remaining_vacant == 0 {
                    break;
                }
            }
        }
    }

    #[cfg(feature = "range")]
    pub(crate) fn insert_occupied_index(&mut self, index: usize) -> RangeIndices {
        let mut prev = index;
        let mut next = index;

        if self.meta.last_occupied != crate::INVALID_INDEX {
            prev = self.meta.last_occupied;

            match &mut self.entries.as_mut()[prev] {
                Entry::Occupied { range, .. } => {
                    next = range.next;
                    range.next = index;
                }
                _ => unreachable!(),
            }

            match &mut self.entries.as_mut()[next] {
                Entry::Occupied { range, .. } => {
                    range.prev = index;
                }
                _ => unreachable!(),
            }
        }

        self.meta.last_occupied = index;
        if self.meta.first_occupied == crate::INVALID_INDEX {
            self.meta.first_occupied = index;
        }

        RangeIndices { prev, next }
    }

    #[cfg(feature = "range")]
    pub(crate) fn remove_occupied_index(&mut self, index: usize, range: RangeIndices) {
        let RangeIndices { prev, next } = range;

        if prev != index {
            match &mut self.entries.as_mut()[prev] {
                Entry::Occupied { range, .. } => range.next = next,
                _ => unreachable!(),
            }
        }

        if next != index {
            match &mut self.entries.as_mut()[next] {
                Entry::Occupied { range, .. } => range.prev = prev,
                _ => unreachable!(),
            }
        }

        if self.meta.first_occupied == index {
            self.meta.first_occupied = if next == index {
                crate::INVALID_INDEX
            } else {
                next
            };
        }

        if self.meta.last_occupied == index {
            self.meta.last_occupied = if prev == index {
                crate::INVALID_INDEX
            } else {
                prev
            };
        }
    }

    #[cfg(feature = "range")]
    fn move_occupied_index(&mut self, old_index: usize, new_index: usize, range: RangeIndices) {
        if range.prev != old_index {
            match &mut self.entries.as_mut()[range.prev] {
                Entry::Occupied { range, .. } => {
                    range.next = new_index;
                }
                _ => unimplemented!(),
            }
        }

        if range.next != old_index {
            match &mut self.entries.as_mut()[range.next] {
                Entry::Occupied { range, .. } => {
                    range.prev = new_index;
                }
                _ => unimplemented!(),
            }
        }

        if self.meta.first_occupied == old_index {
            self.meta.first_occupied = new_index;
        }

        if self.meta.last_occupied == old_index {
            self.meta.last_occupied = new_index;
        }
    }

    #[cfg(feature = "range")]
    fn index_range<R>(&self, range: R) -> (Bound<usize>, Bound<usize>)
    where
        R: RangeBounds<TKey>,
    {
        let front = match range.start_bound() {
            Bound::Included(key) if self.get_at(key).is_some() => {
                Bound::Included(key.index(&self.meta.key_context))
            }
            Bound::Excluded(key) if self.get_at(key).is_some() => {
                Bound::Excluded(key.index(&self.meta.key_context))
            }
            _ => Bound::Included(self.meta.first_occupied),
        };

        let back = match range.end_bound() {
            Bound::Included(key) if self.get_at(key).is_some() => {
                Bound::Included(key.index(&self.meta.key_context))
            }
            Bound::Excluded(key) if self.get_at(key).is_some() => {
                Bound::Excluded(key.index(&self.meta.key_context))
            }
            _ => Bound::Included(self.meta.last_occupied),
        };

        (front, back)
    }
}

impl<T, TKey, TEntries> GenericSlab<T, TKey, TEntries>
where
    TKey: Key<T>,
    TEntries: DynamicEntries<T, TKey>,
{
    /// Construct a new, empty `Slab` with the specified capacity.
    ///
    /// The returned slab will be able to store exactly `capacity` without
    /// reallocating. If `capacity` is 0, the slab will not allocate.
    ///
    /// It is important to note that this function does not specify the *length*
    /// of the returned slab, but only the capacity. For an explanation of the
    /// difference between length and capacity, see [Capacity and
    /// reallocation](index.html#capacity-and-reallocation).
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::with_capacity(10);
    ///
    /// // The slab contains no values, even though it has capacity for more
    /// assert_eq!(slab.len(), 0);
    ///
    /// // These are all done without reallocating...
    /// for i in 0..10 {
    ///     slab.insert(i);
    /// }
    ///
    /// // ...but this may make the slab reallocate
    /// slab.insert(11);
    /// ```
    pub fn with_capacity(capacity: usize) -> Self {
        Self::from_entries(TEntries::with_capacity(capacity))
    }

    /// Reserve capacity for at least `additional` more values to be stored
    /// without allocating.
    ///
    /// `reserve` does nothing if the slab already has sufficient capacity for
    /// `additional` more values. If more capacity is required, a new segment of
    /// memory will be allocated and all existing values will be copied into it.
    /// As such, if the slab is already very large, a call to `reserve` can end
    /// up being expensive.
    ///
    /// The slab may reserve more than `additional` extra space in order to
    /// avoid frequent reallocations. Use `reserve_exact` instead to guarantee
    /// that only the requested space is allocated.
    ///
    /// # Panics
    ///
    /// Panics if the new capacity exceeds `isize::MAX` bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    /// slab.insert("hello");
    /// slab.reserve(10);
    /// assert!(slab.capacity() >= 11);
    /// ```
    pub fn reserve(&mut self, additional: usize) {
        if self.capacity() - self.meta.len >= additional {
            return;
        }

        let need_add = additional - (self.entries.as_ref().len() - self.meta.len);

        self.entries.reserve(need_add);
    }

    /// Reserve the minimum capacity required to store exactly `additional`
    /// more values.
    ///
    /// `reserve_exact` does nothing if the slab already has sufficient capacity
    /// for `additional` more values. If more capacity is required, a new segment
    /// of memory will be allocated and all existing values will be copied into
    /// it.  As such, if the slab is already very large, a call to `reserve` can
    /// end up being expensive.
    ///
    /// Note that the allocator may give the slab more space than it requests.
    /// Therefore capacity can not be relied upon to be precisely minimal.
    /// Prefer `reserve` if future insertions are expected.
    ///
    /// # Panics
    ///
    /// Panics if the new capacity exceeds `isize::MAX` bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// # use generic_slab::*;
    /// let mut slab = Slab::new();
    /// slab.insert("hello");
    /// slab.reserve_exact(10);
    /// assert!(slab.capacity() >= 11);
    /// ```
    pub fn reserve_exact(&mut self, additional: usize) {
        if self.capacity() - self.meta.len >= additional {
            return;
        }

        let need_add = additional - (self.entries.as_ref().len() - self.meta.len);

        self.entries.reserve_exact(need_add);
    }
}

impl<T, TKey, TEntries> Default for GenericSlab<T, TKey, TEntries>
where
    TKey: Key<T>,
    TEntries: Entries<T, TKey> + Default,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T, TKey, TEntries> Debug for GenericSlab<T, TKey, TEntries>
where
    T: Debug,
    TKey: Key<T> + Debug,
    TEntries: Entries<T, TKey>,
{
    fn fmt(&self, fmt: &mut Formatter<'_>) -> FmtResult {
        if fmt.alternate() {
            fmt.debug_map().entries(self.iter()).finish()
        } else {
            fmt.debug_struct("GenericSlab")
                .field("len", &self.meta.len)
                .field("cap", &self.capacity())
                .finish()
        }
    }
}

impl<T, TKey, TEntries> Clone for GenericSlab<T, TKey, TEntries>
where
    TKey: Key<T>,
    TEntries: Clone,
    Meta<T, TKey>: Clone,
{
    fn clone(&self) -> Self {
        Self {
            meta: self.meta.clone(),
            entries: self.entries.clone(),
        }
    }

    fn clone_from(&mut self, source: &Self) {
        self.meta.clone_from(&source.meta);
        self.entries.clone_from(&source.entries);
    }
}

impl<T, TKey, TEntries> Index<TKey> for GenericSlab<T, TKey, TEntries>
where
    TKey: Key<T>,
    TEntries: Entries<T, TKey>,
{
    type Output = T;

    #[cfg_attr(not(slab_no_track_caller), track_caller)]
    fn index(&self, key: TKey) -> &T {
        self.get(key).expect("invalid key")
    }
}

impl<T, TKey, TEntries> IndexMut<TKey> for GenericSlab<T, TKey, TEntries>
where
    TKey: Key<T>,
    TEntries: Entries<T, TKey>,
{
    #[cfg_attr(not(slab_no_track_caller), track_caller)]
    fn index_mut(&mut self, key: TKey) -> &mut T {
        self.get_mut(key).expect("invalid key")
    }
}

impl<T, TKey, TEntries> IntoIterator for GenericSlab<T, TKey, TEntries>
where
    TKey: Key<T>,
    TEntries: IntoIterator<Item = Entry<T, TKey>>,
{
    type Item = (TKey, T);
    type IntoIter = IntoIter<T, TKey, TEntries>;

    fn into_iter(self) -> Self::IntoIter {
        GenericIter::new(
            self.meta.len,
            self.entries.into_iter().enumerate(),
            KeyValueMapper::new(self.meta),
        )
    }
}

impl<'a, T, TKey, TEntries> IntoIterator for &'a GenericSlab<T, TKey, TEntries>
where
    TKey: Key<T>,
    TEntries: Entries<T, TKey>,
{
    type Item = (TKey, &'a T);
    type IntoIter = Iter<'a, T, TKey>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T, TKey, TEntries> IntoIterator for &'a mut GenericSlab<T, TKey, TEntries>
where
    TKey: Key<T>,
    TEntries: Entries<T, TKey>,
{
    type Item = (TKey, &'a mut T);
    type IntoIter = IterMut<'a, T, TKey>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<T, TKey, TEntries> FromIterator<T> for GenericSlab<T, TKey, TEntries>
where
    TKey: Key<T>,
    TEntries: Entries<T, TKey> + Default,
{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut slab = Self::default();

        for value in iter {
            slab.insert(value);
        }

        slab
    }
}

impl<T, TKey, TEntries> FromIterator<(TKey, T)> for GenericSlab<T, TKey, TEntries>
where
    TKey: Key<T>,
    TEntries: Entries<T, TKey> + Default,
{
    fn from_iter<I: IntoIterator<Item = (TKey, T)>>(iter: I) -> Self {
        let mut builder = Builder::new();

        for (key, value) in iter {
            builder.pair(key, value);
        }

        builder.build()
    }
}

/* Meta */

impl<T, TKey> Clone for Meta<T, TKey>
where
    TKey: Key<T>,
    TKey::Context: Clone,
{
    fn clone(&self) -> Self {
        Self {
            len: self.len,
            key_context: self.key_context.clone(),
            first_vacant: self.first_vacant,
            #[cfg(feature = "range")]
            first_occupied: self.first_occupied,
            #[cfg(feature = "range")]
            last_occupied: self.last_occupied,
        }
    }
}

/* GenericVacantEntry */

impl<'a, T, TKey, TEntries> GenericVacantEntry<'a, T, TKey, TEntries>
where
    TKey: Key<T>,
    TEntries: Entries<T, TKey>,
{
    /// Get the key of this vacant entry.
    pub fn key(&self) -> TKey {
        self.slab.vacant_key_at(self.index)
    }

    /// Insert a new item to the slab using the key defined by [`key`](Self::key).
    pub fn insert(self, value: T) -> &'a mut T {
        self.slab.insert_at(self.index, value);

        match self.slab.entries.as_mut().get_mut(self.index) {
            Some(Entry::Occupied { value, .. }) => value,
            _ => unreachable!(),
        }
    }
}

impl<'a, T, TKey, TEntries> Debug for GenericVacantEntry<'a, T, TKey, TEntries>
where
    TKey: Key<T>,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("GenericVacantEntry")
            .field("index", &self.index)
            .finish()
    }
}
