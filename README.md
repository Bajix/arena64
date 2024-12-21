# Arena64
![License](https://img.shields.io/badge/license-MIT-green.svg)
[![Cargo](https://img.shields.io/crates/v/arena64.svg)](https://crates.io/crates/arena64)
[![Documentation](https://docs.rs/arena64/badge.svg)](https://docs.rs/arena64)

Arena64 provides concurrent data structures that return slots which provide mutually exclusive access to interior cells. Slots can be converted into/from raw pointers tagged with the index using the low bits, making these data structures particularly well suited for use-cases that require thin pointers.
