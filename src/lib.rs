#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(any(test, feature = "extern_crate_alloc"))]
extern crate alloc;

pub(crate) const IDX: usize = (1 << 6) - 1;
pub(crate) const IDX_MASK: usize = !IDX;

#[cfg_attr(docsrs, doc(cfg(feature = "extern_crate_alloc")))]
#[cfg(feature = "extern_crate_alloc")]
pub mod arena;
#[cfg_attr(docsrs, doc(cfg(feature = "extern_crate_alloc")))]
#[cfg(feature = "extern_crate_alloc")]
pub mod boxed;
pub mod heapless;
