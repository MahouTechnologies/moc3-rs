use binrw::BinReaderExt;
use moc3_rs::{
    data::Moc3Data,
    puppet::{puppet_from_moc3, Puppet, PuppetFrameData},
};
use moc3_wgpu::renderer::new_renderer;
use std::fs::File;
use std::io::BufReader;
use wgpu::{CompositeAlphaMode, TextureFormat};
use winit::{event::Event, event_loop::EventLoop, window::WindowBuilder};

fn main() {
    let f = File::open("test.moc3").unwrap();
    let mut reader = BufReader::new(f);
    let read: Moc3Data = reader.read_le().unwrap();

    let puppet = puppet_from_moc3(&read);

    let params = puppet.params.clone();

    println!("{:?}", params);

    let art_mesh_data = vec![Default::default(); read.table.count_info.art_meshes as usize];
    let warp_deformer_data =
        vec![Default::default(); read.table.count_info.warp_deformers as usize];
    let rotation_deformer_data =
        vec![Default::default(); read.table.count_info.rotation_deformers as usize];
    let glue_data = vec![Default::default(); read.table.count_info.glues as usize];

    let mut frame_data = PuppetFrameData {
        art_mesh_data,
        warp_deformer_data,
        rotation_deformer_data,
        art_mesh_render_orders: vec![0; read.table.count_info.art_meshes as usize],
        art_mesh_draw_orders: vec![500.0; read.table.count_info.art_meshes as usize],
        deformer_scale_data: vec![1.0; read.table.count_info.deformers as usize],
        art_mesh_opacities: vec![1.0; read.table.count_info.art_meshes as usize],
        warp_deformer_opacities: vec![1.0; read.table.count_info.warp_deformers as usize],
        rotation_deformer_opacities: vec![1.0; read.table.count_info.rotation_deformers as usize],

        art_mesh_colors: vec![Default::default(); read.table.count_info.art_meshes as usize],
        rotation_deformer_colors: vec![
            Default::default();
            read.table.count_info.rotation_deformers as usize
        ],
        warp_deformer_colors: vec![
            Default::default();
            read.table.count_info.warp_deformers as usize
        ],

        glue_data,
    };

    puppet.update(&params, &mut frame_data);

    pollster::block_on(run(puppet, frame_data));
}

pub async fn run(puppet: Puppet, frame_data: PuppetFrameData) {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_inner_size(winit::dpi::PhysicalSize::new(1000, 1000))
        .with_resizable(false)
        .with_transparent(true)
        .build(&event_loop)
        .unwrap();

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
    let surface = unsafe { instance.create_surface(&window).unwrap() };
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })
        .await
        .unwrap();

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                features: wgpu::Features::empty(),
                limits: wgpu::Limits::default(),
                label: None,
            },
            None,
        )
        .await
        .unwrap();

    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Bgra8Unorm,
        width: window.inner_size().width,
        height: window.inner_size().height,
        present_mode: wgpu::PresentMode::AutoVsync,
        alpha_mode: CompositeAlphaMode::Auto,
        view_formats: Vec::new(),
    };
    surface.configure(&device, &config);

    let img = image::io::Reader::open("texture.png")
        .unwrap()
        .decode()
        .unwrap()
        .into_rgba8();

    let mut renderer = new_renderer(&puppet, &device, &queue, TextureFormat::Bgra8Unorm, &[img]);

    // Somehow the Close button doesn't work... Figure that out
    event_loop.run(move |event, _, _| match event {
        Event::RedrawRequested(_) => {
            let output = surface.get_current_texture().unwrap();
            let view = (output.texture).create_view(&wgpu::TextureViewDescriptor::default());

            renderer.prepare(&device, &queue, &frame_data);
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            renderer.render(&view, &mut encoder);
            queue.submit(std::iter::once(encoder.finish()));

            output.present();
        }
        Event::MainEventsCleared => {
            window.request_redraw();
        }
        _ => {}
    });
}
