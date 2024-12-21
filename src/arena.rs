use alloc::boxed::Box;
use core::{
    cell::UnsafeCell,
    fmt::Debug,
    mem::{self, forget, ManuallyDrop, MaybeUninit},
    ops::{Deref, DerefMut},
    ptr::addr_of,
    sync::atomic::{AtomicU64, Ordering},
};

use once_cell::race::OnceBox;

use crate::{IDX, IDX_MASK};

/// An indexed arena designed to allow slots to be converted into/from
/// raw pointers
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

    /// Reconstruct [`Slot`] from a tagged pointer to become the borrow-owner of
    /// an [`Arena64`] cell until dropped
    ///
    /// # Safety
    ///
    /// It must be guaranteed that the underlying [`Arena64`] be valid and at
    /// the same address for the lifetime of [`Slot`]
    pub unsafe fn from_raw(ptr: *mut ()) -> Self {
        Self {
            arena: &*(ptr.map_addr(|addr| addr & IDX_MASK) as *const _),
            idx: ptr as usize & IDX,
        }
    }

    /// Consumes [`Slot`], converting into a raw pointer that points to the
    /// underlying [`Arena64`] with the index as the tag
    ///
    /// # Safety
    ///
    /// For drop to be called on the interior value the raw pointer must be
    /// converted back into [`Slot`] prior to [`Arena64`] being dropped
    pub fn into_raw(self) -> *mut () {
        let slot = ManuallyDrop::new(self);
        addr_of!(*slot.arena).map_addr(|addr| addr | slot.idx) as *mut ()
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
        unsafe {
            (*self.arena.slots[self.idx].get()).assume_init_drop();
        }

        self.arena
            .occupancy
            .fetch_and(!(1 << self.idx), Ordering::Release);
    }
}

impl<T> PartialEq<T> for Slot<'_, T>
where
    T: PartialEq<T>,
{
    fn eq(&self, other: &T) -> bool {
        self.deref().eq(other)
    }
}

impl<T> PartialEq<Slot<'_, T>> for Slot<'_, T>
where
    T: PartialEq<T>,
{
    fn eq(&self, other: &Slot<T>) -> bool {
        self.deref().eq(other)
    }
}

impl<T> Eq for Slot<'_, T> where T: PartialEq<T> {}

impl<T> Debug for Slot<'_, T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.deref().fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;
    use core::sync::atomic::Ordering;

    use crate::arena::{Arena64, Slot};

    #[test]
    fn arena64_capacity_grows() {
        let arena = Arena64::new();

        let slots: Vec<Slot<u32>> = (0..512).map(|i| arena.insert(i)).collect();

        let values: Vec<u32> = slots.into_iter().map(|slot| slot.take()).collect();

        assert_eq!(values, (0..512).collect::<Vec<u32>>())
    }

    #[test]
    fn arena64_converts_into_and_from_raw_pointer() {
        let arena: Arena64<usize> = Arena64::new();

        let slots: Vec<Slot<usize>> = (0..64).map(|i| arena.insert(i)).collect();

        assert_eq!(slots.len(), 64);

        let pointers: Vec<*mut ()> = slots.into_iter().map(|slot| slot.into_raw()).collect();

        let slots: Vec<Slot<usize>> = pointers
            .into_iter()
            .map(|ptr| unsafe { Slot::from_raw(ptr) })
            .collect();

        assert_eq!(arena.occupancy.load(Ordering::Acquire), u64::MAX);
        assert_eq!(slots, (0..64).collect::<Vec<usize>>());

        drop(slots);

        assert_eq!(arena.occupancy.load(Ordering::Acquire), 0);
    }
}
