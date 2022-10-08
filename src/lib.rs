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

  /// # Safety
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

/// # Safety
/// - It is the caller's responsibility to ensure the validity of `input`.
pub unsafe fn compile(input: &glslang_input_t, preamble: Option<*const c_char>, option_flags: CompileOptionFlags, source_file_name: Option<&str>) -> Result<Vec<u32>, GlslangErrorLog> {
  let shader = glslang_shader_create(input);

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
  glslang_program_add_shader(program, shader);

  // `glslang_program_link` takes `c_int` but `messages` (`glslang_messages_t` being an enum) can be `i32` or `u32` depending on build target.
  #[allow(clippy::useless_conversion)]
  if glslang_program_link(program, input.messages.try_into().unwrap()) == 0 {
    return Err(GlslangErrorLog::from_program("glslang_program_link".to_string(), program));
  }

  if option_flags.contains(CompileOptionFlags::AddOpSource) {
    let code_c_str = CStr::from_ptr(input.code);
    glslang_program_add_source_text(program, input.stage, code_c_str.as_ptr(), code_c_str.to_str().unwrap().len() as size_t);

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

  
  let spirv = {
    let spirv_size = glslang_program_SPIRV_get_size(program) as usize;
    let spirv_ptr = glslang_program_SPIRV_get_ptr(program) as *mut u32;
    std::slice::from_raw_parts(spirv_ptr, spirv_size).to_vec()
  };

  glslang_program_delete(program);
  glslang_shader_delete(shader);

  Ok(spirv)
}

#[cfg(test)]
mod tests {
  use std::ffi::CString;
  use super::*;

  #[test]
  fn initialize_and_finalize_process() {
    unsafe {
      glslang_initialize_process();
      glslang_finalize_process();
    }
  }

  #[test]
  fn compile_vertex_shader() -> Result<(), GlslangErrorLog> {
    let spirv = unsafe {
      glslang_initialize_process();
      scopeguard::defer! {
        glslang_finalize_process();
      }

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
        resource: &DEFAULT_RESOURCE_LIMITS as *const glslang_resource_t,
      };

      let spirv = compile(&input, None, CompileOptionFlags::AddOpSource, Some("vertex_shader.vert"))?;
      println!("SPIR-V word count: {}", spirv.len());
      spirv
    };

    {
      let spirv_u8 = unsafe {
        std::slice::from_raw_parts(spirv.as_ptr() as *const u8, spirv.len() * std::mem::size_of::<u32>())
      };

      std::fs::write("vertex_shader.spv", spirv_u8).unwrap();
    }

    Ok(())
  }
}

