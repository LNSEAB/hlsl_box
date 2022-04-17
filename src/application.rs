use crate::*;
use std::path::{Path, PathBuf};

struct Rendering {
    path: PathBuf,
    parameters: Parameters,
    ps: PixelShaderPipeline,
}

enum State {
    Init,
    Rendering(Rendering),
}

struct Empty {
    text_format: mltg::TextFormat,
    white: mltg::Brush,
}

impl UiRender for Empty {
    fn render(&self, cmd: &mltg::DrawCommand) {
        cmd.fill(
            &mltg::Rect::new([100.0, 100.0], [100.0, 100.0]),
            &self.white,
        );
        cmd.draw_text("test", &self.text_format, &self.white, [0.0, 0.0]);
    }
}

pub struct Application {
    settings: Arc<Settings>,
    compiler: hlsl::Compiler,
    window_receiver: WindowReceiver,
    renderer: Renderer,
    clear_color: [f32; 4],
    state: State,
    mouse: [f32; 2],
    start_time: std::time::Instant,
    dir: Option<DirMonitor>,
    empty: Empty,
}

impl Application {
    pub fn new(settings: Arc<Settings>, window_recevier: WindowReceiver) -> anyhow::Result<Self> {
        let args = std::env::args().collect::<Vec<_>>();
        let compiler = hlsl::Compiler::new()?;
        let debug_layer = args.iter().any(|arg| arg == "--debuglayer");
        let renderer = Renderer::new(
            &window_recevier.main_window,
            &compiler,
            &settings.shader.version,
            debug_layer,
        )?;
        let clear_color = [
            settings.appearance.clear_color[0],
            settings.appearance.clear_color[1],
            settings.appearance.clear_color[2],
            0.0,
        ];
        let factory = renderer.mltg_factory();
        let empty = Empty {
            text_format: factory.create_text_format(
                mltg::Font::System("Yu Gothic"),
                mltg::FontPoint(14.0),
                None,
            )?,
            white: factory.create_solid_color_brush([1.0, 1.0, 1.0, 1.0])?,
        };
        Ok(Self {
            settings,
            window_receiver: window_recevier,
            compiler,
            renderer,
            clear_color,
            state: State::Init,
            mouse: [0.0, 0.0],
            start_time: std::time::Instant::now(),
            dir: None,
            empty,
        })
    }

    fn load_file(&mut self, path: &Path) -> anyhow::Result<()> {
        assert!(path.is_file());
        let blob = self.compiler.compile_from_file(
            path,
            "main",
            &format!("ps_{}", &self.settings.shader.version),
            &self.settings.shader.ps_args,
        )?;
        let ps = self.renderer.create_pixel_shader_pipeline(&blob)?;
        let resolution = self.window_receiver.main_window.inner_size();
        let parameters = Parameters {
            resolution: [resolution.width as _, resolution.height as _],
            mouse: self.mouse.clone(),
            time: 0.0,
        };
        self.renderer.wait_all_signals();
        self.state = State::Rendering(Rendering {
            path: path.to_path_buf(),
            parameters,
            ps,
        });
        self.start_time = std::time::Instant::now();
        self.dir = Some(DirMonitor::new(path.parent().unwrap())?);
        self.window_receiver
            .main_window
            .set_title(format!("HLSLBox {}", path.display()));
        info!("load file: {}", path.display());
        Ok(())
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        loop {
            match self.window_receiver.event.try_recv() {
                Ok(WindowEvent::LoadFile(path)) => {
                    if let Err(e) = self.load_file(&path) {
                        error!("{}", e);
                    }
                }
                Ok(WindowEvent::Resized(size)) => {
                    debug!("WindowEvent::Resized");
                    if let Err(e) = self.renderer.resize(size) {
                        error!("{}", e);
                    }
                    if let State::Rendering(r) = &mut self.state {
                        r.parameters.resolution = [size.width as _, size.height as _];
                    }
                }
                _ => {}
            }
            if let Some(path) = self.dir.as_ref().and_then(|dir| dir.try_recv()) {
                if let State::Rendering(r) = &self.state {
                    if r.path == path {
                        if let Err(e) = self.load_file(&path) {
                            error!("{}", e);
                        }
                    }
                }
            }
            if self.window_receiver.main_window.is_closed() {
                break;
            }
            let ret = match &mut self.state {
                State::Init => self
                    .renderer
                    .render(1, &self.clear_color, None, None, &self.empty),
                State::Rendering(r) => {
                    r.parameters.mouse = {
                        let cursor_position = self.window_receiver.cursor_position.lock().unwrap();
                        [cursor_position.x as _, cursor_position.y as _]
                    };
                    r.parameters.time = (std::time::Instant::now() - self.start_time).as_secs_f32();
                    self.renderer.render(
                        1,
                        &self.clear_color,
                        Some(&r.ps),
                        Some(&r.parameters),
                        &self.empty,
                    )
                }
            };
            if let Err(e) = ret {
                error!("render: {}", e);
            }
        }
        Ok(())
    }
}
