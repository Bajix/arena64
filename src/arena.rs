use alloc::boxed::Box;
use core::{
    ptr::{self},
    sync::atomic::{AtomicPtr, Ordering},
    u64,
};

use crossbeam_utils::atomic::AtomicConsume;

use crate::boxed::Inner;
pub use crate::boxed::Slot;
/// An indexed arena designed to allow slots to be converted into/from
/// raw pointers
pub struct Arena64<T> {
    inner: AtomicPtr<Inner<T>>,
}

impl<T> Default for Arena64<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Arena64<T> {
    /// Create with an initial capacity of 64
    pub const fn new() -> Self {
        Arena64 {
            inner: AtomicPtr::new(ptr::null_mut()),
        }
    }

    fn replace_inner(&self, current: *mut Inner<T>) -> *mut Inner<T> {
        let inner: Box<Inner<T>> = unsafe { Box::new_uninit().assume_init() };
        let inner = Box::into_raw(inner);

        match self
            .inner
            .compare_exchange(current, inner, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(previous) => {
                if !previous.is_null() {
                    // Flipping every bit lets slots know to deallocate on the last dropped
                    unsafe { &*previous }
                        .occupancy
                        .fetch_xor(u64::MAX, Ordering::Release);
                }

                inner
            }
            Err(current) => {
                unsafe {
                    drop(Box::from_raw(inner));
                }

                current
            }
        }
    }

    /// Inserts value into an unoccupied [`Slot`], allocating as necessary in
    /// increments of 64.
    pub fn insert(&self, value: T) -> Slot<T> {
        let mut inner = self.inner.load_consume();

        loop {
            if !inner.is_null() {
                if let Some(slot) = unsafe { &*inner }.get_uninit_slot() {
                    return slot.insert(value);
                }
            }

            inner = self.replace_inner(inner);
        }
    }
}

unsafe impl<T> Send for Arena64<T> where T: Send {}
unsafe impl<T> Sync for Arena64<T> where T: Sync {}

impl<T> Drop for Arena64<T> {
    fn drop(&mut self) {
        let inner = self.inner.load_consume();

        if !inner.is_null() {
            unsafe {
                drop(Box::from_raw(inner));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use crate::arena::{Arena64, Slot};

    #[test]
    fn arena64_capacity_grows() {
        let arena = Arena64::new();

        let slots: Vec<Slot<u32>> = (0..512).map(|i| arena.insert(i)).collect();

        let values: Vec<u32> = slots.into_iter().map(|slot| slot.take()).collect();

        assert_eq!(values, (0..512).collect::<Vec<u32>>())
    }
}
