use bytemuck::cast_slice;
use encase::{ShaderSize, ShaderType, UniformBuffer};
use glam::{Mat4, Vec2, Vec3};
use image::RgbaImage;
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    *,
};

use moc3_rs::puppet::{Puppet, PuppetFrameData};

#[derive(ShaderType, Debug, Clone, Copy, PartialEq)]

struct Uniform {
    pub multiply_color: Vec3,
    pub screen_color: Vec3,
    pub opacity: f32,
}

pub struct Renderer {
    pipeline: RenderPipeline,
    texture_nums: Vec<u32>,
    render_orders: Vec<u32>,

    bound_textures: Vec<BindGroup>,
    uniform_bind_group: BindGroup,
    uniform_alignment_needed: u64,

    camera_buffer: Buffer,
    uniform_buffer: Buffer,

    uv_buffers: Vec<Buffer>,
    index_buffers: Vec<Buffer>,
    vertex_buffers: Vec<Buffer>,
}

impl Renderer {
    pub fn prepare(&mut self, _device: &Device, queue: &Queue, frame_data: &PuppetFrameData) {
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
                multiply_color: Vec3::ONE,
                screen_color: Vec3::ZERO,
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

    pub fn render(
        &mut self,
        view: &TextureView,
        encoder: &mut CommandEncoder,
        frame_data: &PuppetFrameData,
    ) {
        let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
            color_attachments: &[Some(RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLACK),
                    store: true,
                },
            })],
            depth_stencil_attachment: None,
            label: None,
        });
        rpass.set_pipeline(&self.pipeline);

        for i in frame_data.art_mesh_render_orders.iter().copied() {
            rpass.set_bind_group(
                0,
                &self.uniform_bind_group,
                &[self.uniform_alignment_needed as u32 * i],
            );

            let i = i as usize;
            rpass.set_bind_group(1, &self.bound_textures[self.texture_nums[i] as usize], &[]);
            rpass.set_index_buffer(self.index_buffers[i].slice(..), IndexFormat::Uint16);
            rpass.set_vertex_buffer(0, self.vertex_buffers[i].slice(..));
            rpass.set_vertex_buffer(1, self.uv_buffers[i].slice(..));

            let x = self.index_buffers[i].size() / 2;
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

    let pipeline = pipeline_for(device, None, &pipeline_layout, format);

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
    for len in &puppet.vertexes_count {
        let vertex_buffer = device.create_buffer(&BufferDescriptor {
            size: ((*len as usize) * std::mem::size_of::<Vec2>()) as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            label: None,
            mapped_at_creation: false,
        });
        vertex_buffers.push(vertex_buffer);
    }

    Renderer {
        pipeline,
        texture_nums: puppet.art_mesh_textures.clone(),
        render_orders: vec![0; puppet.art_mesh_count as usize],

        bound_textures,
        uniform_bind_group,
        uniform_alignment_needed,

        camera_buffer,
        uniform_buffer,

        uv_buffers,
        index_buffers,
        vertex_buffers,
    }
}

fn pipeline_for(
    device: &Device,
    label: Label<'_>,
    layout: &PipelineLayout,
    texture_format: TextureFormat,
) -> RenderPipeline {
    device.create_render_pipeline(&RenderPipelineDescriptor {
        label,
        layout: Some(layout),
        fragment: Some(FragmentState {
            module: &device.create_shader_module(include_wgsl!("./shader/frag.wgsl")),
            entry_point: "fs_main",
            targets: &[Some(ColorTargetState {
                format: texture_format,
                blend: Some(BlendState::ALPHA_BLENDING),
                write_mask: ColorWrites::ALL,
            })],
        }),
        vertex: VertexState {
            module: &device.create_shader_module(include_wgsl!("./shader/vert.wgsl")),
            entry_point: "vs_main",
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
        },
        primitive: PrimitiveState {
            cull_mode: None,
            ..PrimitiveState::default()
        },
        depth_stencil: None,
        multisample: MultisampleState::default(),
        multiview: None,
    })
}