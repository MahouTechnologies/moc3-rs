use binrw::BinReaderExt;
use puppet::{puppet_from_moc3, Puppet};
use renderer::wgpu::new_renderer;
use std::fs::File;
use std::io::BufReader;
use wgpu::{CompositeAlphaMode, TextureFormat};
use winit::{event::Event, event_loop::EventLoop, window::WindowBuilder};

use crate::puppet::PuppetFrameData;

mod data;
mod deformer;
mod interpolate;
mod puppet;
mod renderer;

fn main() {
    let f = File::open("test.moc3").unwrap();
    let mut reader = BufReader::new(f);
    let read: data::Moc3Data = reader.read_le().unwrap();

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

            renderer.prepare(&device, &queue, &puppet, &frame_data);
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            renderer.render(&view, &mut encoder, &frame_data);
            queue.submit(std::iter::once(encoder.finish()));

            output.present();
        }
        Event::MainEventsCleared => {
            window.request_redraw();
        }
        _ => {}
    });
}

// Potentially useful, should be moved to actual docs

// ```dot
// digraph G {
//     D [label="ArtMesh"]
//     DK [label="ArtMesh Keyform"]

//     P [label="Parameter"];
//     PBI [label="Parameter Binding Index"];
//     PB [label="Parameter Binding"];
//     KI [label="Key Index"];

//     PMaxV [label="Parameter Max Value"]
//     PMinV [label="Parameter Min Value"]
//     PDefV [label="Parameter Default Value"]

//     K [label="Key"];

//     KB [label="Keyform Binding"];
//     KPI [label="Keyform Position Index"]
//     KP [label="Keyform Position"]

//     VC [label="Vertex Counts"]
//     TN [label="Texture Number"]

//     D -> DK [style="dashed"];
//     D -> VC;
//     D -> KB;
//     D -> TN;

//     D -> PBI;
//     DK -> KPI;
//     KPI -> KP [style="dotted"];

//     P -> PB [style="dashed"];
//     P -> PMaxV;
//     P -> PMinV;
//     P -> PDefV;

//     KB -> PBI [style="dashed"];

//     PBI -> PB [style="dotted"];
//     PB -> KI;
//     KI -> K [style="dotted"];

//     De [label="Deformer"];
//     WDe [label="Warp Deformer"];
//     RDe [label="Rotation Deformer"];
//     WdeK [label="Warp Deformer Keyform"];
//     RdeK [label="Rotation Deformer Keyform"];

//     De -> WDe;
//     De -> RDe;
//     // De -> KB; this exists but is inconvient to deal with / graph
//     WDe -> KB;
//     WDe -> WdeK [style="dashed"];
//     WdeK -> KPI;
//     RDe -> KB;
//     RDe -> RdeK [style="dashed"];

//     XOri [label="X Origin"];
//     YOri [label="Y Origin"]
//     XRef [label="X Reflect"]
//     YRef [label="Y Reflect"]
//     Ang [label="Angle"]

//     RdeK -> XOri;
//     RdeK -> YOri;
//     RdeK -> Ang;
//     RdeK -> XRef;
//     RdeK -> YRef;
// }
// ```
