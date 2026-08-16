//! `onlyq` is a small crate providing [`OnlyQueue`], a fixed-capacity,
//! heap-allocated cyclic queue.
//!
//! Once the queue is full, pushing a new element overwrites the oldest
//! element currently stored (in insertion order) rather than growing the
//! buffer - the queue's capacity is fixed at construction and is never
//! resized.
//!
//! # Promises
//!
//! ### 1. Items are stored in a _Cyclic_ buffer
//! All items are stored in a structure called a _Cyclic (or Ring) buffer_, which allows the oldest values to be replaced with the newest ones
//! (see [`OnlyQueue::push`]).
//!
//! ### 2. Buffer has a _Fixed_ capacity
//! The buffer will **NEVER** allocate more memory than it needs to store the given number of elements.
//! When the user tries to push an element beyond capacity, the new element will replace the oldest one, according to Promise #1 - so the buffer never needs to be reallocated.
//!
//! ### 3. Order of elements is _Unchangeable_
//! There is no way to remove a single item from the buffer or reorder its elements - the only way to remove anything is [`OnlyQueue::clear`], which empties the buffer entirely.
//!
//! # Getting started
//!
//! ```
//! use onlyq::OnlyQueue;
//!
//! let mut queue = OnlyQueue::<i32>::new(3);
//! queue.push(1);
//! queue.push(2);
//! queue.push(3);
//!
//! // The queue is now full; pushing another element overwrites the oldest one.
//! let evicted = queue.push(4);
//! assert_eq!(evicted, Some(1));
//! assert_eq!(queue.get_all(), &[4, 2, 3]);
//! ```
//!
//! See [`OnlyQueue`] for the full API and its safety invariants.

mod errors;
mod utils;

pub use crate::errors::OnlyqError;
use crate::utils::{alloc_nonnull, dealloc_nonnull};
use std::{
    fmt::{Debug, Display},
    mem::{self, MaybeUninit},
    ops::Deref,
    ptr::{self, NonNull},
    slice,
};

/// A fixed-capacity, heap-allocated cyclic queue.
///
/// Once the queue reaches capacity, pushing a new element overwrites the
/// oldest one currently stored, in the order elements were originally
/// pushed - i.e. eviction always targets the longest-resident element,
/// not any arbitrary slot.
///
/// [`get`](Self::get) and [`get_all`](Self::get_all) index by **physical
/// slot**, not by insertion order - they make no chronological promise.
/// After the buffer has wrapped at least once, slot `0` is not guaranteed
/// to hold the oldest (or any particular) element.
///
/// # Examples
///
/// Filling the queue and then overwriting the oldest element:
/// ```
/// use onlyq::OnlyQueue;
///
/// let mut queue = OnlyQueue::<i32>::new(3);
/// assert_eq!(queue.push(1), None);
/// assert_eq!(queue.push(2), None);
/// assert_eq!(queue.push(3), None);
///
/// // The queue is full now: the next push overwrites the oldest element (1)
/// // and returns it.
/// assert_eq!(queue.push(4), Some(1));
/// assert_eq!(queue.get_all(), &[4, 2, 3]);
/// ```
///
/// Building a queue from an existing `Vec` or slice:
/// ```
/// use onlyq::OnlyQueue;
///
/// // Takes ownership of the Vec's allocation - no cloning.
/// let from_vec = OnlyQueue::try_from(vec![1, 2, 3]).unwrap();
/// assert_eq!(from_vec.get_all(), &[1, 2, 3]);
///
/// // Clones each element into a freshly allocated buffer.
/// let from_slice = OnlyQueue::try_from([1, 2, 3].as_slice()).unwrap();
/// assert_eq!(from_slice.get_all(), &[1, 2, 3]);
/// ```
///
/// Reading elements back:
/// ```
/// use onlyq::OnlyQueue;
///
/// let mut queue = OnlyQueue::<&str>::new(2);
/// queue.push("a");
/// queue.push("b");
///
/// assert_eq!(queue.get(0), Some(&"a"));
/// assert_eq!(queue.get(5), None); // out of bounds
/// assert_eq!(queue.len(), 2);
/// ```
///
/// # Invariant
///
/// Slots `0..self.len` are always initialized; slots `self.len..self.cap`
/// are always uninitialized. This is established in [`new`](Self::new)
/// and every method that touches `len` (`push`, `clear`, `Drop`) is
/// responsible for leaving it intact before returning.
#[derive(Debug)]
pub struct OnlyQueue<T> {
    /// Backing allocation of `cap` slots. Slots `0..len` are always
    /// initialized; the rest are logically uninitialized
    buf: NonNull<MaybeUninit<T>>,
    /// Number of initialized slots. Grows from 0 up to `cap` while the
    /// buffer fills for the first time, then stays at `cap` forever
    /// (elements are replaced in place, not appended) until `clear()`
    /// resets it to 0
    len: usize,
    /// Maximum amount of elements
    cap: usize,
    /// Physical index of the oldest element - the slot that will be
    /// overwritten next once the buffer is full. Unused (stays 0) while
    /// the buffer is still filling for the first time, since writes
    /// during that phase go to index `len` instead
    idx: usize,
}