/// Values copied from ` glslang/StandAlone/ResourceLimits.cpp `.
pub const DEFAULT_RESOURCE_LIMITS: glslang_resource_t = glslang_resource_t {
  max_lights: 32,
  max_clip_planes: 6,
  max_texture_units: 32,
  max_texture_coords: 32,
  max_vertex_attribs: 64,
  max_vertex_uniform_components: 4096,
  max_varying_floats: 64,
  max_vertex_texture_image_units: 32,
  max_combined_texture_image_units: 80,
  max_texture_image_units: 32,
  max_fragment_uniform_components: 4096,
  max_draw_buffers: 32,
  max_vertex_uniform_vectors: 128,
  max_varying_vectors: 8,
  max_fragment_uniform_vectors: 16,
  max_vertex_output_vectors: 16,
  max_fragment_input_vectors: 15,
  min_program_texel_offset: -8,
  max_program_texel_offset: 7,
  max_clip_distances: 8,
  max_compute_work_group_count_x: 65535,
  max_compute_work_group_count_y: 65535,
  max_compute_work_group_count_z: 65535,
  max_compute_work_group_size_x: 1024,
  max_compute_work_group_size_y: 1024,
  max_compute_work_group_size_z: 64,
  max_compute_uniform_components: 1024,
  max_compute_texture_image_units: 16,
  max_compute_image_uniforms: 8,
  max_compute_atomic_counters: 8,
  max_compute_atomic_counter_buffers: 1,
  max_varying_components: 60,
  max_vertex_output_components: 64,
  max_geometry_input_components: 64,
  max_geometry_output_components: 128,
  max_fragment_input_components: 128,
  max_image_units: 8,
  max_combined_image_units_and_fragment_outputs: 8,
  max_combined_shader_output_resources: 8,
  max_image_samples: 0,
  max_vertex_image_uniforms: 0,
  max_tess_control_image_uniforms: 0,
  max_tess_evaluation_image_uniforms: 0,
  max_geometry_image_uniforms: 0,
  max_fragment_image_uniforms: 8,
  max_combined_image_uniforms: 8,
  max_geometry_texture_image_units: 16,
  max_geometry_output_vertices: 256,
  max_geometry_total_output_components: 1024,
  max_geometry_uniform_components: 1024,
  max_geometry_varying_components: 64,
  max_tess_control_input_components: 128,
  max_tess_control_output_components: 128,
  max_tess_control_texture_image_units: 16,
  max_tess_control_uniform_components: 1024,
  max_tess_control_total_output_components: 4096,
  max_tess_evaluation_input_components: 128,
  max_tess_evaluation_output_components: 128,
  max_tess_evaluation_texture_image_units: 16,
  max_tess_evaluation_uniform_components: 1024,
  max_tess_patch_components: 120,
  max_patch_vertices: 32,
  max_tess_gen_level: 64,
  max_viewports: 16,
  max_vertex_atomic_counters: 0,
  max_tess_control_atomic_counters: 0,
  max_tess_evaluation_atomic_counters: 0,
  max_geometry_atomic_counters: 0,
  max_fragment_atomic_counters: 8,
  max_combined_atomic_counters: 8,
  max_atomic_counter_bindings: 1,
  max_vertex_atomic_counter_buffers: 0,
  max_tess_control_atomic_counter_buffers: 0,
  max_tess_evaluation_atomic_counter_buffers: 0,
  max_geometry_atomic_counter_buffers: 0,
  max_fragment_atomic_counter_buffers: 1,
  max_combined_atomic_counter_buffers: 1,
  max_atomic_counter_buffer_size: 16384,
  max_transform_feedback_buffers: 4,
  max_transform_feedback_interleaved_components: 64,
  max_cull_distances: 8,
  max_combined_clip_and_cull_distances: 8,
  max_samples: 4,
  max_mesh_output_vertices_nv: 256,
  max_mesh_output_primitives_nv: 512,
  max_mesh_work_group_size_x_nv: 32,
  max_mesh_work_group_size_y_nv: 1,
  max_mesh_work_group_size_z_nv: 1,
  max_task_work_group_size_x_nv: 32,
  max_task_work_group_size_y_nv: 1,
  max_task_work_group_size_z_nv: 1,
  max_mesh_view_count_nv: 4,
  max_mesh_output_vertices_ext: 256,
  max_mesh_output_primitives_ext: 256,
  max_mesh_work_group_size_x_ext: 128,
  max_mesh_work_group_size_y_ext: 128,
  max_mesh_work_group_size_z_ext: 128,
  max_task_work_group_size_x_ext: 128,
  max_task_work_group_size_y_ext: 128,
  max_task_work_group_size_z_ext: 128,
  max_mesh_view_count_ext: 4,
  maxDualSourceDrawBuffersEXT: 1,
  limits: glslang_limits_s {
      non_inductive_for_loops: true,
      while_loops: true,
      do_while_loops: true,
      general_uniform_indexing: true,
      general_attribute_matrix_vector_indexing: true,
      general_varying_indexing: true,
      general_sampler_indexing: true,
      general_variable_indexing: true,
      general_constant_matrix_vector_indexing: true,
  },
};
