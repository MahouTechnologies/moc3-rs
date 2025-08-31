use bytemuck::cast_slice;
use encase::{ShaderSize, ShaderType, UniformBuffer};
use glam::{Mat4, Vec2, Vec3};
use image::RgbaImage;
use util::TextureDataOrder;
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    *,
};

use moc3_rs::{
    data::{ArtMeshFlags, BlendMode},
    puppet::{Puppet, PuppetFrameData},
};

#[derive(ShaderType, Debug, Clone, Copy, PartialEq)]
struct Uniform {
    pub multiply_color: Vec3,
    pub screen_color: Vec3,
    pub opacity: f32,
}

pub struct Renderer {
    mesh_flags: Vec<ArtMeshFlags>,
    texture_nums: Vec<u32>,
    render_orders: Vec<u32>,
    mask_indices: Vec<Vec<u32>>,

    // blend mode first, then double-sided
    pipeline: [[RenderPipeline; 3]; 2],
    // just double-sided here
    mask_pipeline: [RenderPipeline; 2],

    bound_textures: Vec<BindGroup>,
    uniform_bind_group: BindGroup,
    uniform_alignment_needed: u64,

    camera_buffer: Buffer,
    uniform_buffer: Buffer,

    uv_buffers: Vec<Buffer>,
    index_buffers: Vec<Buffer>,
    vertex_buffers: Vec<Buffer>,

    mask_stencil: Option<Texture>,
}

impl Renderer {
    pub fn prepare(
        &mut self,
        device: &Device,
        queue: &Queue,
        render_size: Extent3d,
        frame_data: &PuppetFrameData,
    ) {
        if let Some(texture) = &mut self.mask_stencil {
            if texture.size() != render_size {
                self.mask_stencil = None;
            }
        }

        self.mask_stencil.get_or_insert_with(|| {
            device.create_texture(&wgpu::TextureDescriptor {
                size: render_size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth24PlusStencil8,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
                label: None,
            })
        });

        self.render_orders[..].copy_from_slice(&frame_data.art_mesh_render_orders);
        for (i, data) in frame_data.art_mesh_data.iter().enumerate() {
            queue.write_buffer(&self.vertex_buffers[i], 0, cast_slice(data.as_slice()));
        }

        queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&[Mat4::IDENTITY]),
        );

        for i in 0..self.texture_nums.len() {
            let uniform = Uniform {
                multiply_color: frame_data.art_mesh_colors[i].multiply_color,
                screen_color: frame_data.art_mesh_colors[i].screen_color,
                opacity: frame_data.art_mesh_opacities[i],
            };

            let mut buffer = UniformBuffer::new([0; Uniform::SHADER_SIZE.get() as usize]);
            buffer.write(&uniform).unwrap();
            queue.write_buffer(
                &self.uniform_buffer,
                self.uniform_alignment_needed * i as u64,
                buffer.as_ref(),
            );
        }
    }

    pub fn render(&mut self, view: &TextureView, encoder: &mut CommandEncoder) {
        let mask_view = self
            .mask_stencil
            .as_ref()
            .unwrap()
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
            color_attachments: &[Some(RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::TRANSPARENT),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &mask_view,
                depth_ops: None,
                stencil_ops: Some(Operations {
                    load: LoadOp::Clear(0),
                    store: StoreOp::Store,
                }),
            }),
            label: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        let mut cur_stencil_test_ref: u8 = 0;

        for art_index in self.render_orders.iter().copied() {
            let art_index = art_index as usize;
            let flags = self.mesh_flags[art_index];

            if self.index_buffers[art_index].size() == 0 {
                continue;
            }

            if self.mask_indices[art_index].is_empty() {
                // Because we use greater, no matter what the value of anything in the stencil buffer, this will work.
                rpass.set_stencil_reference(0);
            } else {
                cur_stencil_test_ref += 1;
                rpass.set_stencil_reference(cur_stencil_test_ref as u32);

                for mask_index in self.mask_indices[art_index].iter().copied() {
                    if mask_index == 4294967295 {
                        continue;
                    }
                    let mask_index = mask_index as usize;
                    let mask_flags = self.mesh_flags[mask_index];

                    rpass.set_pipeline(&self.mask_pipeline[mask_flags.double_sided() as usize]);

                    rpass.set_bind_group(
                        0,
                        &self.uniform_bind_group,
                        &[self.uniform_alignment_needed as u32 * mask_index as u32],
                    );
                    rpass.set_bind_group(
                        1,
                        &self.bound_textures[self.texture_nums[mask_index] as usize],
                        &[],
                    );
                    rpass.set_index_buffer(
                        self.index_buffers[mask_index].slice(..),
                        IndexFormat::Uint16,
                    );
                    rpass.set_vertex_buffer(0, self.vertex_buffers[mask_index].slice(..));
                    rpass.set_vertex_buffer(1, self.uv_buffers[mask_index].slice(..));

                    let x = self.index_buffers[mask_index].size() / 2;
                    rpass.draw_indexed(0..(x as u32), 0, 0..1);
                }

                if flags.inverted() {
                    rpass.set_stencil_reference(0);
                }
            }

            rpass.set_pipeline(
                &self.pipeline[flags.double_sided() as usize][flags.blend_mode() as usize],
            );

            rpass.set_bind_group(
                0,
                &self.uniform_bind_group,
                &[self.uniform_alignment_needed as u32 * art_index as u32],
            );
            rpass.set_bind_group(
                1,
                &self.bound_textures[self.texture_nums[art_index] as usize],
                &[],
            );
            rpass.set_index_buffer(self.index_buffers[art_index].slice(..), IndexFormat::Uint16);
            rpass.set_vertex_buffer(0, self.vertex_buffers[art_index].slice(..));
            rpass.set_vertex_buffer(1, self.uv_buffers[art_index].slice(..));

            let x = self.index_buffers[art_index].size() / 2;
            rpass.draw_indexed(0..(x as u32), 0, 0..1);
        }
    }
}

