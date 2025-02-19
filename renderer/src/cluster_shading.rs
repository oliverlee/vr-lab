use crate::*;
use std::time::Instant;

#[derive(Debug)]
#[repr(C)]
pub struct ClusterHeader {
    pub dimensions: Vector4<u32>,
    pub wld_to_cls: Matrix4<f32>,
    pub cls_to_wld: Matrix4<f32>,
}

pub const CLUSTER_BUFFER_DECLARATION: &'static str = r"
layout(std430, binding = CLUSTER_BUFFER_BINDING) buffer ClusterBuffer {
    uint clusters[];
};
";

fn compute_bounding_box<I>(clp_to_hmd: I) -> BoundingBox<f64>
where
    I: IntoIterator<Item = Matrix4<f64>>,
{
    let corners_in_clp = Frustrum::corners_in_clp(DEPTH_RANGE);
    let mut corners_in_hmd = clp_to_hmd
        .into_iter()
        .flat_map(|clp_to_hmd| corners_in_clp.into_iter().map(move |&p| clp_to_hmd.transform_point(p)))
        .map(|p| p.cast::<f64>().unwrap());
    let first = BoundingBox::from_point(corners_in_hmd.next().unwrap());
    corners_in_hmd.fold(first, |b, p| b.enclose(p))
}

#[derive(Debug)]
pub struct ClusterData {
    pub dimensions: Vector3<u32>,

    pub trans_from_cls_to_hmd: Vector3<f64>,
    pub trans_from_hmd_to_cls: Vector3<f64>,

    pub scale_from_cls_to_hmd: Vector3<f64>,
    pub scale_from_hmd_to_cls: Vector3<f64>,

    pub cls_to_wld: Matrix4<f64>,
    pub wld_to_cls: Matrix4<f64>,
}

impl ClusterData {
    pub fn new<I>(
        cfg: &configuration::ClusteredLightShading,
        clp_to_hmd: I,
        wld_to_hmd: Matrix4<f64>,
        hmd_to_wld: Matrix4<f64>,
    ) -> Self
    where
        I: IntoIterator<Item = Matrix4<f64>>,
    {
        let bb = compute_bounding_box(clp_to_hmd);

        let bb_delta = bb.delta();
        let mut dimensions = (bb_delta / cfg.cluster_side as f64).map(f64::ceil);

        // TODO: Warn?
        if dimensions.x > 256.0 {
            dimensions.x = 256.0
        }

        if dimensions.y > 256.0 {
            dimensions.y = 256.0
        }

        if dimensions.z > 256.0 {
            dimensions.z = 256.0
        }

        let trans_from_hmd_to_cls = Point3::origin() - bb.min;
        let trans_from_cls_to_hmd = bb.min - Point3::origin();

        let scale_from_cls_to_hmd = bb_delta.div_element_wise(dimensions);
        let scale_from_hmd_to_cls = dimensions.div_element_wise(bb_delta);

        let cls_to_hmd: Matrix4<f64> =
            Matrix4::from_translation(trans_from_cls_to_hmd) * Matrix4::from_scale_vector(scale_from_cls_to_hmd);
        let hmd_to_cls: Matrix4<f64> =
            Matrix4::from_scale_vector(scale_from_hmd_to_cls) * Matrix4::from_translation(trans_from_hmd_to_cls);

        Self {
            dimensions: dimensions.cast().unwrap(),

            trans_from_cls_to_hmd,
            trans_from_hmd_to_cls,

            scale_from_cls_to_hmd,
            scale_from_hmd_to_cls,

            cls_to_wld: hmd_to_wld * cls_to_hmd,
            wld_to_cls: hmd_to_cls * wld_to_hmd,
        }
    }

    pub fn cluster_count(&self) -> u32 {
        self.dimensions.product()
    }
}

pub struct ClusterCameraData {
    // Depth pass.
    pub frame_dims: Vector2<i32>,

    pub wld_to_cam: Matrix4<f64>,
    pub cam_to_wld: Matrix4<f64>,

    pub cam_to_clp: Matrix4<f64>,
    pub clp_to_cam: Matrix4<f64>,

    // Cluster orientation and dimensions.
    pub wld_to_hmd: Matrix4<f64>,
    pub hmd_to_wld: Matrix4<f64>,