////////////////////////////////////////////////////////////////////////////////
// OnlyQueue<T> impl
////////////////////////////////////////////////////////////////////////////////

impl<T> OnlyQueue<T> {
    /// Creates a new, empty queue with the given capacity.
    ///
    /// # Panics
    /// * `T` is a zero-sized type
    /// * `cap` is `0`
    pub fn new(cap: usize) -> Self {
        // TODO: zero-sized types support
        assert_ne!(
            mem::size_of::<T>(),
            0,
            "zero-sized types are not supported yet"
        );
        assert_ne!(
            cap, 0,
            "there is no point in creating non-resizable queue with 0 capacity"
        );

        // SAFETY: we checked that cap != 0 and T is not ZST
        unsafe { Self::new_unchecked(cap) }
    }

    /// Attempts to create a new empty queue with the given capacity.
    ///
    /// Unlike [`new`](OnlyQueue::new), this will not panic and will return appropriate errors.
    ///
    /// # Errors
    /// * [`ZeroSizedType`](OnlyqError::ZeroSizedType) - `T` is zero-sized type (not supported yet)
    /// * [`ZeroCapacity`](OnlyqError::ZeroCapacity) - `cap` is `0`
    pub fn try_new(cap: usize) -> Result<Self, OnlyqError> {
        // TODO: zero-sized types support
        if mem::size_of::<T>() == 0 {
            return Err(OnlyqError::ZeroSizedType);
        }
        if cap == 0 {
            return Err(OnlyqError::ZeroCapacity);
        }

        // SAFETY: we checked that cap != 0 and T is not ZST
        Ok(unsafe { Self::new_unchecked(cap) })
    }

    /// Creates a new, empty queue with the given capacity, without
    /// validating `cap` or `T`. Prefer [`new`](Self::new) unless you have
    /// already checked both preconditions yourself.
    ///
    /// # Safety
    /// * `cap` must not be equal to zero, because there is no point in
    ///   creating a non-resizable queue with zero elements.
    /// * `T` must not be zero-sized - this is not currently supported and
    ///   is left unchecked by this function.
    pub unsafe fn new_unchecked(cap: usize) -> Self {
        // SAFETY: Caller guarantees T is not a ZST.
        // We can allocate MaybeUninit<T> instead of T, because it is guaranteed that
        // MaybeUninit<T> has the same layout as T
        let buf = unsafe { alloc_nonnull::<MaybeUninit<T>>(cap) };

        Self {
            buf,
            len: 0,
            cap,
            idx: 0,
        }
    }

    /// Returns the number of elements currently stored in the queue.
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Checks if a queue is empty
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the queue's fixed capacity, set at construction and never
    /// changed afterwards.
    pub const fn cap(&self) -> usize {
        self.cap
    }

    /// Returns the physical slot index of the oldest element - the slot
    /// that the next eviction (once the queue is full) will overwrite.
    pub const fn idx(&self) -> usize {
        self.idx
    }

    ////////////////////////////////////////////////////////////////////////////////
    // Read operations
    ////////////////////////////////////////////////////////////////////////////////

