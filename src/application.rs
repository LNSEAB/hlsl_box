use crate::*;
use std::{
    cell::Cell,
    collections::VecDeque,
    path::{Path, PathBuf},
    rc::Rc,
};
use windows::Win32::Graphics::{Direct3D::*, Direct3D12::*};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Method {
    OpenDialog,
    FrameCounter,
}

#[derive(Clone)]
struct UiProperties {
    factory: mltg::Factory,
    text_format: mltg::TextFormat,
    text_color: mltg::Brush,
    bg_color: mltg::Brush,
}

struct FrameCounter {
    count: Cell<u64>,
    text_layout: RefCell<mltg::TextLayout>,
    t: Cell<std::time::Instant>,
    ui_props: UiProperties,
}

impl FrameCounter {
    fn new(ui_props: &UiProperties) -> anyhow::Result<Self> {
        let text_layout = ui_props.factory.create_text_layout(
            "0",
            &ui_props.text_format,
            mltg::TextAlignment::Center,
            None,
        )?;
        Ok(Self {
            count: Cell::new(0),
            text_layout: RefCell::new(text_layout),
            t: Cell::new(std::time::Instant::now()),
            ui_props: ui_props.clone(),
        })
    }

    fn reset(&self) {
        self.count.set(0);
        self.t.set(std::time::Instant::now());
    }

    fn update(&self) -> anyhow::Result<()> {
        if (std::time::Instant::now() - self.t.get()).as_millis() >= 1000 {
            let text_layout = self.ui_props.factory.create_text_layout(
                &self.count.get().to_string(),
                &self.ui_props.text_format,
                mltg::TextAlignment::Center,
                None,
            )?;
            *self.text_layout.borrow_mut() = text_layout;
            self.reset();
        } else {
            self.count.set(self.count.get() + 1);
        }
        Ok(())
    }

    fn draw(&self, cmd: &mltg::DrawCommand, pos: impl Into<mltg::Point>) {
        let margin = mltg::Size::new(5.0, 3.0);
        let text_layout = self.text_layout.borrow();
        let pos = pos.into();
        let size = text_layout.size();
        cmd.fill(
            &mltg::Rect::new(
                pos,
                [
                    size.width + margin.width * 2.0,
                    size.height + margin.height * 2.0,
                ],
            ),
            &self.ui_props.bg_color,
        );
        cmd.draw_text_layout(
            &text_layout,
            &self.ui_props.text_color,
            [pos.x + margin.width, pos.y + margin.height],
        );
    }
}

struct Rendering {
    path: PathBuf,
    parameters: Parameters,
    ps: PixelShaderPipeline,
    frame_counter: FrameCounter,
    show_frame_counter: Rc<Cell<bool>>,
}

struct ErrorMessage {
    ui_props: UiProperties,
    text: Vec<String>,
    layouts: VecDeque<mltg::TextLayout>,
    current_line: u32,
}

impl ErrorMessage {
    fn new(text: &str, ui_props: &UiProperties, size: mltg::Size) -> anyhow::Result<Self> {
        let text = text.split('\n').map(|t| t.to_string()).collect::<Vec<_>>();
        let mut layouts = VecDeque::new();
        let mut index = 0;
        let mut height = 0.0;
        while index < text.len() && height < size.height {
            let layout = ui_props.factory.create_text_layout(
                &text[index],
                &ui_props.text_format,
                mltg::TextAlignment::Leading,
                None,
            )?;
            height += layout.size().height;
            index += 1;
            layouts.push_back(layout);
        }
        Ok(Self {
            ui_props: ui_props.clone(),
            text,
            layouts,
            current_line: 0,
        })
    }

    fn offset(&mut self, size: mltg::Size, d: i32) -> anyhow::Result<()> {
        let mut line = self.current_line;
        if d < 0 {
            let d = d.abs() as u32;
            if line <= d {
                line = 0;
            } else {
                line -= d;
            }
        } else {
            line = (line + d as u32).min(self.text.len() as u32 - 1);
        }
        if self.current_line == line {
            return Ok(());
        }
        if self.current_line > line {
            let mut index = self.current_line as isize - 1;
            while index >= line as _ {
                let layout = self.ui_props.factory.create_text_layout(
                    &self.text[index as usize],
                    &self.ui_props.text_format,
                    mltg::TextAlignment::Leading,
                    None,
                )?;
                self.layouts.push_front(layout);
                index -= 1;
            }
            let mut height = self.layouts.iter().fold(0.0, |h, l| h + l.size().height);
            while height - self.layouts.back().unwrap().size().height > size.height {
                let layout = self.layouts.pop_back().unwrap();
                height -= layout.size().height;
            }
        } else {
            let mut height = self.layouts.iter().fold(0.0, |h, l| h + l.size().height);
            let d = line - self.current_line;
            self.layouts.drain(..d as usize);
            let mut index = line as usize + self.layouts.len();
            while index < self.text.len() && height < size.height {
                let layout = self.ui_props.factory.create_text_layout(
                    &self.text[index],
                    &self.ui_props.text_format,
                    mltg::TextAlignment::Leading,
                    None,
                )?;
                height += layout.size().height;
                index += 1;
                self.layouts.push_back(layout);
            }
        }
        self.current_line = line;
        Ok(())
    }