    pub hmd_to_clp: Matrix4<f64>,
    pub clp_to_hmd: Matrix4<f64>,
}

pub struct ClusterCameraResources {
    pub profilers: CameraStages<Profiler>,
}

impl_enum_and_enum_map! {
    #[derive(Debug, Copy, Clone, Eq, PartialEq, EnumNext)]
    enum CameraStage => struct CameraStages {
        RenderDepth => render_depth,
        CountFrags => count_frags,
    }
}

impl CameraStage {
    pub const VALUES: [CameraStage; 2] = [CameraStage::RenderDepth, CameraStage::CountFrags];

    pub fn title(self) -> &'static str {
        match self {
            CameraStage::RenderDepth => "render depth",
            CameraStage::CountFrags => "count #frags",
        }
    }
}

pub struct ClusterResources {
    pub cluster_fragment_counts_buffer: DynamicBuffer,
    // pub cluster_metas_buffer: DynamicBuffer,
    pub active_cluster_indices_buffer: DynamicBuffer,
    pub active_cluster_light_counts_buffer: DynamicBuffer,
    pub active_cluster_light_offsets_buffer: DynamicBuffer,
    pub light_xyzr_buffer: DynamicBuffer,
    pub offset_buffer: DynamicBuffer,
    pub draw_command_buffer: DynamicBuffer,
    pub compute_commands_buffer: DynamicBuffer,
    pub light_indices_buffer: DynamicBuffer,
    pub profilers: ClusterStages<Profiler>,
    pub camera_data: Vec<ClusterCameraData>,
    pub camera_res: Vec<ClusterCameraResources>,
    pub cluster_lengths: Vec<u32>,
    pub cluster_meta: Vec<ClusterMeta>,
    pub light_indices: Vec<u32>,
}

impl_enum_and_enum_map! {
    #[derive(Debug, Copy, Clone, Eq, PartialEq, EnumNext)]
    enum ClusterStage => struct ClusterStages {
        CompactClusters => compact_clusters,
        UploadLights => upload_lights,
        CountLights => count_lights,
        LightOffsets => light_offsets,
        AssignLights => assign_lights,
    }
}

impl ClusterStage {
    pub const VALUES: [ClusterStage; 5] = [
        ClusterStage::CompactClusters,
        ClusterStage::UploadLights,
        ClusterStage::CountLights,
        ClusterStage::LightOffsets,
        ClusterStage::AssignLights,
    ];

    pub fn title(self) -> &'static str {
        match self {
            ClusterStage::CompactClusters => "compact clusters",
            ClusterStage::UploadLights => "upload lights",
            ClusterStage::CountLights => "count lights",
            ClusterStage::LightOffsets => "comp light offs",
            ClusterStage::AssignLights => "assign lights",
        }
    }
}

#[repr(C)]
pub struct ClusterMeta {
    light_index_count: u32,
    light_index_offset: u32,
}

