#![no_std]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_variables))
))]

//! `generic_slab` is a fork of [`slab`](https://github.com/tokio-rs/slab) that
//! keeps the familiar `Slab<T>` API but provides more control over the key and
//! storage types. Using a generic approach you can for example implement strong
//! typed keys that must fit the data type stored in the slab, or you can use a
//! fixed size array as backing storage for the slab data.
//!
//! <a href="https://github.com/Bergmann89/generic_slab/blob/master/LICENSE"><img src="https://img.shields.io/crates/l/generic_slab" alt="Crates.io License"></a>
//! <a href="https://crates.io/crates/generic_slab"><img src="https://img.shields.io/crates/v/generic_slab" alt="Crates.io Version"></a>
//! <a href="https://crates.io/crates/generic_slab"><img src="https://img.shields.io/crates/d/generic_slab" alt="Crates.io Total Downloads"></a>
//! <a href="https://docs.rs/generic_slab"><img src="https://img.shields.io/docsrs/generic_slab" alt="docs.rs"></a>
//! <a href="https://github.com/Bergmann89/generic_slab/actions/workflows/ci.yml"><img src="https://github.com/Bergmann89/generic_slab/actions/workflows/ci.yml/badge.svg" alt="Github CI"></a>
//! <a href="https://deps.rs/repo/github/Bergmann89/generic_slab"><img src="https://deps.rs/repo/github/Bergmann89/generic_slab/status.svg" alt="Dependency Status"></a>
//!
//! At its core is [`GenericSlab<T, TKey, TEntries>`], which lets you customize
//! - the **key type** (`TKey: Key<T>`) and
//! - the **backing storage** (`TEntries: Entries<T, TKey>`).
//!
//! This enables things like:
//! - strong, type-safe handles that can’t be mixed across element types
//! - fixed-capacity slabs backed by arrays or `ArrayVec`
//! - alternative storage layouts, as long as they implement [`Entries`]
//!
//! ## Differences from the original `slab` crate
//!
//! Compared to [`slab`](https://crates.io/crates/slab):
//! - `generic_slab::Slab<T>` is a **drop-in replacement** for `slab::Slab<T>`:
//!   it uses `usize` keys and a `Vec<Entry<_, _>>` behind the scenes.
//! - You can use [`GenericSlab<T, TKey, TEntries>`] directly to plug in a
//!   custom key type (anything that implements [`Key<T>`], e.g. [`Handle<T>`]).
//! - You can plug in a custom storage type for entries by implementing
//!   [`Entries<T, TKey>`] for your own collection type.
//! - The crate provides [`StrongSlab<T>`], which uses [`Handle<T>`] for keys
//!   and protects you from accidentally mixing keys from different slabs or
//!   different element types.
//! * `generic_slab` is `no_std`-friendly (it works without `std` if you
//!   disable the default `std` feature).
//!
//! If you just need the behavior of the original `slab`, use [`Slab<T>`].
//! If you need more control, use [`GenericSlab<T, TKey, TEntries>`] or
//! [`StrongSlab<T>`].
//!
//! ## Basic usage (drop-in for `slab`)
//!
//! ```rust
//! use generic_slab::Slab;
//!
//! let mut slab = Slab::new();
//!
//! let hello = slab.insert("hello");
//! let world = slab.insert("world");
//!
//! assert_eq!(slab[hello], "hello");
//! assert_eq!(slab[world], "world");
//!
//! slab[world] = "earth";
//! assert_eq!(slab[world], "earth");
//! ```
//!
//! ## Custom Entries Storage (with fixed capacity)
//!
//! This example shows how to replace the default `Vec<Entry<_, _>>` with a
//! fixed-capacity `ArrayVec`-based storage.
//!
//! ```rust
//! use arrayvec::ArrayVec;
//! use generic_slab::{GenericSlab, Entries, Entry, Key};
//!
//! #[derive(Default)]
//! struct FixedEntries<T, K, const N: usize>(ArrayVec<Entry<T, K>, N>)
//! where
//!     K: Key<T>;
//!
//! impl<T, K, const N: usize> AsRef<[Entry<T, K>]> for FixedEntries<T, K, N>
//! where
//!     K: Key<T>,
//! {
//!     fn as_ref(&self) -> &[Entry<T, K>] {
//!         &self.0
//!     }
//! }
//!
//! impl<T, K, const N: usize> AsMut<[Entry<T, K>]> for FixedEntries<T, K, N>
//! where
//!     K: Key<T>,
//! {
//!     fn as_mut(&mut self) -> &mut [Entry<T, K>] {
//!         &mut self.0
//!     }
//! }
//!
//! impl<T, K, const N: usize> Entries<T, K> for FixedEntries<T, K, N>
//! where
//!     K: Key<T>,
//! {
//!     fn capacity(&self) -> usize {
//!         N
//!     }
//!
//!     fn push(&mut self, entry: Entry<T, K>) {
//!         self.0.push(entry);
//!     }
//!
//!     fn pop(&mut self) -> Option<Entry<T, K>> {
//!         self.0.pop()
//!     }
//!
//!     fn clear(&mut self) {
//!         self.0.clear();
//!     }
//! }
//!
//! /// A slab with a fixed maximum of `N` entries, using `usize` keys.
//! type FixedSlab<T, const N: usize> = GenericSlab<T, usize, FixedEntries<T, usize, N>>;
//!
//! // Use it like a normal slab, but it will never reallocate:
//! let mut slab: FixedSlab<&str, 4> = GenericSlab::new();
//! let k0 = slab.insert("zero");
//! let k1 = slab.insert("one");
//! let k2 = slab.insert("two");
//! let k3 = slab.insert("three");
//!
//! assert_eq!(slab[k0], "zero");
//! assert_eq!(slab[k3], "three");
//!
//! // Inserting a 5th element would panic here because the ArrayVec is full.
//! ```
//!
//! ## Example: Custom Key Type using [`Handle`]
//!
//! This example uses [`StrongSlab<T>`], which is a [`GenericSlab`] configured
//! with [`Handle<T>`] as key type. `Handle<T>` is strongly typed and carries
//! enough metadata to detect stale or cross-slab keys.
//! ```rust
//! use generic_slab::StrongSlab;
//!
//! // Keys are `Handle<String>` instead of `usize`.
//! let mut slab: StrongSlab<String> = StrongSlab::default();
//!
//! let a = slab.insert("hello".into());
//! let b = slab.insert("world".into());
//!
//! assert_eq!(slab[a], "hello");
//! assert_eq!(slab[b], "world");
//!
//! // Handles are `Copy` and cheap to pass around.
//! let a2 = a;
//! assert_eq!(slab[a2], "hello");
//! ```
//!
//! [`StrongSlab<T>`] and [`Handle<T>`] prevent mixing keys of different element
//! types:
//!
//! ```rust,ignore
//! use generic_slab::StrongSlab;
//!
//! let mut slab: StrongSlab<String> = StrongSlab::default();
//! let a = slab.insert("hello".into());
//!
//! let mut ints: StrongSlab<i32> = StrongSlab::default();
//! ints[a]; // <- does not compile: `Handle<String>` is not a key for `StrongSlab<i32>`.
//! ```
//!
//! It also prevents reusing old handles for new data that was inserted at the
//! same place:
//!
//! ```rust
//! use generic_slab::StrongSlab;
//!
//! let mut slab: StrongSlab<String> = StrongSlab::default();
//! let handle0 = slab.insert("hello".into());
//! slab.remove(handle0);
//!
//! let handle1 = slab.insert("world".into()); // Will be placed at the same position than the old `handle0`.
//! assert!(slab.get(handle0).is_none()); // But you cant get the new entry from the old handle.
//! assert!(slab.get(handle1).is_some());
//! ```
//!
//! For even more control, you can implement [`Key<T>`] for your own key type
//! and plug it into [`GenericSlab<T, TKey, TEntries>`] together with a custom
//! [`Entries<T, TKey>`] implementation.
//!
//! ## Range iterators (feature `range`)
//!
//! When the optional `range` feature is enabled, slabs gain efficient
//! range iterators that only visit occupied entries and respect the
//! insertion order.
//!
//! The following APIs are available on [`GenericSlab`] (and therefore on
//! [`Slab`] and [`StrongSlab`]) when `feature = "range"` is enabled:
//! - [`GenericSlab::range`] -> [`RangeIter`] over `(&T)`
//! - [`GenericSlab::range_mut`] -> [`RangeIterMut`] over `(&mut T)`
//!
//! Both methods accept any `R: RangeBounds<TKey>` (e.g. `key_a..key_b`,
//! `..key_b`, `key_a..`, `..=` etc.). Only valid, occupied keys in the
//! specified range are yielded. If all bounds are invalid, the iterator
//! is empty.
//!
//! ### Iteration order and “reverse” ranges
//!
//! Range iterators always walk the slab in insertion order, starting
//! from the first element that falls into the range and then wrapping
//! around until the upper bound is reached.
//!
//! A “reverse” range like `key_b..key_a` behaves as a *negated* range:
//! it yields all occupied entries that are not in `key_a..key_b`,
//! still in insertion order.
//!
//! ### Example: simple range
//!
//! ```rust
//! # #[cfg(feature = "range")]
//! # {
//! use generic_slab::Slab;
//!
//! let mut slab = Slab::new();
//!
//! let k0 = slab.insert(0);
//! let k1 = slab.insert(1);
//! let k2 = slab.insert(2);
//! let k3 = slab.insert(3);
//!
//! let mut it = slab.range(k1..k3);
//!
//! assert_eq!(it.next(), Some((k1, &1)));
//! assert_eq!(it.next(), Some((k2, &2)));
//! assert_eq!(it.next(), None);
//! # }
//! ```
//!
//! ### Example: “negated” range using reversed bounds
//!
//! ```rust
//! # #[cfg(feature = "range")]
//! # {
//! use generic_slab::Slab;
//!
//! let mut slab = Slab::new();
//!
//! let k0 = slab.insert(0);
//! let k1 = slab.insert(1);
//! let k2 = slab.insert(2);
//! let k3 = slab.insert(3);
//!
//! // `k3..k1` yields everything that is *not* in `k1..k3`
//! let mut it = slab.range(k3..k1);
//!
//! assert_eq!(it.next(), Some((k3, &3))); // after k3, wrap to start
//! assert_eq!(it.next(), Some((k0, &0)));
//! assert_eq!(it.next(), None);
//! # }
//! ```
//!
//! These iterators are thin wrappers around [`RangeIter`] and
//! [`RangeIterMut`], which you can also name explicitly if you need
//! to store them in a concrete type.
//!
//! # Capacity and reallocation
//!
//! The capacity of a slab is the amount of space allocated for any future
//! values that will be inserted in the slab. This is not to be confused with
//! the *length* of the slab, which specifies the number of actual values
//! currently being inserted. If a slab's length is equal to its capacity, the
//! next value inserted into the slab will require growing (if the underlying
//! buffer supports it).
//!
//! # Implementation
//!
//! `GenericSlab` is backed by a generic storage of slots. Each slot is either
//! occupied or vacant. `GenericSlab` maintains a stack of vacant slots using a
//! linked list. To find a vacant slot, the stack is popped. When a slot is
//! released, it is pushed onto the stack.

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;
#[cfg(feature = "std")]
extern crate std as alloc;

