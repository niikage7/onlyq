use std::{
    alloc::{self, Layout},
    mem,
    ptr::{self, NonNull},
};

////////////////////////////////////////////////////////////////////////////////
// Memory utils
////////////////////////////////////////////////////////////////////////////////

/// Creates a new array layout for `cap` elements
///
/// # Panics
/// If size of allocated layout greater or equal to isize::MAX
#[inline]
pub fn queue_layout<T>(cap: usize) -> Layout {
    Layout::array::<T>(cap).expect("size of layout must be less than isize::MAX")
}

/// Allocates a new [`NonNull`] pointer able to hold `cap` elements of `T`.
///
/// If `cap == 0`, returns [`NonNull::dangling`] without allocating anything.
/// Otherwise, allocates (but does not initialize) storage for `cap`
/// elements of `T` and returns a pointer to the start of that storage.
/// The caller is responsible for initializing any elements before reading
/// them, and for eventually freeing the allocation (e.g. via
/// [`dealloc_nonnull`]) with the same `cap` and `T`.
///
/// # Safety
/// `T` must not be a zero-sized type when `cap > 0` - otherwise a
/// zero-size layout is passed to the allocator, which is UB.
///
/// # Panics
/// * The allocator reports failure (returns a null pointer).
/// * `cap` produces a layout whose size would be `>= isize::MAX` (see
///   [`queue_layout`]).
pub unsafe fn alloc_nonnull<T>(cap: usize) -> NonNull<T> {
    if cap == 0 {
        return NonNull::dangling();
    }

    let layout = queue_layout::<T>(cap);

    // SAFETY: if we created layout without error, so calling alloc must be fine
    let ptr = unsafe { alloc::alloc(layout) } as *mut T;
    if ptr.is_null() {
        alloc::handle_alloc_error(layout);
    }

    // SAFETY: we checked that ptr is not null
    unsafe { NonNull::new_unchecked(ptr) }
}

/// # Safety
/// * `ptr` must have layout of `cap` elements (because [`alloc::dealloc`] will be called on `ptr`)
/// * `ptr` should have `len` filled elements if `T` needs to be dropped
///     (because [`ptr::drop_in_place`] will be called on `ptr` on each elements in range 0..`len`)
pub unsafe fn dealloc_nonnull<T>(ptr: NonNull<T>, len: usize, cap: usize) {
    if mem::needs_drop::<T>() {
        for i in 0..len {
            unsafe { ptr::drop_in_place(ptr.as_ptr().add(i)) };
        }
    }

    unsafe {
        alloc::dealloc(ptr.as_ptr() as *mut u8, queue_layout::<T>(cap));
    }
}