impl ClusterResources {
    pub fn new(gl: &gl::Gl, cfg: &configuration::ClusteredLightShading) -> Self {
        Self {
            cluster_fragment_counts_buffer: unsafe {
                let mut buffer = Buffer::new(gl);
                gl.buffer_label(&buffer, "cluster_fragment_counts");
                buffer.ensure_capacity(gl, std::mem::size_of::<u32>() * cfg.max_cluster_count as usize);
                buffer
            },
            active_cluster_indices_buffer: unsafe {
                let mut buffer = Buffer::new(gl);
                gl.buffer_label(&buffer, "active_cluster_indices");
                buffer.ensure_capacity(gl, std::mem::size_of::<u32>() * cfg.max_active_cluster_count as usize);
                buffer
            },
            active_cluster_light_counts_buffer: unsafe {
                let mut buffer = Buffer::new(gl);
                gl.buffer_label(&buffer, "active_cluster_light_counts");
                buffer.ensure_capacity(gl, std::mem::size_of::<u32>() * cfg.max_active_cluster_count as usize);
                buffer
            },
            active_cluster_light_offsets_buffer: unsafe {
                let mut buffer = Buffer::new(gl);
                gl.buffer_label(&buffer, "active_cluster_light_offsets");
                buffer.ensure_capacity(gl, std::mem::size_of::<u32>() * cfg.max_active_cluster_count as usize);
                buffer
            },
            light_xyzr_buffer: unsafe {
                let mut buffer = Buffer::new(gl);
                gl.buffer_label(&buffer, "light_xyrz");
                buffer
            },
            offset_buffer: unsafe {
                let mut buffer = Buffer::new(gl);
                gl.buffer_label(&buffer, "offsets");
                buffer
            },
            draw_command_buffer: unsafe {
                let mut buffer = Buffer::new(gl);
                gl.buffer_label(&buffer, "draw_comands");
                let data = rendering::DrawCommand {
                    count: 36,
                    prim_count: 0,
                    first_index: 0,
                    base_vertex: 0,
                    base_instance: 0,
                };
                buffer.ensure_capacity(gl, data.value_as_bytes().len());
                buffer.write(gl, data.value_as_bytes());
                buffer
            },
            profilers: ClusterStages::new(|_| Profiler::new(gl)),
            compute_commands_buffer: unsafe {
                let mut buffer = Buffer::new(gl);
                gl.buffer_label(&buffer, "compute_commands");
                let data = vec![
                    rendering::ComputeCommand {
                        work_group_x: 0,
                        work_group_y: 1,
                        work_group_z: 1,
                    },
                    rendering::ComputeCommand {
                        work_group_x: 0,
                        work_group_y: 1,
                        work_group_z: 1,
                    },
                ];
                buffer.ensure_capacity(gl, data.vec_as_bytes().len());
                buffer.write(gl, data.vec_as_bytes());
                buffer
            },
            light_indices_buffer: unsafe {
                let mut buffer = Buffer::new(gl);
                gl.buffer_label(&buffer, "light_indices");
                buffer.ensure_capacity(gl, std::mem::size_of::<u32>() * cfg.max_light_index_count as usize);
                buffer
            },
            camera_data: Vec::new(),
            camera_res: Vec::new(),
            cluster_lengths: Vec::new(),
            cluster_meta: Vec::new(),
            light_indices: Vec::new(),
        }
    }

    pub fn add_camera(&mut self, gl: &gl::Gl, data: ClusterCameraData) -> usize {
        let index = self.camera_data.len();
        if self.camera_res.len() <= index {
            self.camera_res.push(ClusterCameraResources {
                profilers: CameraStages::new(|_| Profiler::new(gl)),
            });
        }
        self.camera_data.push(data);
        index
    }

    pub fn clear(&mut self) {
        self.camera_data.clear();
    }
}

// Ballpark numbers
//
// light count
// L = 1_000_000
//
// window dimensions
// WX = 1920
// WY = 1080
//
// cluster dimensions
// CX = 400
// CY = 200
// CZ = 200
//
// active clusters
// CA ~ WX*WY/pixels per cluster (16) (depends on geometry, window dimensions, cluster dimensions)
// CA = 130_000
//
// total light indices
// LI ~ CA*lights per cluster (32)
// LI = 4_000_000 (a bit much isn't it)

// 1.1. (light_xyzr_wld_buffer[L]) upload light [x, y, z]_wld in world space.
// 1.2. (light_xyzr_cls_buffer[L]) compute [[x, y, z]_cls | r_ wld] using wld_to_cls.

// 2.1. (depth_buffer[WX, WH]) render depth W*H [z_wld]
// 2.2. (active_clusters[CX, CY, CZ]) compute W*H [active|inactive] clusters.
// 2.3. (active_cluster_ids[CA]) prefix sum active clusters to get offsets, write cluster id.

// 3.1. (cluster_lengths[CA]) intersect active clusters with lights and count.
// 3.2. (light_indices[LI]) prefix sum cluster_lengths to get offsets, write light id.

