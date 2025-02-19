#include "common.glsl"
#include "cotangent_frame.glsl"
#include "native/ATTENUATION_MODE"
#include "native/RENDER_TECHNIQUE"

// uniform sampler2D shadow_sampler;
uniform sampler2D diffuse_sampler;
uniform sampler2D normal_sampler;
uniform sampler2D specular_sampler;

// uniform vec2 shadow_dimensions;
// uniform vec2 diffuse_dimensions;
uniform vec2 normal_dimensions;
// uniform vec2 specular_dimensions;

uniform uint display_mode;

// layout(binding = 0) uniform atomic_uint shading_ops;

in vec2 fs_pos_in_tex;
in vec3 fs_pos_in_lgt;
in vec3 fs_nor_in_lgt;
in vec3 fs_tan_in_lgt;

#if defined(RENDER_TECHNIQUE_CLUSTERED)
uniform uvec3 cluster_dims;

layout(binding = 10) buffer MaybeActiveClusterIndicesBuffer {
  uint maybe_active_cluster_indices[];
};

layout(binding = 11) buffer ActiveClusterLightCountsBuffer {
  uint active_cluster_light_counts[];
};

layout(binding = 12) buffer ActiveClusterLightOffsetsBuffer {
  uint active_cluster_light_offsets[];
};

layout(binding = 13) buffer LightIndicesBuffer { uint light_indices[]; };

in vec3 fs_pos_in_cls;
#endif

layout(location = 0) out vec4 frag_color;
layout(location = 1) out vec3 frag_nor_in_lgt;

vec3 heatmap(float value, float minVal, float maxVal) {
  vec3 color = vec3(0.0, 0.0, 0.0);
  float range = maxVal - minVal;
  float adjustedVal = clamp(value - minVal, 0.0, range);
  float step = range / 6.0;
  if (value < step) {
    color.z = value / step;
  } else if (value < 2.0 * step) {
    color.y = (value - step) / step;
    color.z = 1.0;
  } else if (value < 3.0 * step) {
    color.y = 1.0;
    color.z = 1.0 - (value - 2.0 * step) / step;
  } else if (value < 4.0 * step) {
    color.x = (value - 3.0 * step) / step;
    color.y = 1.0;
  } else if (value < 5.0 * step) {
    color.x = 1.0;
    color.y = 1.0 - (value - 4.0 * step) / step;
  } else {
    color.x = 1.0;
    color.y = (value - 5.0 * step) / step;
    color.z = (value - 5.0 * step) / step;
  }
  return color;
}

uvec3 separate_bits_by_2(uvec3 x) {
  // x       = 0b??????????????????????jihgfedcba
  // mask    = 0b00000000000000000000001111111111
  x &= 0x000003ff;
  // x       = 0b0000000000000000000000jihgfedcba
  // x << 10 = 0b000000000000jihgfedcba0000000000
  // mask    = 0b00000000000011111000000000011111
  x = (x | (x << 10)) & 0x00f8001f;
  // x       = 0b000000000000jihgf0000000000edcba
  // x << 04 = 0b00000000jihgf0000000000edcba0000
  // mask    = 0b00000000111000011000000111000011
  x = (x | (x << 04)) & 0x001e81c3;
  // x       = 0b00000000jih0000gf000000edc0000ba
  // x << 02 = 0b000000jih0000gf000000edc0000ba00
  // mask    = 0b00000011001001001000011001001001
  x = (x | (x << 02)) & 0x03248649;
  // x       = 0b000000ji00h00g00f0000ed00c00b00a
  // x << 02 = 0b0000ji00h00g00f0000ed00c00b00a00
  // mask    = 0b00001001001001001001001001001001
  x = (x | (x << 02)) & 0x09249249;
  // x       = 0b0000j00i00h00g00f00e00d00c00b00a
  return x;
}

uint to_morton_3(uvec3 p) {
  uvec3 q = separate_bits_by_2(p);
  return (q.z << 2) | (q.y << 1) | q.x;
}

