use binrw::BinReaderExt;
use image::RgbaImage;
use moc3_rs::{
    data::Moc3Data,
    puppet::{framedata_for_puppet, puppet_from_moc3, Puppet, PuppetFrameData},
};
use moc3_wgpu::renderer::new_renderer;
use rand::Rng;
use std::{fs::File, time::Instant};
use std::{io::BufReader, sync::Arc};
use wgpu::{CompositeAlphaMode, TextureFormat};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

fn main() {
    let f = File::open("a.moc3").unwrap();
    let mut reader = BufReader::new(f);
    let read: Moc3Data = reader.read_le().unwrap();

    let puppet = puppet_from_moc3(&read);
    drop(read);

    let frame_data = framedata_for_puppet(&puppet);

    let get_img = |a: &str| {
        image::io::Reader::open(a)
            .unwrap()
            .decode()
            .unwrap()
            .into_rgba8()
    };

    let textures = vec![
        get_img("./a.4096/texture_00.png"),
        get_img("./a.4096/texture_01.png"),
        get_img("./a.4096/texture_02.png"),
        get_img("./a.4096/texture_03.png"),
        get_img("./a.4096/texture_04.png"),
        get_img("./a.4096/texture_05.png"),
        get_img("./a.4096/texture_06.png"),
        get_img("./a.4096/texture_07.png"),
    ];
    run(puppet, frame_data, textures);
}

struct App {
    window: Option<Arc<Window>>,
    gfx_state: Option<GfxState>,
    app: AppState,
}

impl App {
    pub fn new(app: AppState) -> Self {
        App {
            window: None,
            gfx_state: None,
            app,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = event_loop
            .create_window(
                Window::default_attributes()
                    .with_inner_size(PhysicalSize::new(1000, 1000))
                    .with_resizable(false)
                    .with_transparent(true)
                    .with_visible(true),
            )
            .unwrap();
        let window = Arc::new(window);
        self.gfx_state = Some(pollster::block_on(GfxState::new(
            window.clone(),
            &mut self.app,
        )));
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                self.app.update();
                if let Some(gfx_state) = &mut self.gfx_state {
                    gfx_state.paint(&mut self.app);
                    self.window.as_ref().unwrap().request_redraw();
                }
            }
            _ => {}
        }
    }
}

struct GfxState {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_config: wgpu::SurfaceConfiguration,
    surface: wgpu::Surface<'static>,
    renderer: moc3_wgpu::renderer::Renderer,
}

impl GfxState {
    async fn new(window: Arc<Window>, state: &mut AppState) -> Self {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(window.clone()).unwrap();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .unwrap();

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8Unorm,
            width: window.inner_size().width,
            height: window.inner_size().height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: CompositeAlphaMode::Auto,
            view_formats: Vec::new(),
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        let renderer = new_renderer(
            &state.puppet,
            &device,
            &queue,
            TextureFormat::Bgra8Unorm,
            &state.textures,
        );

        Self {
            device,
            queue,
            surface_config,
            surface,
            renderer,
        }
    }

    fn paint(&mut self, state: &mut AppState) {
        let output = self.surface.get_current_texture().unwrap();
        let view = (output.texture).create_view(&wgpu::TextureViewDescriptor::default());

        self.renderer.prepare(
            &self.device,
            &self.queue,
            output.texture.size(),
            &state.frame_data,
        );
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        self.renderer.render(&view, &mut encoder);
        self.queue.submit(std::iter::once(encoder.finish()));

        output.present();
    }
}

struct AppState {
    puppet: Puppet,
    frame_data: PuppetFrameData,
    start: Option<Instant>,
    params: Vec<f32>,
    opacities: Vec<f32>,
    sts: Vec<f32>,
    textures: Vec<image::ImageBuffer<image::Rgba<u8>, Vec<u8>>>,
}

impl AppState {
    pub fn update(&mut self) {
        let start = self.start.get_or_insert_with(|| Instant::now());
        let out = Instant::now();
        let t = (out - *start).as_secs_f32();
        for i in 0..self.params.len() {
            self.params[i] = (self.sts[i] + t).sin() * 0.5 + 0.5;
        }

        self.puppet
            .update(&self.params, &self.opacities, &mut self.frame_data);
    }
}

pub fn run(puppet: Puppet, frame_data: PuppetFrameData, textures: Vec<RgbaImage>) {
    let params = puppet.param_data().defaults.clone();
    let opacities = vec![1.0; puppet.part_count as usize];
    let mut sts = params.clone();
    let mut rng = rand::thread_rng();

    for i in sts.iter_mut() {
        *i = rng.gen();
    }

    let state = AppState {
        puppet,
        frame_data,
        params,
        opacities,
        sts,
        start: None,
        textures,
    };

    let mut app = App::new(state);

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut app).unwrap();
}