pub fn new_renderer(
    puppet: &Puppet,
    device: &Device,
    queue: &Queue,
    format: TextureFormat,
    textures: &[RgbaImage],
) -> Renderer {
    let texture_sampler = device.create_sampler(&SamplerDescriptor {
        min_filter: FilterMode::Linear,
        mag_filter: FilterMode::Linear,
        mipmap_filter: FilterMode::Linear,
        ..SamplerDescriptor::default()
    });

    let texture_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    multisampled: false,
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
        ],
        label: None,
    });

    let mut bound_textures = Vec::new();
    for tex in textures {
        let texture = device.create_texture_with_data(
            queue,
            &TextureDescriptor {
                size: Extent3d {
                    width: tex.width(),
                    height: tex.height(),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Rgba8Unorm,
                usage: TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
                label: None,
            },
            TextureDataOrder::default(),
            &tex,
        );

        let texture_view = texture.create_view(&TextureViewDescriptor::default());

        let bound_texture = device.create_bind_group(&BindGroupDescriptor {
            layout: &texture_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&texture_view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&texture_sampler),
                },
            ],
            label: None,
        });
        bound_textures.push(bound_texture);
    }

    let uniform_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: BufferSize::new(std::mem::size_of::<Mat4>() as u64),
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: Some(Uniform::SHADER_SIZE),
                },
                count: None,
            },
        ],
        label: None,
    });

    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        bind_group_layouts: &[&uniform_layout, &texture_layout],
        ..PipelineLayoutDescriptor::default()
    });

    let pipeline = [
        [
            pipeline_for(
                device,
                None,
                &pipeline_layout,
                format,
                false,
                PipelineKind::Render(BlendMode::Normal),
            ),
            pipeline_for(
                device,
                None,
                &pipeline_layout,
                format,
                false,
                PipelineKind::Render(BlendMode::Additive),
            ),
            pipeline_for(
                device,
                None,
                &pipeline_layout,
                format,
                false,
                PipelineKind::Render(BlendMode::Multiplicative),
            ),
        ],
        [
            pipeline_for(
                device,
                None,
                &pipeline_layout,
                format,
                true,
                PipelineKind::Render(BlendMode::Normal),
            ),
            pipeline_for(
                device,
                None,
                &pipeline_layout,
                format,
                true,
                PipelineKind::Render(BlendMode::Additive),
            ),
            pipeline_for(
                device,
                None,
                &pipeline_layout,
                format,
                true,
                PipelineKind::Render(BlendMode::Multiplicative),
            ),
        ],
    ];

    let mask_pipeline = [
        pipeline_for(
            device,
            None,
            &pipeline_layout,
            format,
            false,
            PipelineKind::Mask,
        ),
        pipeline_for(
            device,
            None,
            &pipeline_layout,
            format,
            true,
            PipelineKind::Mask,
        ),
    ];

    let camera_buffer = device.create_buffer(&BufferDescriptor {
        size: std::mem::size_of::<Mat4>() as u64,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
        label: None,
    });

    let min_uniform_alignment = device.limits().min_uniform_buffer_offset_alignment;
    let uniform_alignment_needed = Uniform::SHADER_SIZE.get().max(min_uniform_alignment as u64);

    let uniform_buffer = device.create_buffer(&BufferDescriptor {
        size: uniform_alignment_needed * puppet.art_mesh_count as u64,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
        label: None,
    });

    let uniform_bind_group = device.create_bind_group(&BindGroupDescriptor {
        layout: &uniform_layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::Buffer(BufferBinding {
                    buffer: &uniform_buffer,
                    offset: 0,
                    size: Some(Uniform::SHADER_SIZE),
                }),
            },
        ],
        label: None,
    });

    // TODO: this is dumb - blot it into a single buffer instead
    let mut uv_buffers = Vec::with_capacity(puppet.art_mesh_count as usize);
    for buf in &puppet.art_mesh_uvs {
        let uv_buffer = device.create_buffer_init(&BufferInitDescriptor {
            contents: bytemuck::cast_slice(&buf.as_slice()),
            usage: BufferUsages::VERTEX,
            label: None,
        });
        uv_buffers.push(uv_buffer);
    }
    let mut index_buffers = Vec::with_capacity(puppet.art_mesh_count as usize);
    for buf in &puppet.art_mesh_indices {
        let index_buffer = device.create_buffer_init(&BufferInitDescriptor {
            contents: bytemuck::cast_slice(&buf.as_slice()),
            usage: BufferUsages::INDEX,
            label: None,
        });
        index_buffers.push(index_buffer);
    }

    let mut vertex_buffers = Vec::with_capacity(puppet.art_mesh_count as usize);
    for len in &puppet.art_mesh_vertexes {
        let vertex_buffer = device.create_buffer(&BufferDescriptor {
            size: ((*len as usize) * std::mem::size_of::<Vec2>()) as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            label: None,
            mapped_at_creation: false,
        });
        vertex_buffers.push(vertex_buffer);
    }

    Renderer {
        mesh_flags: puppet.art_mesh_flags.clone(),
        texture_nums: puppet.art_mesh_textures.clone(),
        render_orders: vec![0; puppet.art_mesh_count as usize],
        mask_indices: puppet.art_mesh_mask_indices.clone(),

        pipeline,
        mask_pipeline,

        bound_textures,
        uniform_bind_group,
        uniform_alignment_needed,

        camera_buffer,
        uniform_buffer,

        uv_buffers,
        index_buffers,
        vertex_buffers,

        mask_stencil: None,
    }
}