    fn draw(&self, cmd: &mltg::DrawCommand) {
        let mut y = 0.0;
        for layout in &self.layouts {
            cmd.draw_text_layout(&layout, &self.ui_props.text_color, [0.0, y]);
            y += layout.size().height;
        }
    }
}

enum State {
    Init,
    Rendering(Rendering),
    Error(ErrorMessage),
}

impl RenderUi for State {
    fn render(&self, cmd: &mltg::DrawCommand) {
        match &self {
            State::Init => {}
            State::Rendering(r) => {
                r.frame_counter.update().unwrap();
                if r.show_frame_counter.get() {
                    r.frame_counter.draw(cmd, [10.0, 10.0]);
                }
            }
            State::Error(e) => {
                e.draw(cmd);
            }
        }
    }
}

pub struct Application {
    settings: Arc<Settings>,
    _d3d12_device: ID3D12Device,
    shader_model: hlsl::ShaderModel,
    compiler: hlsl::Compiler,
    window_receiver: WindowReceiver,
    renderer: Renderer,
    clear_color: [f32; 4],
    mouse: [f32; 2],
    start_time: std::time::Instant,
    dir: Option<DirMonitor>,
    state: State,
    ui_props: UiProperties,
    show_frame_counter: Rc<Cell<bool>>,
}

impl Application {
    pub fn new(settings: Arc<Settings>, window_receiver: WindowReceiver) -> anyhow::Result<Self> {
        let args = std::env::args().collect::<Vec<_>>();
        let compiler = hlsl::Compiler::new()?;
        let debug_layer = args.iter().any(|arg| arg == "--debuglayer");
        if debug_layer {
            unsafe {
                let mut debug: Option<ID3D12Debug> = None;
                let debug = D3D12GetDebugInterface(&mut debug).map(|_| debug.unwrap())?;
                debug.EnableDebugLayer();
            }
            info!("enable debug layer");
        }
        info!("settings version: {}", settings.version);
        let d3d12_device: ID3D12Device = unsafe {
            let mut device = None;
            D3D12CreateDevice(None, D3D_FEATURE_LEVEL_12_1, &mut device).map(|_| device.unwrap())?
        };
        let shader_model = hlsl::ShaderModel::new(&d3d12_device, settings.shader.version.as_ref())?;
        info!("shader model: {}", shader_model);
        let clear_color = [
            settings.appearance.clear_color[0],
            settings.appearance.clear_color[1],
            settings.appearance.clear_color[2],
            0.0,
        ];
        let renderer = Renderer::new(
            &d3d12_device,
            &window_receiver.main_window,
            settings.resolution.clone().into(),
            &compiler,
            shader_model,
            &clear_color,
        )?;
        let factory = renderer.mltg_factory();
        let text_format = factory.create_text_format(
            mltg::Font::System(&settings.appearance.font),
            mltg::FontPoint(settings.appearance.font_size),
            None,
        )?;
        let text_color = factory.create_solid_color_brush(settings.appearance.text_color)?;
        let bg_color = factory.create_solid_color_brush(settings.appearance.background_color)?;
        let ui_props = UiProperties {
            factory,
            text_format,
            text_color,
            bg_color,
        };
        let show_frame_counter = Rc::new(Cell::new(settings.frame_counter));
        Ok(Self {
            _d3d12_device: d3d12_device,
            settings,
            window_receiver,
            shader_model,
            compiler,
            renderer,
            clear_color,
            mouse: [0.0, 0.0],
            start_time: std::time::Instant::now(),
            dir: None,
            state: State::Init,
            ui_props,
            show_frame_counter,
        })
    }

