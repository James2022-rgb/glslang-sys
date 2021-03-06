extern crate bindgen;

//
// Host: Windows, Target: x86_64-pc-windows-msvc
//  cmake .. -DCMAKE_INSTALL_PREFIX="install" -DENABLE_OPT=OFF -DENABLE_SPVREMAPPER=OFF -DSPIRV_SKIP_TESTS=ON -DSPIRV_SKIP_EXECUTABLES=ON
//  cmake --build . --config Release --target install
//
// Host: Windows, Target: aarch64-linux-android
//  cmake .. -G "Unix Makefiles" -DCMAKE_INSTALL_PREFIX="install" -DENABLE_OPT=OFF -DENABLE_SPVREMAPPER=OFF -DSPIRV_SKIP_TESTS=ON -DSPIRV_SKIP_EXECUTABLES=ON -DANDROID_ABI=arm64-v8a -DCMAKE_BUILD_TYPE=Release -DANDROID_STL=c++_shared -DANDROID_PLATFORM=android-24 -DCMAKE_SYSTEM_NAME=Android -DANDROID_TOOLCHAIN=clang -DANDROID_ARM_MODE=arm -DCMAKE_MAKE_PROGRAM=%ANDROID_NDK_HOME%\prebuilt\windows-x86_64\bin\make.exe -DCMAKE_TOOLCHAIN_FILE=%ANDROID_NDK_HOME%/build/cmake/android.toolchain.cmake
//  cmake --build . --config Release --target install
//

use std::env;
use std::path::PathBuf;
use std::io::{self, Write};
use std::process::{self, Command};

use log::info;
use thiserror::Error;
use scopeguard::defer;

fn make_package_version_string() -> String {
  format!(
    "{}.{}",
    env::var("CARGO_PKG_VERSION_MAJOR").unwrap().parse::<u8>().unwrap(),
    env::var("CARGO_PKG_VERSION_MINOR").unwrap().parse::<u8>().unwrap(),
  )
}

#[cfg(target_os = "windows")]
fn get_win32_unused_drive_letters() -> Vec<char> {
  let mut logical_drives: Vec<char> = Vec::new();
  let mut bitfield = unsafe { kernel32::GetLogicalDrives() };
  let mut drive_letter = 'A';

  while bitfield != 0 {
    if bitfield & 1 == 0 {
      logical_drives.push(drive_letter);
    }
    drive_letter = char::from_u32((drive_letter as u32) + 1).unwrap();
    bitfield >>= 1;
  }
  logical_drives
}

#[derive(Error, Debug)]
enum BuilderError {
  #[error("No unused drive letter found for working around MAX_PATH limitation on Windows")]
  NoAvailableDriveLetter,
  #[error("Failed to configure project with cmake")]
  ConfigureFailed { output: process::Output },
  #[error("Failed to build project with cmake")]
  BuildFailed { output: process::Output },
}

struct Builder {
  glslang_clone_dst_dir_path: PathBuf,
}
impl Builder {
  fn new() -> Self {
    let glslang_clone_dst_dir = format!("glslang-{}", make_package_version_string());
    let glslang_clone_dst_dir_path = Self::get_raw_out_dir().join(glslang_clone_dst_dir);

    Builder {
      glslang_clone_dst_dir_path,
    }
  }

  fn get_raw_out_dir() -> PathBuf {
    PathBuf::from(env::var("OUT_DIR").unwrap())
  }
}
impl Builder {
  fn fetch_glslang(&self) -> io::Result<()> {
    // Idea taken from:
    //  https://github.com/meh/rust-ffmpeg-sys
    //  https://github.com/google/shaderc-rs

    const GLSLANG_CLONE_URL: &str = "https://github.com/KhronosGroup/glslang";
    const GLSLANG_CLONE_BRANCH: &str = "master";
  
    let original_current_dir = env::current_dir().unwrap();
    defer! {
      env::set_current_dir(original_current_dir).unwrap()
    }
  
    let _ = std::fs::remove_dir_all(&self.glslang_clone_dst_dir_path);
    let output = Command::new("git")
      .arg("clone")
      .arg(GLSLANG_CLONE_URL)
      .arg("-b").arg(GLSLANG_CLONE_BRANCH)
      .arg(&self.glslang_clone_dst_dir_path)
      .output()?;
    io::stdout().write_all(&output.stdout).unwrap();
    
    env::set_current_dir(&self.glslang_clone_dst_dir_path).unwrap();
  
    let output = Command::new("git")
      .arg("clone")
      .arg("https://github.com/google/googletest.git")
      .arg("External/googletest")
      .output()?;
    io::stdout().write_all(&output.stdout).unwrap();
  
    Command::new("python").arg("update_glslang_sources.py").status().unwrap();
    
    if output.status.success() {
      Ok(())
    }
    else {
      Err(io::Error::new(io::ErrorKind::Other, "Failed to fetch glslang !"))
    }
  }

