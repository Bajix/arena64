#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

pub(crate) const IDX: usize = (1 << 6) - 1;
pub(crate) const IDX_MASK: usize = !IDX;

mod arena;
mod boxed;

pub use arena::{Arena64, Bump64};
pub use boxed::{Boxed64, Slot, UninitSlot};