    fn load_file(&mut self, path: &Path) -> anyhow::Result<()> {
        assert!(path.is_file());
        let blob = self.compiler.compile_from_file(
            path,
            "main",
            hlsl::Target::PS(self.shader_model),
            &self.settings.shader.ps_args,
        )?;
        let ps = self.renderer.create_pixel_shader_pipeline(&blob)?;
        let resolution = self.settings.resolution.clone();
        let parameters = Parameters {
            resolution: [resolution.width as _, resolution.height as _],
            mouse: self.mouse.clone(),
            time: 0.0,
        };
        let frame_counter = FrameCounter::new(&self.ui_props)?;
        self.renderer.wait_all_signals();
        self.state = State::Rendering(Rendering {
            path: path.to_path_buf(),
            parameters,
            ps,
            frame_counter,
            show_frame_counter: self.show_frame_counter.clone(),
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
                    debug!("WindowEvent::LoadFile");
                    if let Err(e) = self.load_file(&path) {
                        self.set_error(e)?;
                    }
                }
                Ok(WindowEvent::KeyInput(m)) => {
                    debug!("WindowEvent::KeyInput");
                    match m {
                        Method::OpenDialog => {
                            let dlg = ifdlg::FileOpenDialog::new();
                            match dlg.show::<PathBuf>() {
                                Ok(Some(path)) => {
                                    if let Err(e) = self.load_file(&path) {
                                        self.set_error(e)?;
                                    }
                                }
                                Err(e) => {
                                    error!("open dialog: {}", e);
                                }
                                _ => {}
                            }
                        }
                        Method::FrameCounter => {
                            self.show_frame_counter.set(!self.show_frame_counter.get());
                        }
                    }
                }
                Ok(WindowEvent::Wheel(d)) => {
                    debug!("WindowEvent::Wheel");
                    if let State::Error(em) = &mut self.state {
                        let main_window = &self.window_receiver.main_window;
                        let dpi = main_window.dpi();
                        let size = main_window.inner_size().to_logical(dpi).cast::<f32>();
                        em.offset([size.width, size.height].into(), d)?;
                    }
                }
                Ok(WindowEvent::Resized(size)) => {
                    debug!("WindowEvent::Resized");
                    if let Err(e) = self.renderer.resize(size) {
                        error!("{}", e);
                    }
                }
                Ok(WindowEvent::DpiChanged(dpi)) => {
                    debug!("WindowEvent::DpiChanged");
                    if let Err(e) = self.renderer.change_dpi(dpi) {
                        error!("{}", e);
                    }
                }
                Ok(WindowEvent::Closed { position, size }) => {
                    debug!("WindowEvent::Closed");
                    let settings = Settings {
                        version: self.settings.version.clone(),
                        frame_counter: self.show_frame_counter.get(),
                        window: settings::Window {
                            x: position.x,
                            y: position.y,
                            width: size.width,
                            height: size.height,
                        },
                        resolution: self.settings.resolution.clone(),
                        shader: self.settings.shader.clone(),
                        appearance: self.settings.appearance.clone(),
                    };
                    match settings.save(SETTINGS_PATH) {
                        Ok(_) => info!("saved settings"),
                        Err(e) => error!("save settings: {}", e),
                    }
                    break;
                }
                _ => {}
            }
            if let Some(path) = self.dir.as_ref().and_then(|dir| dir.try_recv()) {
                if let State::Rendering(r) = &self.state {
                    if r.path == path {
                        if let Err(e) = self.load_file(&path) {
                            self.set_error(e)?;
                        }
                    }
                }
            }
            if let State::Rendering(r) = &mut self.state {
                r.parameters.mouse = {
                    let size = self.window_receiver.main_window.inner_size().cast::<f32>();
                    let cursor_position = self.window_receiver.cursor_position.lock().unwrap();
                    [
                        cursor_position.x as f32 / size.width,
                        cursor_position.y as f32 / size.height,
                    ]
                };
                r.parameters.time = (std::time::Instant::now() - self.start_time).as_secs_f32();
            }
            let ret = match &self.state {
                State::Rendering(r) => self.renderer.render(
                    1,
                    &self.clear_color,
                    Some(&r.ps),
                    Some(&r.parameters),
                    &self.state,
                ),
                _ => self
                    .renderer
                    .render(1, &self.clear_color, None, None, &self.state),
            };
            if let Err(e) = ret {
                error!("render: {}", e);
            }
        }
        Ok(())
    }

    fn set_error(&mut self, e: anyhow::Error) -> anyhow::Result<()> {
        let dpi = self.window_receiver.main_window.dpi();
        let size = self
            .window_receiver
            .main_window
            .inner_size()
            .to_logical(dpi)
            .cast::<f32>();
        self.state = State::Error(ErrorMessage::new(
            &format!("{}", e),
            &self.ui_props,
            [size.width, size.height].into(),
        )?);
        error!("{}", e);
        Ok(())
    }
}