// FIXME
vec3 sample_nor_in_tan(vec2 pos_in_tex) {
  float dx = 1.0 / normal_dimensions.x;
  float dy = 1.0 / normal_dimensions.y;

  float v00 = texture(normal_sampler, pos_in_tex + vec2(-dx, -dy)).x;
  float v01 = texture(normal_sampler, pos_in_tex + vec2(0.0, -dy)).x;
  float v02 = texture(normal_sampler, pos_in_tex + vec2(dx, -dy)).x;
  float v10 = texture(normal_sampler, pos_in_tex + vec2(-dx, 0.0)).x;
  // v11
  float v12 = texture(normal_sampler, pos_in_tex + vec2(dx, 0.0)).x;
  float v20 = texture(normal_sampler, pos_in_tex + vec2(-dx, dy)).x;
  float v21 = texture(normal_sampler, pos_in_tex + vec2(0.0, dy)).x;
  float v22 = texture(normal_sampler, pos_in_tex + vec2(dx, dy)).x;

  float x = (v02 - v00) + 2.0 * (v12 - v10) + (v22 - v20);
  float y = (v20 - v00) + 2.0 * (v21 - v01) + (v22 - v02);

  return normalize(vec3(-x, -y, 1.0));
}

// All computations are in lgt space.
vec3 point_light_contribution(PointLight point_light, vec3 nor, vec3 frag_pos,
                              vec3 lgt_dir_norm) {
  vec3 pos_from_frag_to_light = point_light.pos_in_lgt.xyz - frag_pos;
  vec3 light_dir_norm = normalize(pos_from_frag_to_light);

  float I = point_light.att[0];
  float C = point_light.att[1];
  float R0 = point_light.att[2];
  float R1 = point_light.att[3]; // R1^2 = I/C

  // Attenuation.
  float d_sq_unclipped = dot(pos_from_frag_to_light, pos_from_frag_to_light);
  float d_unclipped = sqrt(d_sq_unclipped);
  float d_sq = max(R0, d_sq_unclipped);
  float d = max(R0, d_unclipped);

  float diffuse_attenuation;
  float specular_attenuation;

  if (d_unclipped < R1) {
#if defined(ATTENUATION_MODE_STEP)
    diffuse_attenuation = I * (1.0 / R0 + R0 - 1.0 / R1) / R1;
#elif defined(ATTENUATION_MODE_LINEAR)
    // Linear doesn't go infinite so we can use the unclipped distance.
    diffuse_attenuation = I - (I / R1) * d_unclipped;
#elif defined(ATTENUATION_MODE_PHYSICAL)
    diffuse_attenuation = I / d_sq;
#elif defined(ATTENUATION_MODE_INTERPOLATED)
    diffuse_attenuation = I / d_sq - (C / R1) * d;
    // diffuse_attenuation = I / (d_sq + 1) - C * pow(d_sq / (R1 * R1), 1);
#elif defined(ATTENUATION_MODE_REDUCED)
    diffuse_attenuation = I / d_sq - C;
#elif defined(ATTENUATION_MODE_SMOOTH)
    diffuse_attenuation = I / d_sq - 3.0 * C + (2.0 * C / R1) * d;
#else
#error invalid attenuation mode!
#endif
    specular_attenuation = I * (1.0 - d_sq / (R1 * R1));
  } else {
    diffuse_attenuation = 0.0;
    specular_attenuation = 0.0;
  }

  // Diffuse.
  float diffuse_weight = max(0.0, dot(nor, light_dir_norm));

  // Specular.
  float specular_angle =
      max(0.0, dot(lgt_dir_norm, reflect(-light_dir_norm, nor)));
  // TODO: Upload shininess
  float specular_weight = pow(specular_angle, 10.0);

  // LIGHT ATTENUATION.
  // return vec3(diffuse_attenuation);

  // LIGHT CONTRIBUTION.
  return
      // Diffuse
      (diffuse_attenuation * diffuse_weight) * point_light.diffuse.rgb *
          texture(diffuse_sampler, fs_pos_in_tex).rgb +
      // Specular
      (specular_attenuation * specular_weight) * point_light.specular.rgb *
          texture(specular_sampler, fs_pos_in_tex).rgb;
}