    /// Returns a reference to the element at physical slot `idx`, or
    /// `None` if `idx >= len()`.
    ///
    /// Indexes by physical slot, not insertion order - see the
    /// struct-level docs.
    pub fn get(&self, idx: usize) -> Option<&T> {
        // valid indices are 0..self.len; idx >= self.len is out of bounds
        if idx >= self.len {
            return None;
        }

        // SAFETY: we checked that idx < self.len, and by invariant every slot
        // in 0..self.len holds an initialized T.
        Some(unsafe { self.read_ref_unchecked(idx).assume_init_ref() })
    }

    /// Returns a reference to the `MaybeUninit<T>` at physical slot `idx`,
    /// without bounds-checking.
    ///
    /// # Safety
    /// `idx` must be `< self.len` - by the struct's invariant, every such
    /// slot holds an initialized `T`.
    #[inline]
    unsafe fn read_ref_unchecked(&self, idx: usize) -> &MaybeUninit<T> {
        unsafe { &*self.buf.as_ptr().add(idx) }
    }

    /// Returns every currently stored element as a slice, in physical
    /// slot order (`0..len`) rather than insertion order.
    pub fn get_all(&self) -> &[T] {
        // SAFETY: self.len is, by invariant, the number of initialized slots,
        // so reinterpreting the first self.len `MaybeUninit<T>` slots as `T`
        // is sound. `transmute` is valid because MaybeUninit<T> and T share layout.
        unsafe {
            let items = slice::from_raw_parts(self.buf.as_ptr(), self.len);
            mem::transmute::<&[MaybeUninit<T>], &[T]>(items)
        }
    }

    ////////////////////////////////////////////////////////////////////////////////
    // Write operations
    ////////////////////////////////////////////////////////////////////////////////

    /// Pushes `item` onto the queue.
    ///
    /// While the queue has free capacity, this appends the element and
    /// returns `None`. Once full, it overwrites the oldest element (in
    /// insertion order) and returns it as `Some(old)`.
    ///
    /// # Example
    /// ```
    /// use onlyq::OnlyQueue;
    ///
    /// let mut q = OnlyQueue::<i32>::new(2);
    ///
    /// let first = q.push(1);
    /// assert!(first.is_none()); // because `1` takes free slot
    ///
    /// let second = q.push(2);
    /// assert!(second.is_none());
    ///
    /// let third = q.push(3);
    ///
    /// // We're trying to add a third element to a queue that can hold only 2,
    /// // so the first one is overwritten by the newly added (third) element.
    /// assert_eq!(third, Some(1))
    /// ```
    pub fn push(&mut self, item: T) -> Option<T> {
        if self.len < self.cap {
            // Buffer isn't full yet: append at the next free physical slot.

            // SAFETY: we checked that self.len < self.cap, so slot self.len
            // is within the allocation and not yet initialized.
            unsafe {
                self.write_unchecked(self.len, MaybeUninit::new(item));
            }

            self.len += 1;
            return None;
        }

        // Buffer is full (self.len == self.cap): overwrite the oldest element.
        let target_idx = self.idx % self.cap;

        // SAFETY: this branch only runs once self.len == self.cap, so every
        // slot in 0..self.cap is initialized; target_idx is always < self.cap
        // by construction, so it names an initialized slot.
        let old = unsafe {
            self.replace_unchecked(target_idx, MaybeUninit::new(item))
                .assume_init()
        };

        self.idx = (target_idx + 1) % self.cap;
        Some(old)
    }

    /// Writes `item` into physical slot `idx`, overwriting whatever was
    /// there without reading or dropping it.
    ///
    /// # Safety
    /// `idx` must be `< self.cap` (within the allocation). Unlike
    /// [`replace_unchecked`](Self::replace_unchecked), this does **not**
    /// drop or return the slot's previous contents - calling it on a slot
    /// that already holds an initialized `T` leaks that value instead of
    /// dropping it. Only call this on a slot the invariant guarantees is
    /// currently uninitialized (i.e. `idx >= self.len`).
    #[inline]
    unsafe fn write_unchecked(&mut self, idx: usize, item: MaybeUninit<T>) {
        unsafe { ptr::write(self.buf.as_ptr().add(idx), item) }
    }