#[cfg(feature = "serde")]
mod serde;

/// Module that contains the generic base implementation of [`Slab`]
pub mod generic;

mod builder;
mod entries;
mod handle;
mod iter;
mod key;

#[cfg(feature = "range")]
mod range_iter;

use alloc::vec::Vec;

pub use crate::entries::{Entries, Entry};
pub use crate::generic::GenericSlab;
pub use crate::handle::Handle;
pub use crate::key::Key;

use crate::generic::GenericVacantEntry;

/// Pre-allocated storage for a uniform data type
///
/// See the [module documentation] for more details.
///
/// [module documentation]: index.html
pub type Slab<T> = GenericSlab<T, usize, Vec<Entry<T, usize>>>;

/// Pre-allocated storage for a uniform data type with a [`Handle`] as strong key.
///
/// See the [module documentation] for more details.
///
/// [module documentation]: index.html
///
/// # Examples
///
/// ```
/// # use generic_slab::*;
/// let mut slab = StrongSlab::default();
///
/// let hello = slab.insert("hello");
/// let world = slab.insert("world");
///
/// assert_eq!(slab[hello], "hello");
/// assert_eq!(slab[world], "world");
///
/// slab[world] = "earth";
/// assert_eq!(slab[world], "earth");
/// ```
pub type StrongSlab<T> = GenericSlab<T, Handle<T>, Vec<Entry<T, Handle<T>>>>;

/// A handle to a vacant entry in a `Slab`.
///
/// `VacantEntry` allows constructing values with the key that they will be
/// assigned to.
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
pub type VacantEntry<'a, T> = GenericVacantEntry<'a, T, usize, Vec<Entry<T, usize>>>;

/// A consuming iterator over the values stored in a `Slab`
pub type IntoIter<T> = self::generic::IntoIter<T, usize, Vec<Entry<T, usize>>>;

/// An iterator over the values stored in the `Slab`
pub type Iter<'a, T> = self::generic::Iter<'a, T, usize>;

/// A mutable iterator over the values stored in the `Slab`
pub type IterMut<'a, T> = self::generic::IterMut<'a, T, usize>;

/// A iterator over a specific range of values stored in the `Slab`
#[cfg(feature = "range")]
pub type RangeIter<'a, T> = self::generic::RangeIter<'a, T, usize, Vec<Entry<T, usize>>>;

/// A mutable iterator over a specific range of values stored in the `Slab`
#[cfg(feature = "range")]
pub type RangeIterMut<'a, T> = self::generic::RangeIterMut<'a, T, usize, Vec<Entry<T, usize>>>;

#[cfg(feature = "range")]
pub(crate) const INVALID_INDEX: usize = core::usize::MAX;
