use core::{
    cell::UnsafeCell,
    fmt::Debug,
    mem::{self, forget, ManuallyDrop, MaybeUninit},
    ops::{Deref, DerefMut},
    ptr::addr_of,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{IDX, IDX_MASK};

/// A slab with 64 pre-allocated slots
#[repr(align(64))]
pub struct Fixed64<T> {
    occupancy: AtomicU64,
    slots: [UnsafeCell<MaybeUninit<T>>; 64],
}

impl<T> Default for Fixed64<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Fixed64<T> {
    /// Create with a fixed capacity of 64
    pub const fn new() -> Self {
        let slots = unsafe { MaybeUninit::uninit().assume_init() };

        Fixed64 {
            occupancy: AtomicU64::new(0),
            slots,
        }
    }

    /// Get an unoccupied [`UninitSlot`] if available
    pub fn get_uninit_slot(&self) -> Option<UninitSlot<'_, T>> {
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
                return None;
            }
        };

        Some(UninitSlot {
            slab: self,
            idx: idx as usize,
        })
    }
}

unsafe impl<T> Send for Fixed64<T> where T: Send {}
unsafe impl<T> Sync for Fixed64<T> where T: Sync {}

/// Provides exclusive access over an unitialized index of [`Fixed64`] until
/// dropped
pub struct UninitSlot<'a, T> {
    slab: &'a Fixed64<T>,
    idx: usize,
}

impl<'a, T> UninitSlot<'a, T> {
    /// Initialize slot with value
    pub fn insert(self, value: T) -> Slot<'a, T> {
        unsafe {
            *self.slab.slots[self.idx].get() = MaybeUninit::new(value);
        }

        unsafe { mem::transmute(self) }
    }
}

unsafe impl<T> Send for UninitSlot<'_, T> where T: Send {}
unsafe impl<T> Sync for UninitSlot<'_, T> where T: Sync {}

impl<T> Drop for UninitSlot<'_, T> {
    fn drop(&mut self) {
        self.slab
            .occupancy
            .fetch_and(!(1 << self.idx), Ordering::Release);
    }
}

/// Provides exclusive access over an index of [`Fixed64`] until dropped
pub struct Slot<'a, T> {
    slab: &'a Fixed64<T>,
    idx: usize,
}

impl<T> Slot<'_, T> {
    pub fn take(self) -> T {
        let value = unsafe {
            mem::replace(&mut *self.slab.slots[self.idx].get(), MaybeUninit::uninit()).assume_init()
        };

        self.slab
            .occupancy
            .fetch_and(!(1 << self.idx), Ordering::Release);

        forget(self);

        value
    }

    /// Reconstruct [`Slot`] from a tagged pointer to become the borrow-owner of
    /// a [`Fixed64`] cell until dropped
    ///
    /// # Safety
    ///
    /// It must be guaranteed that the underlying [`Fixed64`] be valid and at
    /// the same address for the lifetime of [`Slot`].
    pub unsafe fn from_raw(ptr: *mut ()) -> Self {
        Self {
            slab: &*(ptr.map_addr(|addr| addr & IDX_MASK) as *const _),
            idx: ptr as usize & IDX,
        }
    }

    /// Consumes [`Slot`], converting into a raw pointer that points to the
    /// underlying [`Fixed64`] with the index as the tag (low bits)
    ///
    /// # Safety
    ///
    /// For drop to be called on the interior value this must be converted back
    /// into [`Slot`] prior to [`Fixed64`] being dropped
    pub fn into_raw(self) -> *mut () {
        let slot = ManuallyDrop::new(self);

        addr_of!(*slot.slab).map_addr(|addr| addr | slot.idx) as *mut ()
    }
}

unsafe impl<T> Send for Slot<'_, T> where T: Send {}
unsafe impl<T> Sync for Slot<'_, T> where T: Sync {}

impl<T> Deref for Slot<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { (*self.slab.slots[self.idx].get()).assume_init_ref() }
    }
}

impl<T> DerefMut for Slot<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { (*self.slab.slots[self.idx].get()).assume_init_mut() }
    }
}

impl<T> Drop for Slot<'_, T> {
    fn drop(&mut self) {
        unsafe { (*self.slab.slots[self.idx].get()).assume_init_drop() }
        self.slab
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

    use super::{Fixed64, Slot};
    use crate::heapless::UninitSlot;

    #[test]
    fn fixed64_allocs_64() {
        let slab = Fixed64::new();

        let slots: Vec<UninitSlot<usize>> =
            (0..64).filter_map(|_| slab.get_uninit_slot()).collect();

        assert_eq!(slots.len(), 64);
        assert!(slab.get_uninit_slot().is_none());

        let slots: Vec<Slot<usize>> = slots
            .into_iter()
            .enumerate()
            .map(|(i, slot)| slot.insert(i))
            .collect();

        assert_eq!(slots, (0..64).collect::<Vec<usize>>());
    }

    #[test]
    fn fixed64_converts_into_and_from_raw_pointer() {
        let slab = Fixed64::new();

        let slots: Vec<UninitSlot<usize>> =
            (0..64).filter_map(|_| slab.get_uninit_slot()).collect();

        assert_eq!(slots.len(), 64);
        assert!(slab.get_uninit_slot().is_none());

        let slots: Vec<Slot<usize>> = slots
            .into_iter()
            .enumerate()
            .map(|(i, slot)| slot.insert(i))
            .collect();

        let pointers: Vec<*mut ()> = slots.into_iter().map(|slot| slot.into_raw()).collect();

        let slots: Vec<Slot<usize>> = pointers
            .into_iter()
            .map(|ptr| unsafe { Slot::from_raw(ptr) })
            .collect();

        assert_eq!(slab.occupancy.load(Ordering::Acquire), u64::MAX);
        assert_eq!(slots, (0..64).collect::<Vec<usize>>());

        drop(slots);

        assert_eq!(slab.occupancy.load(Ordering::Acquire), 0);
    }
}