void main() {
  // Perturbed normal in camera space.
  // TODO: Consider https://github.com/mickvangelderen/vr-lab/issues/3
  vec3 fs_nor_in_lgt_norm = normalize(fs_nor_in_lgt);
#define PER_PIXEL_COTANGENT_FRAME
#if defined(PER_PIXEL_COTANGENT_FRAME)
  mat3 tbn = cotangent_frame(fs_nor_in_lgt, fs_pos_in_lgt, fs_pos_in_tex);
  vec3 nor_in_lgt = tbn * sample_nor_in_tan(fs_pos_in_tex);
#else
  vec3 fs_bitan_in_lgt_norm =
      cross(fs_nor_in_lgt_norm, normalize(fs_tan_in_lgt));
  vec3 fs_tan_in_lgt_norm = cross(fs_bitan_in_lgt_norm, fs_nor_in_lgt_norm);
  mat3 dir_from_tan_to_cam =
      mat3(fs_tan_in_lgt_norm, fs_bitan_in_lgt_norm, fs_nor_in_lgt_norm);
  vec3 nor_in_lgt = dir_from_tan_to_cam * sample_nor_in_tan(fs_pos_in_tex);
#endif
  frag_nor_in_lgt = nor_in_lgt * 0.5 + vec3(0.5);

  // TODO: Render unmasked and masked materials separately.
  // vec4 diffuse_sample = texture(diffuse_sampler, fs_pos_in_tex);
  // if (diffuse_sample.a < 0.5) {
  //   discard;
  // }

  if (display_mode == 0) {

    vec3 cam_dir_in_lgt_norm = normalize(cam_pos_in_lgt.xyz - fs_pos_in_lgt);

#if defined(RENDER_TECHNIQUE_NAIVE)
    vec3 color_accumulator = vec3(0.0);
    for (uint i = 0; i < light_buffer.light_count.x; i++) {
      color_accumulator +=
          point_light_contribution(light_buffer.point_lights[i], nor_in_lgt,
                                   fs_pos_in_lgt, cam_dir_in_lgt_norm);
      // atomicCounterIncrement(shading_ops);
    }
    frag_color = vec4(color_accumulator, 1.0);
#elif defined(RENDER_TECHNIQUE_CLUSTERED)
    uvec3 fs_idx_in_cls = uvec3(fs_pos_in_cls);

    // CLUSTER INDICES X, Y, Z
    // frag_color = vec4(vec3(fs_idx_in_cls)/vec3(cluster_dims), 1.0);

    // CLUSTER INDICES X, Y, Z mod 3
    // vec3 cluster_index_colors = vec3((fs_idx_in_cls % 3) + 1)/4.0;
    // frag_color = vec4(cluster_index_colors.xyz, 1.0);

    // CLUSTER MORTON INDEX
    // uint cluster_morton_index = to_morton_3(fs_idx_in_cls);
    // frag_color = vec4(                              //
    //     float((cluster_morton_index >> 16) & 0xff) / 255.0, //
    //     float((cluster_morton_index >> 8) & 0xff) / 255.0,  //
    //     float((cluster_morton_index >> 0) & 0xff) / 255.0, 1.0);

    uint cluster_index = index_3_to_1(fs_idx_in_cls, cluster_dims);
    uint maybe_active_cluster_index =
        maybe_active_cluster_indices[cluster_index];

    uint cluster_light_count;
    uint cluster_light_offset;
    if (maybe_active_cluster_index == 0) {
      cluster_light_count = 0;
      cluster_light_offset = 0;
    } else {
      uint active_cluster_index = maybe_active_cluster_index - 1;
      cluster_light_count = active_cluster_light_counts[active_cluster_index];
      cluster_light_offset = active_cluster_light_offsets[active_cluster_index];
    }

    // CLUSTER LENGHTS
    // frag_color = vec4(vec3(float(cluster_light_count) / 132.0), 1.0);
    // frag_color = vec4(heatmap(float(cluster_light_count), 0.0, 32.0), 1.0);

    // COLORED CLUSTER LENGTHS
    // if (cluster_light_count == 0) {
    //   discard;
    // }
    // frag_color = vec4(vec3(float(cluster_light_count)/2.0) *
    // cluster_index_colors, 1.0);

    // CLUSTERED SHADING
    vec3 color_accumulator = vec3(0.0);
    for (uint i = 0; i < cluster_light_count; i++) {
      uint light_index =
          light_indices[cluster_light_offset + i];

      color_accumulator += point_light_contribution(
          light_buffer.point_lights[light_index], nor_in_lgt, fs_pos_in_lgt,
          cam_dir_in_lgt_norm);
      // atomicCounterIncrement(shading_ops);
    }
    frag_color = vec4(color_accumulator, 1.0);
#else
#error Unimplemented render technique!
#endif
  }

  if (display_mode == 1) {
    // DIFFUSE TEXTURE
    frag_color = texture(diffuse_sampler, fs_pos_in_tex);
  }

  if (display_mode == 2) {
    // NORMAL TEXTURE
    frag_color = texture(normal_sampler, fs_pos_in_tex);
  }

  if (display_mode == 2) {
    // SPECULAR_TEXTURE
    frag_color = texture(specular_sampler, fs_pos_in_tex);
  }

  if (display_mode == 3) {
    // NORMAL
    frag_color = vec4(nor_in_lgt, 1.0);
  }
}