    /// Writes `item` into physical slot `idx` and returns whatever was
    /// previously stored there, without dropping it.
    ///
    /// # Safety
    /// `idx` must be `< self.cap` (within the allocation). This function
    /// is sound regardless of whether the slot was previously
    /// initialized - but the *caller* must only call `assume_init` on the
    /// returned value if `idx` did in fact name a previously-initialized
    /// slot; doing so otherwise is undefined behavior.
    #[inline]
    unsafe fn replace_unchecked(&mut self, idx: usize, item: MaybeUninit<T>) -> MaybeUninit<T> {
        unsafe { ptr::replace(self.buf.as_ptr().add(idx), item) }
    }

    /// Removes and drops every element in the queue, resetting it to the
    /// same empty state as a freshly constructed queue. Capacity is
    /// unchanged.
    pub fn clear(&mut self) {
        for i in 0..self.len {
            // SAFETY: i is in 0..self.len, and by invariant every such slot
            // holds an initialized T, so it's sound to drop it and then mark
            // the slot uninitialized. Resetting self.len to 0 below keeps the
            // invariant "slots 0..self.len are initialized" intact afterwards.
            unsafe {
                let ptr = self.buf.as_ptr().add(i);
                if mem::needs_drop::<T>() {
                    ptr::drop_in_place(ptr);
                }
                ptr.write(MaybeUninit::uninit());
            }
        }

        self.idx = 0;
        self.len = 0;
    }
}

////////////////////////////////////////////////////////////////////////////////
// Trait impls
////////////////////////////////////////////////////////////////////////////////

impl<T> Deref for OnlyQueue<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.get_all()
    }
}

/// Formats as the debug representation of [`get_all`](Self::get_all) -
/// i.e. the elements in physical slot order, not insertion order.
impl<T: Debug> Display for OnlyQueue<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.get_all())
    }
}

/// Drops every remaining element and deallocates the queue's backing
/// buffer.
impl<T> Drop for OnlyQueue<T> {
    fn drop(&mut self) {
        // SAFETY: self.buf was allocated for self.cap elements of T (see
        // [`OnlyQueue::new`]), and exactly the first self.len slots are guaranteed
        // initialized
        unsafe {
            dealloc_nonnull(self.buf.cast::<T>(), self.len, self.cap);
        }
    }
}

/// Builds a queue directly from an existing `Vec<T>`, taking ownership of
/// its allocation (the queue's capacity becomes `value.capacity()`)
/// rather than cloning elements.
///
/// # Errors
/// Returns [`OnlyqError::ZeroSizedType`] if `T` is a zero-sized type, or
/// [`OnlyqError::ZeroCapacity`] if `value.capacity()` is `0`. On error,
/// `value` is dropped normally.
impl<T> TryFrom<Vec<T>> for OnlyQueue<T> {
    type Error = OnlyqError;

    fn try_from(value: Vec<T>) -> Result<Self, Self::Error> {
        if mem::size_of::<T>() == 0 {
            return Err(OnlyqError::ZeroSizedType);
        }

        let cap = value.capacity();
        if cap == 0 {
            return Err(OnlyqError::ZeroCapacity);
        }

        let len = value.len();
        let ptr = value.as_ptr().cast_mut();

        // Vec must NOT call its own destructor
        mem::forget(value);

        Ok(Self {
            // SAFETY: vec ptr is never null
            buf: unsafe { NonNull::new_unchecked(ptr.cast::<MaybeUninit<T>>()) },
            len,
            cap,
            idx: 0,
        })
    }
}

/// Builds a queue by cloning every element of `value` into a freshly
/// allocated buffer (capacity becomes `value.len()`).
///
/// # Errors
/// Returns [`OnlyqError::ZeroSizedType`] if `T` is a zero-sized type, or
/// [`OnlyqError::ZeroCapacity`] if `value` is empty.
impl<T: Clone> TryFrom<&[T]> for OnlyQueue<T> {
    type Error = OnlyqError;

