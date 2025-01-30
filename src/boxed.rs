use alloc::boxed::Box;
use core::{
    borrow::{Borrow, BorrowMut},
    cell::UnsafeCell,
    fmt::{self},
    future::Future,
    hash::{Hash, Hasher},
    mem::{self, forget, ManuallyDrop, MaybeUninit},
    ops::{Deref, DerefMut},
    pin::Pin,
    ptr::addr_of,
    sync::atomic::{AtomicU64, Ordering},
    task::{Context, Poll},
};

use crate::{IDX, IDX_MASK};

#[repr(align(64))]
pub(crate) struct Inner<T> {
    pub(crate) occupancy: AtomicU64,
    pub(crate) slots: [UnsafeCell<MaybeUninit<T>>; 64],
}

impl<T> Inner<T> {
    /// Get an unoccupied [`UninitSlot`] if available
    pub(crate) fn get_uninit_slot(&self) -> Option<UninitSlot<T>> {
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
            slab: addr_of!(*self),
            idx: idx as usize,
        })
    }
}

/// A slab with 64 pre-allocated slots. The underlying heap allocation won't
/// deallocate until all slots have dropped
#[repr(align(64))]
pub struct Boxed64<T> {
    inner: *mut Inner<T>,
}

impl<T> Default for Boxed64<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Boxed64<T> {
    /// Create with a fixed capacity of 64
    pub fn new() -> Self {
        let inner: Box<Inner<T>> = unsafe { Box::new_uninit().assume_init() };
        let inner = Box::into_raw(inner);

        Boxed64 { inner }
    }

    fn inner(&self) -> &Inner<T> {
        unsafe { &*self.inner }
    }

    /// Get an unoccupied [`UninitSlot`] if available
    pub fn get_uninit_slot(&self) -> Option<UninitSlot<T>> {
        self.inner().get_uninit_slot()
    }
}

unsafe impl<T> Send for Boxed64<T> where T: Send {}
unsafe impl<T> Sync for Boxed64<T> where T: Sync {}

impl<T> Drop for Boxed64<T> {
    fn drop(&mut self) {
        // Flipping every bit lets slots know to deallocate on the last dropped
        let occupancy = self.inner().occupancy.fetch_xor(u64::MAX, Ordering::AcqRel);

        if occupancy.eq(&0) {
            unsafe {
                drop(Box::from_raw(self.inner));
            }
        }
    }
}

/// Provides exclusive access over an unitialized index of [`Boxed64`] until
/// dropped
pub struct UninitSlot<T> {
    slab: *const Inner<T>,
    idx: usize,
}

impl<T> UninitSlot<T> {
    fn inner(&self) -> &Inner<T> {
        unsafe { &*self.slab }
    }

    /// Initialize slot with value
    pub fn insert(self, value: T) -> Slot<T> {
        unsafe {
            *self.inner().slots[self.idx].get() = MaybeUninit::new(value);
        }

        unsafe { mem::transmute(self) }
    }
}

unsafe impl<T> Send for UninitSlot<T> where T: Send {}
unsafe impl<T> Sync for UninitSlot<T> where T: Sync {}

impl<T> Drop for UninitSlot<T> {
    fn drop(&mut self) {
        let occupancy = self
            .inner()
            .occupancy
            .fetch_xor(1 << self.idx, Ordering::AcqRel);

        // If this was the last slot after Boxed64 was previously dropped, then the
        // underlying heap allocation needs to be dropped
        if occupancy.eq(&!(1 << self.idx)) {
            unsafe {
                drop(Box::from_raw(self.slab as *mut Inner<T>));
            }
        }
    }
}

/// Provides exclusive access over an index of [`Boxed64`] until dropped
pub struct Slot<T> {
    pub(crate) slab: *const Inner<T>,
    pub(crate) idx: usize,
}

impl<T> Slot<T> {
    fn inner(&self) -> &Inner<T> {
        unsafe { &*self.slab }
    }

    pub fn take(self) -> T {
        let value = unsafe {
            mem::replace(
                &mut *self.inner().slots[self.idx].get(),
                MaybeUninit::uninit(),
            )
            .assume_init()
        };

        let occupancy = self
            .inner()
            .occupancy
            .fetch_xor(1 << self.idx, Ordering::AcqRel);

        // If this was the last slot after Boxed64 was previously dropped, then the
        // underlying heap allocation needs to be dropped
        if occupancy.eq(&!(1 << self.idx)) {
            unsafe {
                drop(Box::from_raw(self.slab as *mut Inner<T>));
            }
        }

        forget(self);

        value
    }

    /// Reconstruct [`Slot`] from a tagged pointer to become the borrow-owner of
    /// a [`Boxed64`] cell until dropped
    ///
    /// # Safety
    ///
    /// This pointer must have been created by [`Slot::into_raw`] and logically
    /// passes ownership; [`Slot`] becomes the borrow-owner of the cell
    pub unsafe fn from_raw(ptr: *mut ()) -> Self {
        Self {
            slab: &*(ptr.map_addr(|addr| addr & IDX_MASK) as *const _),
            idx: ptr as usize & IDX,
        }
    }

