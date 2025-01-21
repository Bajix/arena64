# Arena64
![License](https://img.shields.io/badge/license-MIT-green.svg)
[![Cargo](https://img.shields.io/crates/v/arena64.svg)](https://crates.io/crates/arena64)
[![Documentation](https://docs.rs/arena64/badge.svg)](https://docs.rs/arena64)

Arena64 provides data structures yielding slots that grant mutually exclusive access to interior cells. As slots can be converted into/from raw pointers tagged with the index using the low bits, this is well-suited for use-cases requiring thin-pointers.

## Benchmark Results

### Alloc

|            | `Box<T>`                  | `Box<Box<T>>`                   | `Bump64`                         | `Arena64`                         |
|:-----------|:--------------------------|:--------------------------------|:---------------------------------|:--------------------------------- |
| **`64`**   | `956.77 ns` (✅ **1.00x**) | `3.12 us` (❌ *3.26x slower*)    | `239.17 ns` (🚀 **4.00x faster**) | `336.61 ns` (🚀 **2.84x faster**)  |
| **`128`**  | `1.91 us` (✅ **1.00x**)   | `6.17 us` (❌ *3.23x slower*)    | `430.16 ns` (🚀 **4.44x faster**) | `643.05 ns` (🚀 **2.97x faster**)  |
| **`256`**  | `3.85 us` (✅ **1.00x**)   | `12.44 us` (❌ *3.23x slower*)   | `858.97 ns` (🚀 **4.49x faster**) | `1.31 us` (🚀 **2.93x faster**)    |
| **`512`**  | `7.66 us` (✅ **1.00x**)   | `26.81 us` (❌ *3.50x slower*)   | `1.64 us` (🚀 **4.66x faster**)   | `2.55 us` (🚀 **3.00x faster**)    |
| **`1024`** | `15.22 us` (✅ **1.00x**)  | `46.29 us` (❌ *3.04x slower*)   | `3.23 us` (🚀 **4.72x faster**)   | `5.14 us` (🚀 **2.96x faster**)    |
| **`2048`** | `30.50 us` (✅ **1.00x**)  | `99.62 us` (❌ *3.27x slower*)   | `6.38 us` (🚀 **4.78x faster**)   | `10.18 us` (🚀 **3.00x faster**)   |