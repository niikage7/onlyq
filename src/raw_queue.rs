use std::{
    alloc::{self, Layout},
    mem::MaybeUninit,
    ptr::{self, NonNull},
    slice,
};

/// Just a buffer-and-capacity holder, responsible for
/// allocating/deallocating memory and pointer manipulations (like `RawVec` for `Vec` does).
///
/// This structure does **NOT** provide any circular functionality, just a _contiguous_ buffer.
pub struct RawQueue<T> {
    pub(crate) buf: NonNull<MaybeUninit<T>>,
    pub(crate) cap: usize,
}

////////////////////////////////////////////////////////////////////////////////
// RawQueue<T> impl
////////////////////////////////////////////////////////////////////////////////

impl<T> RawQueue<T> {
    /// Creates a new [`RawQueue`] with empty buffer, which can hold `cap` elements of type `MaybeUninit<T>`.
    ///
    /// # Safety
    /// `T` must not be a zero-sized type when `cap > 0` - otherwise a
    /// zero-size layout is passed to the allocator, which is UB.
    ///
    /// # Panics
    /// * The allocator reports failure (returns a null pointer).
    /// * `cap` produces a layout whose size would be `>= isize::MAX` (see
    ///   [`layout`](RawQueue::layout)).
    pub(crate) unsafe fn new(cap: usize) -> Self {
        // SAFETY: Safety contract for this function is the same as for alloc_buf
        let buf = unsafe { Self::alloc_buf(cap) };
        Self { buf, cap }
    }

    /// Creates a new [`RawQueue`] from the given pointer and capacity.
    ///
    /// # Safety
    /// * `ptr` must not be null.
    /// * `ptr` must point to a live allocation whose layout matches
    ///   `Layout::array::<T>(cap)` (see [`layout`](RawQueue::layout)) -
    ///   `Drop` will later deallocate using that layout, so a mismatch is
    ///   undefined behavior.
    /// * `T` must not be a zero-sized type when `cap > 0` - otherwise a
    ///   zero-size layout is passed to the allocator, which is UB.
    pub(crate) unsafe fn new_from_ptr_and_cap(ptr: *mut T, cap: usize) -> Self {
        Self {
            buf: unsafe { NonNull::new_unchecked(ptr.cast()) },
            cap,
        }
    }

    ////////////////////////////////////////////////////////////////////////////////
    // Reads & writes
    ////////////////////////////////////////////////////////////////////////////////

    /// Returns a reference to the `MaybeUninit<T>` at physical slot `idx`,
    /// without bounds-checking.
    ///
    /// # Safety
    /// `idx` must be `< self.cap` (within the allocation), and the slot at
    /// `idx` must hold an initialized item.
    #[inline]
    pub(crate) unsafe fn read_ref(&self, idx: usize) -> &MaybeUninit<T> {
        unsafe { &*self.buf.as_ptr().add(idx) }
    }

    /// Returns a slice over the first `len` elements of the buffer.
    ///
    /// # Safety
    /// `len` must be `<= self.cap` (within the allocation), and every slot
    /// in `0..len` must hold an initialized item.
    #[inline]
    pub(crate) unsafe fn as_slice(&self, len: usize) -> &[MaybeUninit<T>] {
        unsafe { slice::from_raw_parts(self.buf.as_ptr(), len) }
    }

    /// Returns a raw pointer to the start of the buffer, cast to `*mut T`.
    ///
    /// # Safety
    /// The returned pointer is valid for `self.cap` elements, but only the
    /// first `len` (tracked by the caller, not `RawQueue`) are
    /// initialized. The caller must not read through this pointer beyond
    /// that initialized prefix.
    #[inline]
    pub(crate) unsafe fn as_ptr(&self) -> *mut T {
        self.buf.as_ptr().cast::<T>()
    }

    /// Writes `item` into physical slot `idx`, overwriting whatever was
    /// there without reading or dropping it.
    ///
    /// # Safety
    /// `idx` must be `< self.cap` (within the allocation). This does
    /// **NOT** drop the slot's previous contents - calling it on a slot
    /// that already holds an initialized `T` leaks that value instead of
    /// dropping it. Use [`replace`](Self::replace) instead if the previous
    /// value needs to be recovered or dropped.
    #[inline]
    pub(crate) unsafe fn write(&mut self, idx: usize, item: MaybeUninit<T>) {
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
    pub(crate) unsafe fn replace(&mut self, idx: usize, item: MaybeUninit<T>) -> MaybeUninit<T> {
        unsafe { ptr::replace(self.buf.as_ptr().add(idx), item) }
    }

    ////////////////////////////////////////////////////////////////////////////////
    // Memory stuff
    ////////////////////////////////////////////////////////////////////////////////

    /// Allocates a new [`NonNull`] buffer able to hold `cap` elements of `MaybeUninit<T>`.
    ///
    /// If `cap == 0`, returns [`NonNull::dangling`] without allocating anything.
    /// Otherwise, allocates (but does not initialize) storage for `cap` elements of `MaybeUninit<T>` and
    /// returns a pointer to the start of that storage.
    /// Allocated memory will be freed when `drop` method is called on `RawQueue`, but it will **NOT**
    /// drop any elements - just deallocate memory.
    ///
    /// The caller is responsible for **initializing** any elements **before reading**
    /// them.
    ///
    /// # Safety
    /// `T` must not be a zero-sized type when `cap > 0` - otherwise a
    /// zero-size layout is passed to the allocator, which is UB.
    ///
    /// # Panics
    /// * The allocator reports failure (returns a null pointer).
    /// * `cap` produces a layout whose size would be `>= isize::MAX` (see
    ///   [`layout`](RawQueue::layout)).
    pub(crate) unsafe fn alloc_buf(cap: usize) -> NonNull<MaybeUninit<T>> {
        if cap == 0 {
            return NonNull::dangling();
        }

        let layout = Self::layout(cap);

        // SAFETY: if we created layout without error, so calling alloc must be fine.
        // We can allocate MaybeUninit<T> instead of T, because it is guaranteed that
        // MaybeUninit<T> has the same layout as T
        let ptr = unsafe { alloc::alloc(layout) } as *mut MaybeUninit<T>;
        if ptr.is_null() {
            alloc::handle_alloc_error(layout);
        }

        // SAFETY: we checked that ptr is not null
        unsafe { NonNull::new_unchecked(ptr) }
    }

    /// Creates a new array layout for `cap` elements
    ///
    /// # Panics
    /// If size of allocated layout greater or equal to isize::MAX
    #[inline]
    fn layout(cap: usize) -> Layout {
        Layout::array::<T>(cap).expect("size of layout must be less than isize::MAX")
    }
}

////////////////////////////////////////////////////////////////////////////////
// Trait impls
////////////////////////////////////////////////////////////////////////////////

impl<T> Drop for RawQueue<T> {
    fn drop(&mut self) {
        unsafe {
            alloc::dealloc(self.buf.as_ptr() as *mut u8, Self::layout(self.cap));
        }
    }
}
