#![no_std]
#![cfg_attr(feature = "strict_provenance", feature(strict_provenance))]

extern crate alloc;

use alloc::boxed::Box;
use core::{
    cell::UnsafeCell,
    mem::{self, forget, MaybeUninit},
    ops::{Deref, DerefMut},
    ptr::addr_of,
    sync::atomic::{AtomicU64, Ordering},
};

use once_cell::race::OnceBox;

const IDX: usize = (1 << 6) - 1;
const IDX_MASK: usize = !IDX;

/// An indexed arena designed to allow slots to be converted into/from
/// raw-pointers
#[repr(align(64))]
pub struct Arena64<T> {
    occupancy: AtomicU64,
    slots: [UnsafeCell<MaybeUninit<T>>; 64],
    next: OnceBox<Arena64<T>>,
}

impl<T> Default for Arena64<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Arena64<T> {
    /// Create with an initial capacity of 64
    pub const fn new() -> Self {
        let slots = unsafe { MaybeUninit::uninit().assume_init() };

        Arena64 {
            occupancy: AtomicU64::new(0),
            slots,
            next: OnceBox::new(),
        }
    }

    /// Inserts value into an unoccupied [`Slot`], allocating as necessary in
    /// increments of 64.
    pub fn insert(&self, value: T) -> Slot<'_, T> {
        let mut occupancy = self.occupancy.load(Ordering::Acquire);

        let idx = loop {
            // Isolate lowest clear bit. See https://docs.rs/bitintr/latest/bitintr/trait.Blcic.html
            let least_significant_bit = !occupancy & (occupancy.wrapping_add(1));

            if least_significant_bit.ne(&0) {
                occupancy = self
                    .occupancy
                    .fetch_or(least_significant_bit, Ordering::AcqRel);

                if (occupancy & least_significant_bit).eq(&0) {
                    break least_significant_bit.trailing_zeros();
                }
            } else {
                return self
                    .next
                    .get_or_init(|| unsafe { Box::new_uninit().assume_init() })
                    .insert(value);
            }
        };

        unsafe {
            *self.slots[idx as usize].get() = MaybeUninit::new(value);
        }

        Slot {
            arena: self,
            idx: idx as usize,
        }
    }
}

unsafe impl<T> Send for Arena64<T> where T: Send {}
unsafe impl<T> Sync for Arena64<T> where T: Sync {}

/// Provides exclusive access over an index in [`Arena64`] until dropped
pub struct Slot<'a, T> {
    arena: &'a Arena64<T>,
    idx: usize,
}

impl<T> Slot<'_, T> {
    pub fn take(self) -> T {
        let value = unsafe {
            mem::replace(
                &mut *self.arena.slots[self.idx].get(),
                MaybeUninit::uninit(),
            )
            .assume_init()
        };

        self.arena
            .occupancy
            .fetch_and(!(1 << self.idx), Ordering::Release);

        forget(self);

        value
    }

    /// Reconstruct `Slot` from a tagged pointer
    #[cfg(not(feature = "strict_provenance"))]
    pub unsafe fn from_raw(ptr: *mut ()) -> Self {
        Self {
            arena: &*((ptr as usize & IDX_MASK) as *const _),
            idx: ptr as usize & IDX,
        }
    }

    /// Reconstruct `Slot` from a tagged pointer
    #[cfg(feature = "strict_provenance")]
    pub unsafe fn from_raw(ptr: *mut ()) -> Self {
        Self {
            arena: &*(ptr.map_addr(|addr| addr & IDX_MASK) as *const _),
            idx: ptr as usize & IDX,
        }
    }

    /// Consumes `Slot`, converting into a raw pointer that points to the
    /// underlying arena with the index as the tag
    #[cfg(not(feature = "strict_provenance"))]
    pub fn into_raw(self) -> *mut () {
        ((addr_of!(*self.arena) as usize) | self.idx) as *mut ()
    }

    /// Consumes `Slot`, converting into a raw pointer that points to the
    /// underlying arena with the index as the tag
    #[cfg(feature = "strict_provenance")]
    pub fn into_raw(self) -> *mut () {
        addr_of!(*self.arena).map_addr(|addr| addr | self.idx) as *mut ()
    }
}

unsafe impl<T> Send for Slot<'_, T> where T: Send {}
unsafe impl<T> Sync for Slot<'_, T> where T: Sync {}

impl<T> Deref for Slot<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { (*self.arena.slots[self.idx].get()).assume_init_ref() }
    }
}

impl<T> DerefMut for Slot<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { (*self.arena.slots[self.idx].get()).assume_init_mut() }
    }
}

impl<T> Drop for Slot<'_, T> {
    fn drop(&mut self) {
        unsafe { (*self.arena.slots[self.idx].get()).assume_init_drop() }
        self.arena
            .occupancy
            .fetch_and(!(1 << self.idx), Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use crate::{Arena64, Slot};

    #[test]
    fn it_grows() {
        let arena = Arena64::new();

        let slots: Vec<Slot<u32>> = (0..512).map(|i| arena.insert(i)).collect();

        let values: Vec<u32> = slots.into_iter().map(|slot| slot.take()).collect();

        assert_eq!(values, (0..512).collect::<Vec<u32>>())
    }
}
