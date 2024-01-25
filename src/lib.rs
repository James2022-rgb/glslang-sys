#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

impl Default for glslang_spv_options_t {
  fn default() -> Self {
    glslang_spv_options_t {
      generate_debug_info: false,
      strip_debug_info: false,
      disable_optimizer: true,
      optimize_size: false,
      disassemble: false,
      validate: false,
      emit_nonsemantic_shader_debug_info: false,
      emit_nonsemantic_shader_debug_source: false,
      compile_only: false,
    }
  }
}

impl Default for glslang_resource_t {
  fn default() -> Self {
    unsafe {
      *glslang_default_resource()
    }
  }
}

use std::{
  ffi::CStr,
  os::raw::c_char,
};

use thiserror::Error;
use bitflags::bitflags;

#[derive(Debug, Clone, Error)]
pub struct GlslangErrorLog {
  pub context: String,
  pub info_log: String,
  pub debug_log: String,
}
impl std::fmt::Display for GlslangErrorLog {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "context: {}", self.context)?;
    write!(f, "info_log: {}", self.info_log)?;
    write!(f, "debug_log: {}", self.debug_log)
  }
}
impl GlslangErrorLog {
  #[must_use]
  unsafe fn from_shader(context: String, shader: *mut glslang_shader_t) -> Self {
    Self::new(context, glslang_shader_get_info_log(shader), glslang_shader_get_info_debug_log(shader))
  }
  #[must_use]
  unsafe fn from_program(context: String, program: *mut glslang_program_t) -> Self {
    Self::new(context, glslang_program_get_info_log(program), glslang_program_get_info_debug_log(program))
  }

  /// ## Safety
  /// - `info_log` and `debug_log` MUST point to a valid, null-terminated C string.
  unsafe fn new(context: String, info_log: *const c_char, debug_log: *const c_char) -> Self {
    let info_log = CStr::from_ptr(info_log);
    let debug_log = CStr::from_ptr(debug_log);
    GlslangErrorLog {
      context,
      info_log: info_log.to_str().unwrap().to_owned(),
      debug_log: debug_log.to_str().unwrap().to_owned(),
    }
  }
}

bitflags! {
  pub struct CompileOptionFlags: u32 {
    const GenerateDebugInfo = 0b0001;
    /// Implies `GenerateDebugInfo`.
    const AddOpSource = 0b0010;
  }
}

/// ## Safety
/// - It is the caller's responsibility to ensure the validity of `input`.
pub unsafe fn compile(
  input: &glslang_input_t,
  preamble: Option<*const c_char>,
  option_flags: CompileOptionFlags,
  source_file_name: Option<&str>
) -> Result<Vec<u32>, GlslangErrorLog> {
  let shader = glslang_shader_create(input);
  scopeguard::defer! {
    glslang_shader_delete(shader);
  }

  if let Some(preamble) = preamble {
    glslang_shader_set_preamble(shader, preamble);
  }

  if glslang_shader_preprocess(shader, input) == 0 {
    return Err(GlslangErrorLog::from_shader("glslang_shader_preprocess".to_string(), shader));
  }
  if glslang_shader_parse(shader, input) == 0 {
    return Err(GlslangErrorLog::from_shader("glslang_shader_parse".to_string(), shader));
  }

  let program = glslang_program_create();
  scopeguard::defer! {
    glslang_program_delete(program);
  }
  glslang_program_add_shader(program, shader);

  // `glslang_program_link` takes `c_int` but `messages` (`glslang_messages_t` being an enum) can be `i32` or `u32` depending on build target.
  #[allow(clippy::useless_conversion)]
  if glslang_program_link(program, input.messages.try_into().unwrap()) == 0 {
    return Err(GlslangErrorLog::from_program("glslang_program_link".to_string(), program));
  }

  if option_flags.contains(CompileOptionFlags::AddOpSource) {
    let code_c_str = CStr::from_ptr(input.code);
    glslang_program_add_source_text(program, input.stage, code_c_str.as_ptr(), code_c_str.to_str().unwrap().len());

    if let Some(source_file_name) = source_file_name {
      let source_file_name_c_string = std::ffi::CString::new(source_file_name).unwrap();
      glslang_program_set_source_file(program, input.stage, source_file_name_c_string.as_ptr());
    }
  }

  let mut spv_options = glslang_spv_options_t {
    generate_debug_info: option_flags.intersects(CompileOptionFlags::GenerateDebugInfo | CompileOptionFlags::AddOpSource),
    validate: true,
    ..Default::default()
  };

  glslang_program_SPIRV_generate_with_options(program, input.stage, &mut spv_options);

  if !glslang_program_SPIRV_get_messages(program).is_null() {
    let messages_c_str = CStr::from_ptr(glslang_program_SPIRV_get_messages(program));
    println!("{:?}", messages_c_str);
  }

  let spirv: Vec<u32> = {
    let spirv_size = glslang_program_SPIRV_get_size(program) as usize;
    let spirv_ptr: *mut u32 = glslang_program_SPIRV_get_ptr(program);
    std::slice::from_raw_parts(spirv_ptr, spirv_size).to_vec()
  };

  Ok(spirv)
}

