use crate::*;
use std::path::Path;

struct Rendering {
    parameters: Parameters,
    ps: PixelShaderPipeline,
}

enum State {
    Init,
    Rendering(Rendering),
}

pub struct Application {
    settings: Arc<Settings>,
    compiler: hlsl::Compiler,
    windows: WindowReceiver,
    window_thread: Option<std::thread::JoinHandle<()>>,
    renderer: Renderer,
    clear_color: [f32; 4],
    state: State,
    mouse: [f32; 2],
}

impl Application {
    pub fn new() -> anyhow::Result<Self> {
        let settings = Settings::load(SETTINGS_PATH)?;
        let compiler = hlsl::Compiler::new()?;
        let (windows, window_thread) = run_window_thread(settings.clone())?;
        let renderer = Renderer::new(&windows.main_window, &compiler)?;
        let clear_color = [
            settings.appearance.clear_color[0],
            settings.appearance.clear_color[1],
            settings.appearance.clear_color[2],
            0.0,
        ];
        Ok(Self {
            settings,
            windows,
            window_thread: Some(window_thread),
            compiler,
            renderer,
            clear_color,
            state: State::Init,
            mouse: [0.0, 0.0],
        })
    }

    fn load_file(&mut self, path: &Path) -> anyhow::Result<()> {
        let blob = self.compiler.compile_from_file(path, "main", &self.settings.shader.ps)?;
        let ps = self.renderer.create_pixel_shader_pipeline(&blob)?;
        let resolution = self.windows.main_window.inner_size();
        let parameters = Parameters {
            resolution: [resolution.width as _, resolution.height as _],
            mouse: self.mouse.clone(),
            time: 0.0,
        };
        self.state = State::Rendering(Rendering {
            parameters,
            ps,
        });
        Ok(())
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        loop {
            match self.windows.event.try_recv() {
                Ok(WindowEvent::LoadFile(path)) => {
                    match self.load_file(&path) {
                        Ok(_) => info!("load file: {}", path.display()),
                        Err(e) => error!("{}", e),
                    }
                }
                Ok(WindowEvent::Resized(size)) => {
                    if let Err(e) = self.renderer.resize(size) {
                        error!("{}", e);
                    }
                }
                Ok(WindowEvent::CursorMoved(pos)) => {
                    self.mouse = [pos.x as _, pos.y as _];
                    if let State::Rendering(r) = &mut self.state {
                        r.parameters.mouse = self.mouse.clone();
                    }
                }
                Ok(WindowEvent::Closed) => break,
                _ => {}
            }
            let ret = match &self.state {
                State::Init => {
                    self.renderer.render(&self.clear_color, None, None)
                }
                State::Rendering(r) => {
                    self.renderer.render(&self.clear_color, Some(&r.ps), Some(&r.parameters))
                }
            };
            if let Err(e) = ret {
                error!("{}", e);
            }
        }
        self.window_thread.take().unwrap().join().unwrap();
        Ok(())
    }
}
