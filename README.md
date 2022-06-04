# Rust FFI bindings for glslang

Low-level, unsafe bindings to the C interface of [glslang](https://github.com/KhronosGroup/glslang), generated with [rust-bindgen](https://github.com/rust-lang/rust-bindgen).

## Motivation
[shaderc-rs](https://github.com/google/shaderc-rs) already exists, but it was found not to be straight-forward to build for Android.

## Remarks
glslang is built with:
 - `ENABLE_OPT=OFF`
 - `ENABLE_SPVREMAPPER=OFF`
 - (Android) `ANDROID_STL=c++_shared`

## Usage

(WIP)

## Building

### glslang

The C++ library [glslang](https://github.com/KhronosGroup/glslang) is required (though bindings are only provided for its C interface).
The [build script](src/build.rs) attempts to obtain the native glslang library binaries in the following order of preference:

1. Check out and build from source, if feature `build` is enabled.
1. Use the prebuilt binaries in the [prebuilt](prebuilt) directory.

#### Building from source

CMake and Python 3.x are required in addition to MSVC or Android NDK.
Refer to the [relevant section](https://github.com/KhronosGroup/glslang#building-cmake) on glslang's README.md.

(WIP)