pub use process::GlslangProcess;

mod process {
  use std::sync::Mutex;
  use super::{glslang_initialize_process, glslang_finalize_process};

  static GLSLANG_PROCESS_MUTEX: Mutex<()> = Mutex::new(());

  /// Calls [`glslang_initialize_process`] on construction, and [`glslang_finalize_process`] on [`Drop`].
  ///
  /// ## Thread safety
  /// Safe. Can be constructed/dropped concurrently in the same process, making it safe to run `cargo test` without the use of `--test-threads=1`.
  pub struct GlslangProcess {
    _private: (),
  }
  impl Default for GlslangProcess {
    fn default() -> Self {
      let _lock = GLSLANG_PROCESS_MUTEX.lock().unwrap();

      unsafe {
        glslang_initialize_process();
      }

      Self { _private: () }
    }
  }
  impl Drop for GlslangProcess {
    fn drop(&mut self) {
      let _lock = GLSLANG_PROCESS_MUTEX.lock().unwrap();

      unsafe {
        glslang_finalize_process();
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use std::ffi::CString;
  use super::*;

  #[test]
  fn initialize_and_finalize_process() {
    let _process = GlslangProcess::default();
  }

  #[test]
  fn compile_vertex_shader() -> Result<(), GlslangErrorLog> {
    let spirv = unsafe {
      let _process = GlslangProcess::default();

      let source =
        r##"
        #version 450
        layout(location = 0) out vec2 out_uv;
        void main() {
          out_uv = vec2((gl_VertexIndex << 1) & 2, gl_VertexIndex & 2);
          gl_Position = vec4(out_uv * 2.0 + -1.0, 0.0, 1.0);
        }
        "##;

      let source_c_string = CString::new(source).unwrap();
      let resource_limits: glslang_resource_t = Default::default();

      let callbacks = glsl_include_callbacks_t {
        include_system: None,
        include_local: None,
        free_include_result: None,
      };

      let input = glslang_input_t {
        language: glslang_source_t_GLSLANG_SOURCE_GLSL,
        stage: glslang_stage_t_GLSLANG_STAGE_VERTEX,
        client: glslang_client_t_GLSLANG_CLIENT_VULKAN,
        client_version: glslang_target_client_version_t_GLSLANG_TARGET_VULKAN_1_1,
        target_language: glslang_target_language_t_GLSLANG_TARGET_SPV,
        target_language_version: glslang_target_language_version_t_GLSLANG_TARGET_SPV_1_0,
        code: source_c_string.as_ptr(),
        default_version: 100,
        default_profile: glslang_profile_t_GLSLANG_NO_PROFILE,
        force_default_version_and_profile: 0,
        forward_compatible: 0,
        messages: glslang_messages_t_GLSLANG_MSG_DEFAULT_BIT | glslang_messages_t_GLSLANG_MSG_SPV_RULES_BIT | glslang_messages_t_GLSLANG_MSG_VULKAN_RULES_BIT,
        resource: &resource_limits,
        callbacks,
        callbacks_ctx: core::ptr::null_mut(),
      };

      let spirv = compile(&input, None, CompileOptionFlags::AddOpSource, Some("vertex_shader.vert"))?;
      println!("SPIR-V word count: {}", spirv.len());
      spirv
    };

    {
      let spirv_u8 = unsafe {
        std::slice::from_raw_parts(spirv.as_ptr() as *const u8, spirv.len() * std::mem::size_of::<u32>())
      };

      std::fs::write("vertex_shader.spv", spirv_u8)
        .unwrap();
    }

    Ok(())
  }
}