    fn try_from(value: &[T]) -> Result<Self, Self::Error> {
        let cap = value.len();

        let mut queue = Self::try_new(cap)?;
        for item in value {
            // len < cap on each iteration, so operation will be _append_, not _replace_
            queue.push(item.clone());
        }

        Ok(queue)
    }
}
////////////////////////////////////////////////////////////////////////////////
// Tests
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod onlyq_tests {
    use crate::{OnlyQueue, errors::OnlyqError};

    ////////////////////////////////////////////////////////////////////////////////
    // Push tests
    ////////////////////////////////////////////////////////////////////////////////

    ////////////////////////////////////////////////////////////////////////////////
    // Construction tests
    ////////////////////////////////////////////////////////////////////////////////

    /// Tests cases when `T` is ZST (this operation must always return [`OnlyqError::ZeroSizedType`])
    #[test]
    fn create_zero_sized_type() {
        // Constructed by user
        let q = OnlyQueue::<()>::try_new(1);
        assert!(q.is_err());
        assert_eq!(q.unwrap_err(), OnlyqError::ZeroSizedType);

        // Constructed with try_from (supported: vec, slice)
        let v = Vec::<()>::with_capacity(3);
        let q = OnlyQueue::try_from(v);
        assert!(q.is_err());
        assert_eq!(q.unwrap_err(), OnlyqError::ZeroSizedType);

        let s = [(), (), ()].as_slice();
        let q = OnlyQueue::try_from(s);
        assert!(q.is_err());
        assert_eq!(q.unwrap_err(), OnlyqError::ZeroSizedType);
    }

    /// Tests cases when `cap` == 0 (this operation must always return [`OnlyqError::ZeroCapacity`])
    #[test]
    fn create_zero_cap() {
        // Constructed by user
        let q = OnlyQueue::<i32>::try_new(0);
        assert!(q.is_err());
        assert_eq!(q.unwrap_err(), OnlyqError::ZeroCapacity);

        // Constructed with try_from (supported: vec, slice)
        let v = Vec::<i32>::with_capacity(0);
        let q = OnlyQueue::try_from(v);
        assert!(q.is_err());
        assert_eq!(q.unwrap_err(), OnlyqError::ZeroCapacity);

        let s: &[i32] = [].as_slice();
        let q = OnlyQueue::try_from(s);
        assert!(q.is_err());
        assert_eq!(q.unwrap_err(), OnlyqError::ZeroCapacity);
    }

    /// Tests cases when user pushes items to empty queue.
    ///
    /// It must append items, because `len` < `cap` and there is no point in
    /// overwriting any elements.
    #[test]
    fn push_append() {
        let mut q = OnlyQueue::<i32>::new(3);
        let f = q.push(1);
        assert!(f.is_none());

        let s = q.push(2);
        assert!(s.is_none());

        let t = q.push(3);
        assert!(t.is_none());

        assert_eq!(q.get_all(), &[1, 2, 3]);
    }

    /// Tests cases when user pushes items to full queue.
    ///
    /// It must append the first 3 items, because `len` < `cap`, then overwrite
    /// the first (`idx = 0`) item with the fourth one.
    #[test]
    fn push_replace() {
        let mut q = OnlyQueue::<i32>::new(3);
        let f = q.push(1);
        assert!(f.is_none());

        let s = q.push(2);
        assert!(s.is_none());

        let t = q.push(3);
        assert!(t.is_none());

        // Replaces element at index 0 and returns replaced item
        let f = q.push(4);
        assert!(f.is_some());
        assert_eq!(f.unwrap(), 1);

        assert_eq!(q.get_all(), &[4, 2, 3]);
    }

    ////////////////////////////////////////////////////////////////////////////////
    // Get tests
    ////////////////////////////////////////////////////////////////////////////////

    /// Tests cases when user tries to get item within the bounds of queue
    #[test]
    fn get_in_bounds() {
        let mut q = OnlyQueue::<i32>::new(3);
        q.push(1);
        q.push(2);
        q.push(3);

        let i = q.get(1);
        assert!(i.is_some());
        assert_eq!(i.unwrap(), &2);
    }

    /// Tests cases when user tries to get item out of the bounds of queue
    #[test]
    fn get_out_of_bounds() {
        let mut q = OnlyQueue::<i32>::new(3);
        q.push(1);
        q.push(2);
        q.push(3);

        let i = q.get(3);
        assert!(i.is_none());
    }

    ////////////////////////////////////////////////////////////////////////////////
    // Clear
    ////////////////////////////////////////////////////////////////////////////////

    /// Tests case when user clears the queue
    #[test]
    fn clear() {
        let mut q = OnlyQueue::<i32>::new(3);
        q.push(1);
        q.push(2);
        q.push(3);

        q.clear();
        assert_eq!(q.len(), 0);
        assert_eq!(q.cap(), 3);
        assert_eq!(q.idx(), 0);
    }
}