  fn build_glslang(&self, target_os: &str, target_arch: &str) -> Result<PathBuf, BuilderError> {
    // Building is only supported for these platforms now:
    assert!(cfg!(target_os = "windows"), "Building only supported on Windows.");

    let original_current_dir = env::current_dir().unwrap();
    defer! {
      env::set_current_dir(original_current_dir).unwrap()
    }

    env::set_current_dir(&self.glslang_clone_dst_dir_path).unwrap();

    // TODO: cfg_attr ?
    #[cfg(target_os = "windows")]
    let drive_letter = {
      let unused_drive_letters = get_win32_unused_drive_letters();
      *unused_drive_letters.first().ok_or(BuilderError::NoAvailableDriveLetter)?
    };

    #[cfg(target_os = "windows")]
    let mapped_glslang_clone_dst_dir_path =
      {
        Command::new("subst").arg(format!("{}:", drive_letter)).arg(Self::get_raw_out_dir()).status().unwrap();
        let relative = self.glslang_clone_dst_dir_path.strip_prefix(Self::get_raw_out_dir()).unwrap();
        PathBuf::from(format!(r#"{}:/"#, drive_letter)).join(relative)
      };
    #[cfg(not(target_os = "windows"))]
    let mapped_glslang_clone_dst_dir_path = self.glslang_clone_dst_dir_path.clone();
    info!("mapped_glslang_clone_dst_dir_path:{:?}", mapped_glslang_clone_dst_dir_path);

    #[cfg(target_os = "windows")]
    defer! {
      Command::new("subst").arg(format!("{}:", drive_letter)).arg("/d").status().unwrap();
    }

    let build_dir = format!("build-{}-{}", target_os, target_arch);
    let build_dir_path = self.glslang_clone_dst_dir_path.join(&build_dir);
    let mapped_build_dir_path = mapped_glslang_clone_dst_dir_path.join(&build_dir);
    
    std::fs::create_dir_all(&mapped_build_dir_path).unwrap();
    env::set_current_dir(&mapped_build_dir_path).unwrap();

    let install_dir = "install";
    let install_dir_path = build_dir_path.join(install_dir);
    let mapped_install_dir_path = mapped_build_dir_path.join(install_dir);
    std::fs::create_dir_all(&mapped_install_dir_path).unwrap();

    // Configure.
    match target_os {
      "windows" => {
        let output = Command::new("cmake")
          .arg("..")
          .arg(format!(r#"-DCMAKE_INSTALL_PREFIX={}"#, install_dir))
          // glslang options
          .arg(r#"-DENABLE_OPT=OFF"#)
          .arg(r#"-DENABLE_SPVREMAPPER=OFF"#)
          // SPIRV-Tools options
          .arg(r#"-DSPIRV_SKIP_TESTS=ON"#)
          .arg(r#"-DSPIRV_SKIP_EXECUTABLES=ON"#)
          .output()
          .unwrap();
        if !output.status.success() {
          return Err(BuilderError::ConfigureFailed { output });
        }
      },
      "android" => {
        assert!(cfg!(target_os = "windows"), "TODO: CMAKE_MAKE_PROGRAM for other platforms.");

        let android_ndk_home = env::var("ANDROID_NDK_HOME").expect("Environment variable ANDROID_NDK_HOME not set !");
        let android_abi_name = match target_arch {
          "aarch64" => "arm64-v8a",
          "arm"     => "armeabi-v7a",
          _ => panic!("Unexpected CARGO_CFG_TARGET_ARCH: {:?}", target_arch),
        };

        let output = Command::new("cmake")
          .arg("..")
          .arg("-G").arg("Unix Makefiles")
          .arg(format!(r#"-DCMAKE_INSTALL_PREFIX={}"#, install_dir))
          // glslang options
          .arg(r#"-DENABLE_OPT=OFF"#)
          .arg(r#"-DENABLE_SPVREMAPPER=OFF"#)
          // SPIRV-Tools options
          .arg(r#"-DSPIRV_SKIP_TESTS=ON"#)
          .arg(r#"-DSPIRV_SKIP_EXECUTABLES=ON"#)
          // Android
          .arg(format!(r#"-DANDROID_ABI={}"#, android_abi_name))
          .arg(r#"-DCMAKE_BUILD_TYPE=Release"#)
          .arg(r#"-DANDROID_STL=c++_shared"#)
          .arg(r#"-DANDROID_PLATFORM=android-24"#)
          .arg(r#"-DCMAKE_SYSTEM_NAME=Android"#)
          .arg(r#"-DANDROID_TOOLCHAIN=clang"#)
          .arg(r#"-DANDROID_ARM_MODE=arm"#)
          .arg(format!(r#"-DCMAKE_MAKE_PROGRAM={}/prebuilt/windows-x86_64/bin/make.exe"#, android_ndk_home))
          .arg(format!(r#"-DCMAKE_TOOLCHAIN_FILE={}/build/cmake/android.toolchain.cmake"#, android_ndk_home))
          .output()
          .unwrap();
        io::stdout().write_all(&output.stdout).unwrap();
        if !output.status.success() {
          io::stdout().write_all(&output.stderr).unwrap();
          return Err(BuilderError::ConfigureFailed { output });
        }
      },
      _ => panic!("Unexpected target_os:{:?}", target_os)
    };

    // Build.
    // cmake --build . --config Release --target install
    let output = Command::new("cmake")
      .arg("--build")
      .arg(".")
      .arg("--config").arg("Release")
      .arg("--target").arg(install_dir)
      .arg("--parallel").arg("8")
      .output()
      .unwrap();

    if output.status.success() {
      Ok(install_dir_path)
    }
    else {
      Err(BuilderError::BuildFailed { output })
    }
  }
}

fn get_prebuilt_glslang_install_dir() -> PathBuf {
  let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();  

  let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
  let target_dir = match target_os.as_str() {
    "windows" => {
      assert_eq!(target_arch, "x86_64");
      "x86_64-pc-windows-msvc"
    },
    "android" => {
      assert_eq!(target_arch, "aarch64");
      "aarch64-linux-android"
    },
    _ => panic!("Unexpected CARGO_CFG_TARGET_OS:{:?}", target_os)
  };

  let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

  PathBuf::from(manifest_dir).join(format!("prebuilt/{}", target_dir))
}

fn main() {
  const WRAPPER_HEADER: &str = "src/wrapper.h";
  const LIBS: [&str; 8] = [
    "GenericCodeGen",
    "glslang",
    "glslang-default-resource-limits",
    "HLSL",
    "MachineIndependent",
    "OGLCompiler",
    "OSDependent",
    "SPIRV",
  ];

  env_logger::init();

  let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
  let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();

  let install_dir_path: PathBuf =
    if cfg!(feature = "build-from-source") {
      let builder = Builder::new();
      builder.fetch_glslang().unwrap();

      match builder.build_glslang(&target_os, &target_arch) {
        Ok(path) => path,
        Err(error) => {
          match error {
            BuilderError::NoAvailableDriveLetter => (),
            BuilderError::ConfigureFailed { output } => {
              io::stderr().write_all(&output.stdout).unwrap();
              io::stderr().write_all(&output.stderr).unwrap();
            },
            BuilderError::BuildFailed { output } => {
              io::stderr().write_all(&output.stdout).unwrap();
              io::stderr().write_all(&output.stderr).unwrap();
            },
          }
          panic!("Failed to build glslang from source !");
        },
      }
    }
    else {
      get_prebuilt_glslang_install_dir()
    };

  let link_search_path = install_dir_path.join("lib");
  println!("cargo:rustc-link-search=native={}", link_search_path.to_str().unwrap());
  for lib in LIBS {
    println!("cargo:rustc-link-lib=static={}", lib);
  }
  println!("cargo:rerun-if-changed={}", WRAPPER_HEADER);

  // For Android, link to `c++_shared`.
  if target_os == "android" {
    println!("cargo:rustc-link-lib=c++_shared");
  }

  let glslang_include_dir = install_dir_path.join("include");

  let mut bindings_builder = bindgen::Builder::default()
    .header(WRAPPER_HEADER)
    .allowlist_file(".*glslang_c_shader_types.h")
    .allowlist_file(".*glslang_c_interface.h")
    .parse_callbacks(Box::new(bindgen::CargoCallbacks))
    .clang_arg(format!("-I{}", glslang_include_dir.to_str().unwrap()));

  // For Android, add header search paths:
  //  %ANDROID_NDK_HOME%/sysroot/usr/include
  //  %ANDROID_NDK_HOME%/sysroot/usr/include/(aarch64-linux-android|arm-linux-androideabi)
  if target_os == "android" {
    let android_ndk_home = env::var("ANDROID_NDK_HOME").expect("Environment variable ANDROID_NDK_HOME not set !");
    info!("ANDROID_NDK_HOME: {:?}", android_ndk_home);
    
    let android_arch_name = match target_arch.as_str() {
      "aarch64" => "aarch64-linux-android",
      "arm"     => "arm-linux-androideabi",
      _ => panic!("Unexpected CARGO_CFG_TARGET_ARCH: {:?}", target_arch),
    };

    let android_ndk_include_dir: PathBuf = [ android_ndk_home.as_str(), r#"sysroot/usr/include"# ].iter().collect();
    let android_ndk_arch_include_dir: PathBuf = android_ndk_include_dir.join(android_arch_name);
    info!("Android NDK include directory: {:?}", android_ndk_include_dir);
    info!("Android NDK architecture-dependent include directory: {:?}", android_ndk_arch_include_dir);

    bindings_builder = bindings_builder
      .clang_arg(format!("-isystem{}", android_ndk_arch_include_dir.to_str().unwrap()))
      .clang_arg(format!("-isystem{}", android_ndk_include_dir.to_str().unwrap()));
  }

  let bindings = bindings_builder.generate().expect("Unable to generate bindings !");

  let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
  bindings
    .write_to_file(out_dir.join("bindings.rs"))
    .expect("Couldn't write bindings !");
}
