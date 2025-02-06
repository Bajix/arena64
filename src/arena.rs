use alloc::boxed::Box;
use core::{
    mem::MaybeUninit,
    ptr::{self},
    sync::atomic::{AtomicPtr, Ordering},
};

use crossbeam_utils::atomic::AtomicConsume;

use crate::boxed::{Inner, Slot};
/// A concurrent arena
pub struct Arena64<T> {
    inner: AtomicPtr<Inner<T>>,
}

impl<T> Default for Arena64<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Arena64<T> {
    pub const fn new() -> Self {
        Arena64 {
            inner: AtomicPtr::new(ptr::null_mut()),
        }
    }

    #[inline]
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

    /// Allocate value into an unoccupied [`Slot`]
    pub fn alloc(&self, value: T) -> Slot<T> {
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

/// A bump allocator
pub struct Bump64<T> {
    occupancy: u64,
    inner: *mut Inner<T>,
}

impl<T> Default for Bump64<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Bump64<T> {
    pub const fn new() -> Self {
        Bump64 {
            occupancy: 0,
            inner: ptr::null_mut(),
        }
    }

    /// Allocate value into the next [`Slot`]
    pub fn alloc(&mut self, value: T) -> Slot<T> {
        loop {
            if !self.inner.is_null() {
                let least_significant_bit = !self.occupancy & self.occupancy.wrapping_add(1);

                if least_significant_bit.ne(&0) {
                    self.occupancy |= least_significant_bit;

                    let idx = least_significant_bit.trailing_zeros() as usize;

                    unsafe {
                        *(*self.inner).slots[idx].get() = MaybeUninit::new(value);
                    }

                    return Slot {
                        slab: self.inner,
                        idx,
                    };
                }
            }

            self.inner = Box::into_raw(unsafe { Box::new_uninit().assume_init() });
            self.occupancy = 0;
        }
    }
}

unsafe impl<T> Send for Bump64<T> where T: Send {}
unsafe impl<T> Sync for Bump64<T> where T: Sync {}

impl<T> Drop for Bump64<T> {
    fn drop(&mut self) {
        if !self.inner.is_null() && self.occupancy.ne(&u64::MAX) {
            // These bits were never assigned to
            let unoccupied_bits = self.occupancy ^ u64::MAX;

            // Because bits weren't set when occupying, [`Slot`] dropping results in indexes
            // being set
            let released = unsafe { &*self.inner }
                .occupancy
                .fetch_xor(unoccupied_bits, Ordering::AcqRel);

            // If every bit has already been set, then every [`Slot`] has dropped
            if released.eq(&self.occupancy) {
                unsafe {
                    drop(Box::from_raw(self.inner));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use crate::arena::{Arena64, Bump64, Slot};

    #[test]
    fn arena64_capacity_grows() {
        let arena = Arena64::new();

        let slots: Vec<Slot<u32>> = (0..4096).map(|i| arena.alloc(i)).collect();

        assert_eq!(slots, (0..4096).collect::<Vec<u32>>())
    }

    #[test]
    fn bump64_capacity_grows() {
        let mut arena = Bump64::new();

        let slots: Vec<Slot<u32>> = (0..4096).map(|i| arena.alloc(i)).collect();

        assert_eq!(slots, (0..4096).collect::<Vec<u32>>())
    }
}