impl ClusterResources {
    pub fn compute_and_upload(
        &mut self,
        gl: &gl::Gl,
        cfg: &configuration::ClusteredLightShading,
        space: &ClusterData,
        point_lights: &[light::PointLight],
    ) {
        // self.cpu_start = Some(Instant::now());

        // let ClusterData {
        //     dimensions,
        //     scale_from_cls_to_hmd,
        //     scale_from_hmd_to_cls,
        //     wld_to_cls,
        //     cls_to_wld,
        //     ..
        // } = *space;

        // let dimensions_u32 = dimensions.cast::<u32>().unwrap();
        // let dimensions = dimensions.cast::<f64>().unwrap();

        // let cluster_count = dimensions_u32.product();

        // // First pass, compute cluster lengths and offsets.
        // self.cluster_lengths.clear();
        // self.cluster_lengths
        //     .resize_with(cluster_count as usize, Default::default);

        // for (i, l) in point_lights.iter().enumerate() {
        //     if let Some(light_index) = cfg.light_index {
        //         if i as u32 != light_index {
        //             continue;
        //         }
        //     }

        //     let pos_in_cls = wld_to_cls.transform_point(l.pos_in_wld.cast::<f64>().unwrap());

        //     let r = l.attenuation.clip_far as f64;
        //     let r_sq = r * r;

        //     let minima = Point3::partial_clamp_element_wise(
        //         (pos_in_cls - scale_from_hmd_to_cls * r).map(f64::floor),
        //         Point3::origin(),
        //         Point3::from_vec(dimensions),
        //     )
        //     .map(|e| e as u32);

        //     let centers = Point3::partial_clamp_element_wise(
        //         (pos_in_cls).map(f64::floor),
        //         Point3::origin(),
        //         Point3::from_vec(dimensions),
        //     )
        //     .map(|e| e as u32);

        //     let maxima = Point3::partial_clamp_element_wise(
        //         (pos_in_cls + scale_from_hmd_to_cls * r).map(f64::ceil),
        //         Point3::origin(),
        //         Point3::from_vec(dimensions),
        //     )
        //     .map(|e| e as u32);

        //     let Point3 { x: x0, y: y0, z: z0 } = minima;
        //     let Point3 { x: x1, y: y1, z: z1 } = centers;
        //     let Point3 { x: x2, y: y2, z: z2 } = maxima;

        //     // NOTE: We must clamp as f64 because the value might actually overflow.

        //     macro_rules! closest_face_dist {
        //         ($x: ident, $x1: ident, $pos: ident) => {
        //             if $x < $x1 {
        //                 ($x + 1) as f64 - $pos.$x
        //             } else if $x > $x1 {
        //                 $x as f64 - $pos.$x
        //             } else {
        //                 0.0
        //             }
        //         };
        //     }

        //     for z in z0..z2 {
        //         let dz = closest_face_dist!(z, z1, pos_in_cls) * scale_from_cls_to_hmd.z;
        //         for y in y0..y2 {
        //             let dy = closest_face_dist!(y, y1, pos_in_cls) * scale_from_cls_to_hmd.y;
        //             for x in x0..x2 {
        //                 let dx = closest_face_dist!(x, x1, pos_in_cls) * scale_from_cls_to_hmd.x;
        //                 if dz * dz + dy * dy + dx * dx < r_sq {
        //                     // It's a hit!
        //                     let index = ((z * dimensions_u32.y) + y) * dimensions_u32.x + x;
        //                     self.cluster_lengths[index as usize] += 1;
        //                 }
        //             }
        //         }
        //     }
        // }

        // let total_light_indices: u64 = self.cluster_lengths.iter().map(|&x| x as u64).sum();

        // // Scan cluster offsets from lengths.
        // self.cluster_meta.clear();
        // self.cluster_meta.reserve(cluster_count as usize);

        // self.cluster_meta
        //     .extend(self.cluster_lengths.iter().scan(0, |offset, &length| {
        //         let meta = ClusterMeta {
        //             offset: *offset,
        //             length: length,
        //         };
        //         *offset += length;
        //         Some(meta)
        //     }));

        // // Second pass
        // self.cluster_lengths.clear();
        // self.cluster_lengths
        //     .resize_with(cluster_count as usize, Default::default);
        // self.light_indices
        //     .resize_with(total_light_indices as usize, Default::default);

        // for (i, l) in point_lights.iter().enumerate() {
        //     if let Some(light_index) = cfg.light_index {
        //         if i as u32 != light_index {
        //             continue;
        //         }
        //     }

        //     let pos_in_cls = wld_to_cls.transform_point(l.pos_in_wld.cast::<f64>().unwrap());

        //     let r = l.attenuation.clip_far as f64;
        //     let r_sq = r * r;

        //     let minima = Point3::partial_clamp_element_wise(
        //         (pos_in_cls - scale_from_hmd_to_cls * r).map(f64::floor),
        //         Point3::origin(),
        //         Point3::from_vec(dimensions),
        //     )
        //     .map(|e| e as u32);

        //     let centers = Point3::partial_clamp_element_wise(
        //         (pos_in_cls).map(f64::floor),
        //         Point3::origin(),
        //         Point3::from_vec(dimensions),
        //     )
        //     .map(|e| e as u32);

        //     let maxima = Point3::partial_clamp_element_wise(
        //         (pos_in_cls + scale_from_hmd_to_cls * r).map(f64::ceil),
        //         Point3::origin(),
        //         Point3::from_vec(dimensions),
        //     )
        //     .map(|e| e as u32);

        //     let Point3 { x: x0, y: y0, z: z0 } = minima;
        //     let Point3 { x: x1, y: y1, z: z1 } = centers;
        //     let Point3 { x: x2, y: y2, z: z2 } = maxima;

        //     // NOTE: We must clamp as f64 because the value might actually overflow.

        //     macro_rules! closest_face_dist {
        //         ($x: ident, $x1: ident, $pos: ident) => {
        //             if $x < $x1 {
        //                 ($x + 1) as f64 - $pos.$x
        //             } else if $x > $x1 {
        //                 $x as f64 - $pos.$x
        //             } else {
        //                 0.0
        //             }
        //         };
        //     }

        //     for z in z0..z2 {
        //         let dz = closest_face_dist!(z, z1, pos_in_cls) * scale_from_cls_to_hmd.z;
        //         for y in y0..y2 {
        //             let dy = closest_face_dist!(y, y1, pos_in_cls) * scale_from_cls_to_hmd.y;
        //             for x in x0..x2 {
        //                 let dx = closest_face_dist!(x, x1, pos_in_cls) * scale_from_cls_to_hmd.x;
        //                 if dz * dz + dy * dy + dx * dx < r_sq {
        //                     // It's a hit!
        //                     let cluster_index = ((z * dimensions_u32.y) + y) * dimensions_u32.x + x;
        //                     let light_offset = self.cluster_lengths[cluster_index as usize];
        //                     self.cluster_lengths[cluster_index as usize] += 1;

        //                     let ClusterMeta {
        //                         offset: cluster_offset,
        //                         length: cluster_len,
        //                     } = self.cluster_meta[cluster_index as usize];
        //                     debug_assert!(light_offset < cluster_len);

        //                     self.light_indices[(cluster_offset + light_offset) as usize] = i as u32;
        //                 }
        //             }
        //         }
        //     }
        // }
        // FIXME
        // unsafe {
        //     let header = ClusterHeader {
        //         dimensions: dimensions_u32.extend(0),
        //         wld_to_cls: wld_to_cls.cast().unwrap(),
        //         cls_to_wld: cls_to_wld.cast().unwrap(),
        //     };

        //     let header_bytes = header.value_as_bytes();
        //     let header_bytes_offset = 0;
        //     let meta_bytes = self.cluster_meta.vec_as_bytes();
        //     let meta_bytes_offset = header_bytes_offset + header_bytes.len();
        //     let light_indices_bytes = self.light_indices.vec_as_bytes();
        //     let light_indices_bytes_offset = meta_bytes_offset + meta_bytes.len();
        //     let total_byte_count = light_indices_bytes_offset + light_indices_bytes.len();

        //     gl.named_buffer_reserve(self.buffer_name, total_byte_count, gl::STREAM_DRAW);
        //     gl.named_buffer_sub_data(self.buffer_name, header_bytes_offset, header_bytes);
        //     gl.named_buffer_sub_data(self.buffer_name, meta_bytes_offset, meta_bytes);
        //     gl.named_buffer_sub_data(self.buffer_name, light_indices_bytes_offset, light_indices_bytes);
        //     gl.bind_buffer_base(gl::SHADER_STORAGE_BUFFER, CLUSTER_BUFFER_BINDING, self.buffer_name);
        // }
        // self.cpu_end = Some(Instant::now());
    }
}

#[derive(Debug)]
struct GlobalClusterResources {
    pub fragments_per_cluster_program: ProgramName,
    pub compress_active_clusters_program: ProgramName,
}
