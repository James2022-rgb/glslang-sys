# Rust FFI bindings for glslang

![Build](https://github.com/James2022-rgb/glslang-sys/actions/workflows/rust_ci.yml/badge.svg)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Low-level, unsafe Rust bindings to the C interface of [glslang](https://github.com/KhronosGroup/glslang), generated with [rust-bindgen](https://github.com/rust-lang/rust-bindgen).

License
----------------------------
Refer to [glslang's LICENSE.txt](https://github.com/KhronosGroup/glslang/blob/master/LICENSE.txt).

This crate is licensed under the [MIT license](LICENSE-MIT).

Motivation
----------------------------
- [shaderc-rs](https://github.com/google/shaderc-rs) already exists, but it was found not to be straight-forward to build for Android.
- [TimNN/glslang-sys](https://github.com/TimNN/glslang-sys) hasn't been maintained since 2015.

glslang Version
----------------------------
[14.0.0](https://github.com/KhronosGroup/glslang/releases/tag/14.0.0)
https://github.com/KhronosGroup/glslang/commit/a91631b260cba3f22858d6c6827511e636c2458a

Build target support
----------------------------
|                          | Windows            | Linux              |
| ------------------------ | ------------------ | ------------------ |
| x86_64-pc-windows-msvc   | ✅                |                    |
| aarch64-linux-android    | ✅                |                    |
| x86_64-unknown-linux-gnu |                    | ✅                |

Remarks
----------------------------

glslang is built with:
 - `ENABLE_OPT=OFF`
 - `ENABLE_SPVREMAPPER=OFF`
 - (Android) `ANDROID_STL=c++_shared`

Usage
----------------------------

`Cargo.toml`:
```toml
[dependencies]
glslang-sys-2022 = { git = "https://github.com/James2022-rgb/glslang-sys" }
```

Rust code:
```rust
use glslang_sys_2022 as glslang_sys;
```
```rust
unsafe {
  glslang_sys::glslang_initialize_process();
}
```

(WIP)

Building
----------------------------

### glslang

The C++ library [glslang](https://github.com/KhronosGroup/glslang) is required (though bindings are only provided for its C interface).
The [build script](src/build.rs) attempts to obtain the native glslang library binaries in the following order of preference:

1. Check out and build from source, if feature `build-from-source` is enabled. The repository and commit SHA are defined in [known_good.json](known_good.json)
1. Use the prebuilt binaries in the [prebuilt](prebuilt) directory.

#### Building from source

CMake and Python 3.x are required in addition to MSVC or Android NDK.
Refer to the [relevant section](https://github.com/KhronosGroup/glslang#building-cmake) on glslang's README.md.

Build with feature `build-from-source`, e.g.
```bash
cargo build --target x86_64-pc-windows-msvc --features build-from-source
```

(WIP)

#### Using the prebuilt binaries

This is the default behavior when nothing is specified.