enum PipelineKind {
    Render(BlendMode),
    Mask,
}

fn pipeline_for(
    device: &Device,
    label: Label<'_>,
    layout: &PipelineLayout,
    texture_format: TextureFormat,
    double_sided: bool,
    kind: PipelineKind,
) -> RenderPipeline {
    let face_state = match kind {
        PipelineKind::Render(_) => StencilFaceState {
            compare: CompareFunction::LessEqual,
            fail_op: StencilOperation::Keep,
            depth_fail_op: StencilOperation::Keep,
            pass_op: StencilOperation::Keep,
        },
        PipelineKind::Mask => StencilFaceState {
            compare: CompareFunction::Always,
            fail_op: StencilOperation::Replace,
            depth_fail_op: StencilOperation::Replace,
            pass_op: StencilOperation::Replace,
        },
    };

    let stencil = StencilState {
        front: face_state,
        back: face_state,
        read_mask: 0xff,
        write_mask: 0xff,
    };

    let (blend, write_mask) = match kind {
        PipelineKind::Render(blend_mode) => {
            let blend = match blend_mode {
                BlendMode::Normal => BlendState::PREMULTIPLIED_ALPHA_BLENDING,
                BlendMode::Additive => BlendState {
                    color: BlendComponent {
                        src_factor: BlendFactor::One,
                        dst_factor: BlendFactor::One,
                        operation: BlendOperation::Add,
                    },
                    alpha: BlendComponent {
                        src_factor: BlendFactor::Zero,
                        dst_factor: BlendFactor::One,
                        operation: BlendOperation::Add,
                    },
                },
                BlendMode::Multiplicative => BlendState {
                    color: BlendComponent {
                        src_factor: BlendFactor::Dst,
                        dst_factor: BlendFactor::OneMinusSrcAlpha,
                        operation: BlendOperation::Add,
                    },
                    alpha: BlendComponent {
                        src_factor: BlendFactor::Zero,
                        dst_factor: BlendFactor::One,
                        operation: BlendOperation::Add,
                    },
                },
            };

            (Some(blend), ColorWrites::ALL)
        }
        PipelineKind::Mask => (None, ColorWrites::empty()),
    };

    device.create_render_pipeline(&RenderPipelineDescriptor {
        label,
        layout: Some(layout),
        fragment: Some(FragmentState {
            module: &device.create_shader_module(match kind {
                PipelineKind::Render(_) => include_wgsl!("./shader/frag.wgsl"),
                PipelineKind::Mask => include_wgsl!("./shader/mask.frag.wgsl"),
            }),
            entry_point: Some("fs_main"),
            targets: &[Some(ColorTargetState {
                format: texture_format,
                blend,
                write_mask,
            })],
            compilation_options: PipelineCompilationOptions::default(),
        }),
        vertex: VertexState {
            module: &device.create_shader_module(include_wgsl!("./shader/vert.wgsl")),
            entry_point: Some("vs_main"),
            buffers: &[
                VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vec2>() as BufferAddress,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &vertex_attr_array![0 => Float32x2],
                },
                VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vec2>() as BufferAddress,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &vertex_attr_array![1 => Float32x2],
                },
            ],
            compilation_options: PipelineCompilationOptions::default(),
        },
        primitive: PrimitiveState {
            front_face: FrontFace::Cw,
            cull_mode: if double_sided { None } else { Some(Face::Back) },
            ..PrimitiveState::default()
        },
        depth_stencil: Some(DepthStencilState {
            format: TextureFormat::Depth24PlusStencil8,
            depth_write_enabled: false,
            depth_compare: CompareFunction::Always,
            stencil,
            bias: DepthBiasState::default(),
        }),
        multisample: MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}