    /// Consumes [`Slot`], converting into a raw pointer that points to the
    /// underlying [`Boxed64`] with the index as the tag (low bits)
    ///
    /// # Safety
    ///
    /// For drop to be called this must be converted back into [`Slot`]
    pub fn into_raw(self) -> *mut () {
        let slot = ManuallyDrop::new(self);

        slot.slab.map_addr(|addr| addr | slot.idx) as *mut ()
    }
}

unsafe impl<T> Send for Slot<T> where T: Send {}
unsafe impl<T> Sync for Slot<T> where T: Sync {}

impl<T> Deref for Slot<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { (*self.inner().slots[self.idx].get()).assume_init_ref() }
    }
}

impl<T> DerefMut for Slot<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { (*self.inner().slots[self.idx].get()).assume_init_mut() }
    }
}

impl<T> AsRef<T> for Slot<T> {
    fn as_ref(&self) -> &T {
        self
    }
}

impl<T> AsMut<T> for Slot<T> {
    fn as_mut(&mut self) -> &mut T {
        self
    }
}

impl<T> Borrow<T> for Slot<T> {
    fn borrow(&self) -> &T {
        self
    }
}

impl<T> BorrowMut<T> for Slot<T> {
    fn borrow_mut(&mut self) -> &mut T {
        self
    }
}

impl<T> Drop for Slot<T> {
    fn drop(&mut self) {
        unsafe { (*self.inner().slots[self.idx].get()).assume_init_drop() }

        let occupancy = self
            .inner()
            .occupancy
            .fetch_xor(1 << self.idx, Ordering::AcqRel);

        // If this was the last slot after Boxed64 was previously dropped, then the
        // underlying heap allocation needs to be dropped
        if occupancy.eq(&!(1 << self.idx)) {
            unsafe {
                drop(Box::from_raw(self.slab as *mut Inner<T>));
            }
        }
    }
}

impl<T> PartialEq<T> for Slot<T>
where
    T: PartialEq<T>,
{
    fn eq(&self, other: &T) -> bool {
        PartialEq::eq(&**self, other)
    }
}

impl<T> PartialEq<Slot<T>> for Slot<T>
where
    T: PartialEq<T>,
{
    fn eq(&self, other: &Slot<T>) -> bool {
        PartialEq::eq(&**self, &**other)
    }
}

impl<T> Eq for Slot<T> where T: PartialEq<T> {}

impl<T: PartialOrd> PartialOrd for Slot<T> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        PartialOrd::partial_cmp(&**self, &**other)
    }
    #[inline]
    fn lt(&self, other: &Self) -> bool {
        PartialOrd::lt(&**self, &**other)
    }
    #[inline]
    fn le(&self, other: &Self) -> bool {
        PartialOrd::le(&**self, &**other)
    }
    #[inline]
    fn ge(&self, other: &Self) -> bool {
        PartialOrd::ge(&**self, &**other)
    }
    #[inline]
    fn gt(&self, other: &Self) -> bool {
        PartialOrd::gt(&**self, &**other)
    }
}

impl<T: Ord> Ord for Slot<T> {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        Ord::cmp(&**self, &**other)
    }
}

impl<T: Hash> Hash for Slot<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}

impl<T: fmt::Display> fmt::Display for Slot<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<T: fmt::Debug> fmt::Debug for Slot<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T> fmt::Pointer for Slot<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ptr: *const T = &**self;
        fmt::Pointer::fmt(&ptr, f)
    }
}

impl<T> Unpin for Slot<T> {}

impl<F: Future + Unpin> Future for Slot<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        F::poll(Pin::new(&mut *self), cx)
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;
    use core::sync::atomic::Ordering;

    use super::{Boxed64, Slot, UninitSlot};

    #[test]
    fn fixed64_allocs_64() {
        let slab = Boxed64::new();

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
        let slab = Boxed64::new();

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

        assert_eq!(slab.inner().occupancy.load(Ordering::Acquire), u64::MAX);
        assert_eq!(slots, (0..64).collect::<Vec<usize>>());

        drop(slots);

        assert_eq!(slab.inner().occupancy.load(Ordering::Acquire), 0);
    }

    #[test]
    fn drops_after_last_slot() {
        let slab = Boxed64::new();

        let slots: Vec<UninitSlot<usize>> =
            (0..64).filter_map(|_| slab.get_uninit_slot()).collect();

        assert_eq!(slots.len(), 64);
        assert!(slab.get_uninit_slot().is_none());

        let slots: Vec<Slot<usize>> = slots
            .into_iter()
            .enumerate()
            .map(|(i, slot)| slot.insert(i))
            .collect();

        assert_eq!(slab.inner().occupancy.load(Ordering::Acquire), u64::MAX);

        drop(slab);

        assert_eq!(slots, (0..64).collect::<Vec<usize>>());
        drop(slots);
    }
}
