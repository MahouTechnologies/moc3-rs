use image::RgbaImage;
use moc3_data::physics::Physics3Data;
use moc3_impressionism::PhysicsSystem;
use moc3_rs::{
    data::Moc3,
    puppet::{Puppet, PuppetFrameData, framedata_for_puppet, puppet_from_moc3},
};
use moc3_wgpu::renderer::new_renderer;
use rand::Rng;
use std::sync::Arc;
use std::{fs, time::Instant};
use wgpu::{CompositeAlphaMode, TextureFormat};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

fn main() {
    let puppet = {
        let bytes = fs::read("a.moc3").unwrap();
        let read = Moc3::new(&bytes).unwrap();
        puppet_from_moc3(read)
    };

    let frame_data = framedata_for_puppet(&puppet);

    let physics_json = fs::read_to_string("a.physics.json").unwrap();
    let physics_data: Physics3Data = serde_json::from_str(&physics_json).unwrap();
    let mut physics = PhysicsSystem::from_data(&physics_data, puppet.param_data());

    // Don't manually animate anything driven by physics.
    let param_count = puppet.param_data().count as usize;
    let mut physics_driven = vec![false; param_count];
    for idx in physics.output_param_indices() {
        physics_driven[idx] = true;
    }

    // Start from defaults and settle physics to a stable initial state.
    let mut params = puppet.param_data().defaults.clone();
    physics.fixpoint(&mut params);

    let get_img = |a: &str| {
        image::ImageReader::open(a)
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
    run(
        puppet,
        frame_data,
        textures,
        physics,
        physics_driven,
        params,
    );
}

struct App {
    gfx_state: Option<GfxState>,
    app: AppState,
    window: Option<Arc<Window>>,
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
    fn suspended(&mut self, _: &ActiveEventLoop) {
        self.gfx_state = None;
        self.window = None;
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let Ok(window) = event_loop.create_window(
            Window::default_attributes()
                .with_inner_size(PhysicalSize::new(1000, 1000))
                .with_resizable(true)
                .with_transparent(true)
                .with_visible(true),
        ) else {
            return;
        };
        let window = Arc::new(window);
        self.gfx_state = Some(pollster::block_on(GfxState::new(
            window.clone(),
            &mut self.app,
        )));
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        physical_key: winit::keyboard::PhysicalKey::Code(key_code),
                        ..
                    },
                ..
            } => {
                if matches!(key_code, winit::keyboard::KeyCode::Escape) {
                    event_loop.exit();
                }
            }
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
            WindowEvent::Resized(new_size) => {
                if let Some(gfx_state) = &mut self.gfx_state {
                    gfx_state.resize_surface(new_size.width, new_size.height);
                }
            }
            _ => {}
        }
    }
}

struct GfxState {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_config: wgpu::SurfaceConfiguration,
    renderer: moc3_wgpu::renderer::Renderer,
}

impl GfxState {
    async fn new(window: Arc<Window>, state: &mut AppState) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            ..wgpu::InstanceDescriptor::new_without_display_handle_from_env()
        });
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
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
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
            TextureFormat::Rgba8UnormSrgb,
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

    fn resize_surface(&mut self, width: u32, height: u32) {
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
    }

    fn paint(&mut self, state: &mut AppState) {
        let output = self.surface.get_current_texture();
        let surface_texture = match output {
            wgpu::CurrentSurfaceTexture::Success(texture) => texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                self.surface.configure(&self.device, &self.surface_config);
                texture
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.surface_config);
                return;
            }
            _ => return,
        };
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        self.renderer.prepare(
            &self.device,
            &self.queue,
            surface_texture.texture.size(),
            &state.frame_data,
        );
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        self.renderer.render(&view, &mut encoder);
        self.queue.submit(std::iter::once(encoder.finish()));

        surface_texture.present();
    }
}

struct AppState {
    puppet: Puppet,
    frame_data: PuppetFrameData,
    /// Absolute start time, used as the base for the animation sine wave.
    start: Instant,
    /// Time of the previous frame, used to compute `delta_seconds` for physics.
    last_update: Option<Instant>,
    params: Vec<f32>,
    opacities: Vec<f32>,
    /// Random phase offsets for the per-parameter sine animation.
    sts: Vec<f32>,
    textures: Vec<image::ImageBuffer<image::Rgba<u8>, Vec<u8>>>,
    physics: PhysicsSystem,
    /// `true` for each parameter index iff it is a physics output destination.
    physics_driven: Vec<bool>,
}

impl AppState {
    pub fn update(&mut self) {
        let now = Instant::now();
        let t = (now - self.start).as_secs_f32();
        let delta = self
            .last_update
            .map(|last| (now - last).as_secs_f32())
            .unwrap_or(0.0);
        self.last_update = Some(now);

        // Animate non-physics parameters with a per-parameter sine wave.
        for (i, param) in self.params.iter_mut().enumerate() {
            if !self.physics_driven[i] {
                let min = self.puppet.param_data().mins[i];
                let max = self.puppet.param_data().maxes[i];
                *param = (min + max) / 2.0 + (self.sts[i] + t * 0.5).sin() * (max - min) / 2.0;
            }
        }

        // Run the physics simulation and write outputs into `params`.
        self.physics.step(&mut self.params, delta);

        self.puppet
            .update(&self.params, &self.opacities, &mut self.frame_data);
    }
}

pub fn run(
    puppet: Puppet,
    frame_data: PuppetFrameData,
    textures: Vec<RgbaImage>,
    physics: PhysicsSystem,
    physics_driven: Vec<bool>,
    params: Vec<f32>,
) {
    let opacities = vec![1.0; puppet.part_count as usize];
    let mut sts = params.clone();
    let mut rng = rand::rng();

    for st in sts.iter_mut() {
        *st = rng.random();
    }

    let state = AppState {
        puppet,
        frame_data,
        params,
        opacities,
        sts,
        start: Instant::now(),
        last_update: None,
        textures,
        physics,
        physics_driven,
    };

    let mut app = App::new(state);

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut app).unwrap();
}
