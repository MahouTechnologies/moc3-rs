use binrw::BinReaderExt;
use moc3_rs::{
    data::Moc3Data,
    puppet::{framedata_for_puppet, puppet_from_moc3, Puppet, PuppetFrameData},
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
    drop(read);

    let frame_data = framedata_for_puppet(&puppet);

    pollster::block_on(run(puppet, frame_data));
}

pub async fn run(puppet: Puppet, mut frame_data: PuppetFrameData) {
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
    let params = puppet.param_data().defaults.clone();
    // Somehow the Close button doesn't work... Figure that out
    event_loop.run(move |event, _, _| match event {
        Event::RedrawRequested(_) => {
            let output = surface.get_current_texture().unwrap();
            let view = (output.texture).create_view(&wgpu::TextureViewDescriptor::default());

            puppet.update(&params, &mut frame_data);

            renderer.prepare(&device, &queue, output.texture.size(), &frame_data);
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
