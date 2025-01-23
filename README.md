# Arena64
![License](https://img.shields.io/badge/license-MIT-green.svg)
[![Cargo](https://img.shields.io/crates/v/arena64.svg)](https://crates.io/crates/arena64)
[![Documentation](https://docs.rs/arena64/badge.svg)](https://docs.rs/arena64)

Arena64 provides data structures that pre-allocate storage for 64 elements at a time with insertions yielding slots that uniquely own access to the respective cell. Comparable to `Box`, slots can be converted into/from raw pointers by storing the index in the low-bits, making them well-suited for use cases requiring thin-pointers.

## Benchmark Results

### Alloc

|            | `Box`                     | `Bump64`                         | `Arena64`                         |
|:-----------|:--------------------------|:---------------------------------|:--------------------------------- |
| **`64`**   | `967.87 ns` (✅ **1.00x**) | `234.03 ns` (🚀 **4.14x faster**) | `333.00 ns` (🚀 **2.91x faster**)  |
| **`128`**  | `1.92 us` (✅ **1.00x**)   | `444.37 ns` (🚀 **4.32x faster**) | `646.06 ns` (🚀 **2.97x faster**)  |
| **`256`**  | `3.81 us` (✅ **1.00x**)   | `848.89 ns` (🚀 **4.49x faster**) | `1.30 us` (🚀 **2.94x faster**)    |
| **`512`**  | `7.69 us` (✅ **1.00x**)   | `1.64 us` (🚀 **4.69x faster**)   | `2.57 us` (🚀 **2.99x faster**)    |
| **`1024`** | `15.23 us` (✅ **1.00x**)  | `3.24 us` (🚀 **4.70x faster**)   | `5.06 us` (🚀 **3.01x faster**)    |
| **`2048`** | `30.43 us` (✅ **1.00x**)  | `6.45 us` (🚀 **4.72x faster**)   | `9.93 us` (🚀 **3.06x faster**)    |