mod error_message;
mod frame_counter;

use crate::*;
use std::sync::mpsc;
use std::{
    cell::Cell,
    collections::VecDeque,
    path::{Path, PathBuf},
    rc::Rc,
};
use windows::Win32::Graphics::{Direct3D::*, Direct3D12::*};

use error_message::*;
use frame_counter::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Method {
    OpenDialog,
    FrameCounter,
    ScreenShot,
    Play,
    Head,
}

#[derive(Clone)]
struct ScrollBarProperties {
    width: f32,
    bg_color: mltg::Brush,
    thumb_color: mltg::Brush,
    thumb_hover_color: mltg::Brush,
    thumb_moving_color: mltg::Brush,
}

impl ScrollBarProperties {
    fn new(settings: &Settings, factory: &mltg::Factory) -> Result<Self, Error> {
        let bg_color = factory.create_solid_color_brush(settings.appearance.scroll_bar.bg_color)?;
        let thumb_color =
            factory.create_solid_color_brush(settings.appearance.scroll_bar.thumb_color)?;
        let thumb_hover_color =
            factory.create_solid_color_brush(settings.appearance.scroll_bar.thumb_hover_color)?;
        let thumb_moving_color =
            factory.create_solid_color_brush(settings.appearance.scroll_bar.thumb_moving_color)?;
        Ok(Self {
            width: settings.appearance.scroll_bar.width,
            bg_color,
            thumb_color,
            thumb_hover_color,
            thumb_moving_color,
        })
    }
}

#[derive(Clone)]
struct UiProperties {
    factory: mltg::Factory,
    text_format: mltg::TextFormat,
    text_color: mltg::Brush,
    error_label_color: mltg::Brush,
    warn_label_color: mltg::Brush,
    info_label_color: mltg::Brush,
    under_line_color: mltg::Brush,
    bg_color: mltg::Brush,
    scroll_bar: ScrollBarProperties,
    line_height: f32,
}

impl UiProperties {
    fn new(settings: &Settings, factory: &mltg::Factory) -> Result<Self, Error> {
        let text_format = factory.create_text_format(
            mltg::Font::System(&settings.appearance.font),
            mltg::FontPoint(settings.appearance.font_size),
            None,
        )?;
        let text_color = factory.create_solid_color_brush(settings.appearance.text_color)?;
        let error_label_color =
            factory.create_solid_color_brush(settings.appearance.error_label_color)?;
        let warn_label_color =
            factory.create_solid_color_brush(settings.appearance.warn_label_color)?;
        let info_label_color =
            factory.create_solid_color_brush(settings.appearance.info_label_color)?;
        let under_line_color =
            factory.create_solid_color_brush(settings.appearance.under_line_color)?;
        let bg_color = factory.create_solid_color_brush(settings.appearance.background_color)?;
        let line_height = {
            let layout = factory.create_text_layout(
                "A",
                &text_format,
                mltg::TextAlignment::Leading,
                None,
            )?;
            layout.size().height
        };
        let scroll_bar = ScrollBarProperties::new(settings, factory)?;
        Ok(Self {
            factory: factory.clone(),
            text_format,
            text_color,
            error_label_color,
            warn_label_color,
            info_label_color,
            under_line_color,
            bg_color,
            scroll_bar,
            line_height,
        })
    }
}

struct Rendering {
    path: PathBuf,
    parameters: pixel_shader::Parameters,
    ps: pixel_shader::Pipeline,
    frame_counter: FrameCounter,
    show_frame_counter: Rc<Cell<bool>>,
}

enum State {
    Init,
    Rendering(Rendering),
    Error(ErrorMessage),
}

impl RenderUi for State {
    fn render(&self, cmd: &mltg::DrawCommand, size: wita::LogicalSize<f32>) {
        match &self {
            State::Init => {}
            State::Rendering(r) => {
                r.frame_counter.update().unwrap();
                if r.show_frame_counter.get() {
                    r.frame_counter.draw(cmd, [10.0, 10.0]);
                }
            }
            State::Error(e) => {
                e.draw(cmd, size);
            }
        }
    }
}

struct Timer {
    start_time: std::time::Instant,
    d: std::time::Duration,
}

impl Timer {
    fn new() -> Self {
        Self {
            start_time: std::time::Instant::now(),
            d: std::time::Duration::from_secs(0),
        }
    }

    fn get(&self) -> std::time::Duration {
        std::time::Instant::now() - self.start_time + self.d
    }

    fn start(&mut self) {
        self.start_time = std::time::Instant::now();
    }

    fn stop(&mut self) {
        self.d = self.get();
    }
}

struct ScreenShot {
    date: chrono::Date<chrono::Local>,
    count: u64,
    th: Option<std::thread::JoinHandle<()>>,
    tx: Option<mpsc::Sender<(image::RgbaImage, PathBuf)>>,
}

impl ScreenShot {
    fn new() -> Self {
        let date = chrono::Local::today();
        let date_str = format!("{}", date);
        let read_dir = |dir: std::fs::ReadDir| {
            dir.flatten()
                .filter_map(|entry| {
                    entry
                        .file_name()
                        .to_str()
                        .filter(|name| name.starts_with(&date_str))
                        .and_then(|name| name.split('-').last().and_then(|l| l.parse::<u64>().ok()))
                })
                .max()
        };
        let count = SCREEN_SHOT_PATH
            .read_dir()
            .ok()
            .and_then(read_dir)
            .unwrap_or(1);
        let (tx, rx) = mpsc::channel::<(image::RgbaImage, PathBuf)>();
        let th = std::thread::spawn(move || {
            while let Ok((img, path)) = rx.recv() {
                match img.save(&path) {
                    Ok(_) => info!("save screen shot: {}", path.display()),
                    Err(e) => error!("save screen shot: {}", e),
                }
            }
        });
        Self {
            date,
            count,
            th: Some(th),
            tx: Some(tx),
        }
    }

    fn save(&mut self, renderer: &Renderer) -> anyhow::Result<()> {
        if !SCREEN_SHOT_PATH.is_dir() {
            std::fs::create_dir(&*SCREEN_SHOT_PATH).unwrap();
        }
        let img = renderer.screen_shot()?;
        if img.is_none() {
            return Ok(());
        }
        let img = img.unwrap();
        let date = chrono::Local::today();
        if date != self.date {
            self.date = date;
            self.count = 1;
        }
        let path = loop {
            let file_name = format!("{}-{}.png", date.format("%Y-%m-%d"), self.count);
            let path = SCREEN_SHOT_PATH.join(file_name);
            if !path.is_file() {
                break path;
            }
            self.count += 1;
        };
        self.tx.as_ref().unwrap().send((img, path)).ok();
        self.count += 1;
        Ok(())
    }
}

impl Drop for ScreenShot {
    fn drop(&mut self) {
        std::mem::drop(self.tx.take().unwrap());
        self.th.take().unwrap().join().unwrap_or(());
    }
}

pub struct Application {
    d3d12_device: ID3D12Device,
    settings: Settings,
    shader_model: hlsl::ShaderModel,
    compiler: hlsl::Compiler,
    window_manager: WindowManager,
    renderer: Renderer,
    clear_color: [f32; 4],
    mouse: [f32; 2],
    play: bool,
    timer: Timer,
    exe_dir_monitor: DirMonitor,
    hlsl_dir_monitor: Option<DirMonitor>,
    state: State,
    ui_props: UiProperties,
    show_frame_counter: Rc<Cell<bool>>,
    screen_shot: ScreenShot,
}

impl Application {
    pub fn new(settings: Settings, window_manager: WindowManager) -> anyhow::Result<Self> {
        let compiler = hlsl::Compiler::new()?;
        let debug_layer = ENV_ARGS.debuglayer;
        if debug_layer {
            unsafe {
                let mut debug: Option<ID3D12Debug> = None;
                let debug = D3D12GetDebugInterface(&mut debug).map(|_| debug.unwrap())?;
                debug.EnableDebugLayer();
            }
            info!("enable debug layer");
        }
        info!("locale: {}", LOCALE.as_ref().map_or("", |s| s.as_str()));
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
            &window_manager.main_window,
            settings.resolution.into(),
            &compiler,
            shader_model,
        )?;
        let factory = renderer.mltg_factory();
        let ui_props = UiProperties::new(&settings, &factory)?;
        let show_frame_counter = Rc::new(Cell::new(settings.frame_counter));
        let exe_dir_monitor = DirMonitor::new(&*EXE_DIR_PATH)?;
        let screen_shot = ScreenShot::new();
        let mut this = Self {
            settings,
            d3d12_device,
            window_manager,
            shader_model,
            compiler,
            renderer,
            clear_color,
            mouse: [0.0, 0.0],
            play: false,
            timer: Timer::new(),
            exe_dir_monitor,
            hlsl_dir_monitor: None,
            state: State::Init,
            ui_props,
            show_frame_counter,
            screen_shot,
        };
        if let Some(path) = ENV_ARGS.input_file.as_ref().map(Path::new) {
            if let Err(e) = this.load_file(path) {
                this.set_error(path, e)?;
            }
        }
        if ENV_ARGS.debug_error_msg {
            let msg = (0..2000).fold(String::new(), |mut s, i| {
                s.push_str(&format!("{}\n", i));
                s
            });
            this.set_error(Path::new("./this_is_test"), Error::TestErrorMessage(msg))?;
        }
        Ok(this)
    }

    fn load_file(&mut self, path: &Path) -> Result<(), Error> {
        assert!(path.is_file());
        let parent = path.parent().unwrap();
        let same_dir_monitor = self
            .hlsl_dir_monitor
            .as_ref()
            .map_or(true, |d| d.path() != parent);
        if same_dir_monitor {
            debug!("load_file: DirMonitor::new: {}", parent.display());
            self.hlsl_dir_monitor = Some(DirMonitor::new(parent)?);
        }
        let blob = self.compiler.compile_from_file(
            path,
            "main",
            hlsl::Target::PS(self.shader_model),
            &self.settings.shader.ps_args,
        )?;
        let ps = self
            .renderer
            .create_pixel_shader_pipeline(&format!("{}", path.display()), &blob)?;
        let resolution = self.settings.resolution;
        let parameters = pixel_shader::Parameters {
            resolution: [resolution.width as _, resolution.height as _],
            mouse: self.mouse,
            time: 0.0,
        };
        let frame_counter = FrameCounter::new(&self.ui_props)?;
        self.set_state(State::Rendering(Rendering {
            path: path.to_path_buf(),
            parameters,
            ps,
            frame_counter,
            show_frame_counter: self.show_frame_counter.clone(),
        }));
        self.play = self.settings.auto_play;
        self.timer = Timer::new();
        self.window_manager
            .main_window
            .set_title(format!("{} {}", TITLE, path.display()));
        info!("load file: {}", path.display());
        Ok(())
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        loop {
            if let Some(path) = self.exe_dir_monitor.try_recv() {
                if path.as_path() == SETTINGS_PATH.as_path() {
                    self.reload_settings()?;
                    info!("reload settings.toml");
                }
            }
            let cursor_position = self.window_manager.get_cursor_position();
            match self.window_manager.try_recv() {
                Some(WindowEvent::LoadFile(path)) => {
                    debug!("WindowEvent::LoadFile");
                    if let Err(e) = self.load_file(&path) {
                        self.set_error(&path, e)?;
                    }
                }
                Some(WindowEvent::KeyInput(m)) => {
                    debug!("WindowEvent::KeyInput");
                    match m {
                        Method::OpenDialog => {
                            let dlg = ifdlg::FileOpenDialog::new();
                            match dlg.show::<PathBuf>() {
                                Ok(Some(path)) => {
                                    if let Err(e) = self.load_file(&path) {
                                        self.set_error(&path, e)?;
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
                        Method::ScreenShot => {
                            self.screen_shot.save(&self.renderer)?;
                        }
                        Method::Play => {
                            self.play = !self.play;
                            if self.play {
                                self.timer.start();
                            } else {
                                self.timer.stop();
                            }
                        }
                        Method::Head => {
                            self.timer = Timer::new();
                            if let State::Rendering(r) = &mut self.state {
                                r.parameters.time = 0.0;
                            }
                        }
                    }
                }
                Some(WindowEvent::MouseInput(button, state)) => {
                    debug!("WindowEvent::MouseInput");
                    if let State::Error(em) = &mut self.state {
                        let main_window = &self.window_manager.main_window;
                        let dpi = main_window.dpi();
                        let size = main_window.inner_size().to_logical(dpi).cast::<f32>();
                        let mouse_pos = cursor_position.to_logical(dpi as _).cast::<f32>();
                        em.mouse_event(mouse_pos, Some((button, state)), size)?;
                    }
                }
                Some(WindowEvent::Wheel(d)) => {
                    debug!("WindowEvent::Wheel");
                    if let State::Error(em) = &mut self.state {
                        let main_window = &self.window_manager.main_window;
                        let dpi = main_window.dpi();
                        let size = main_window.inner_size().to_logical(dpi).cast::<f32>();
                        em.offset(size, d)?;
                    }
                }
                Some(WindowEvent::Resized(size)) => {
                    debug!("WindowEvent::Resized");
                    if let Err(e) = self.renderer.resize(size) {
                        error!("{}", e);
                    }
                    if let State::Error(e) = &mut self.state {
                        let main_window = &self.window_manager.main_window;
                        let dpi = main_window.dpi();
                        let size = main_window.inner_size().to_logical(dpi).cast::<f32>();
                        e.recreate_text(size)?;
                    }
                }
                Some(WindowEvent::Restored(size)) => {
                    debug!("WindowEvent::Restored");
                    if let Err(e) = self.renderer.restore(size) {
                        error!("{}", e);
                    }
                    if let State::Error(e) = &mut self.state {
                        let main_window = &self.window_manager.main_window;
                        let dpi = main_window.dpi();
                        let size = size.to_logical(dpi).cast::<f32>();
                        e.recreate_text(size)?;
                    }
                }
                Some(WindowEvent::Minimized) => {
                    debug!("WindowEvent::Minimized");
                }
                Some(WindowEvent::Maximized(size)) => {
                    debug!("WindowEvent::Maximized");
                    if let Err(e) = self.renderer.maximize(size) {
                        error!("{}", e);
                    }
                    if let State::Error(e) = &mut self.state {
                        let main_window = &self.window_manager.main_window;
                        let dpi = main_window.dpi();
                        let size = size.to_logical(dpi).cast::<f32>();
                        e.recreate_text(size)?;
                    }
                }
                Some(WindowEvent::DpiChanged(dpi)) => {
                    debug!("WindowEvent::DpiChanged");
                    if let Err(e) = self.renderer.change_dpi(dpi) {
                        error!("{}", e);
                    }
                    let size = self.window_manager.main_window.inner_size();
                    if let Err(e) = self.renderer.resize(size) {
                        error!("{}", e);
                    }
                }
                Some(WindowEvent::Closed(window)) => {
                    debug!("WindowEvent::Closed");
                    self.settings.window = window;
                    match self.settings.save(&*SETTINGS_PATH) {
                        Ok(_) => info!("saved settings"),
                        Err(e) => error!("save settings: {}", e),
                    }
                    break;
                }
                _ => {
                    if let State::Error(em) = &mut self.state {
                        let main_window = &self.window_manager.main_window;
                        let dpi = main_window.dpi();
                        let size = main_window.inner_size().to_logical(dpi).cast::<f32>();
                        let mouse_pos = cursor_position.to_logical(dpi as _).cast::<f32>();
                        em.mouse_event(mouse_pos, None, size)?;
                    }
                }
            }
            if let Some(path) = self
                .hlsl_dir_monitor
                .as_ref()
                .and_then(|dir| dir.try_recv())
            {
                match &self.state {
                    State::Rendering(r) => {
                        if r.path == path {
                            if let Err(e) = self.load_file(&path) {
                                self.set_error(&path, e)?;
                            }
                        }
                    }
                    State::Error(e) => {
                        if e.path() == path {
                            if let Err(e) = self.load_file(&path) {
                                self.set_error(&path, e)?;
                            }
                        }
                    }
                    _ => {}
                }
            }
            if let State::Rendering(r) = &mut self.state {
                if self.play {
                    r.parameters.mouse = {
                        let size = self.window_manager.main_window.inner_size().cast::<f32>();
                        [
                            cursor_position.x as f32 / size.width,
                            cursor_position.y as f32 / size.height,
                        ]
                    };
                    r.parameters.time = self.timer.get().as_secs_f32();
                }
            }
            let ret = match &self.state {
                State::Rendering(r) => self.renderer.render(
                    1,
                    self.clear_color,
                    Some(&r.ps),
                    Some(&r.parameters),
                    &self.state,
                ),
                _ => self
                    .renderer
                    .render(1, self.clear_color, None, None, &self.state),
            };
            if let Err(e) = ret {
                error!("render: {}", e);
            }
        }
        Ok(())
    }

    fn set_error(&mut self, path: &Path, e: Error) -> anyhow::Result<()> {
        let dpi = self.window_manager.main_window.dpi();
        let size = self
            .window_manager
            .main_window
            .inner_size()
            .to_logical(dpi)
            .cast::<f32>();
        self.set_state(State::Error(ErrorMessage::new(
            path.to_path_buf(),
            &e,
            &self.ui_props,
            [size.width, size.height].into(),
        )?));
        error!("{}", e);
        Ok(())
    }

    fn set_state(&mut self, new_state: State) {
        self.renderer.wait_all_signals();
        self.state = new_state;
    }

    fn reload_settings(&mut self) -> anyhow::Result<()> {
        let settings = Settings::load(&*SETTINGS_PATH)?;
        let shader_model =
            hlsl::ShaderModel::new(&self.d3d12_device, settings.shader.version.as_ref())?;
        let clear_color = [
            settings.appearance.clear_color[0],
            settings.appearance.clear_color[1],
            settings.appearance.clear_color[2],
            0.0,
        ];
        let ui_props = UiProperties::new(&settings, &self.ui_props.factory)?;
        self.renderer
            .recreate(settings.resolution, &self.compiler, shader_model)?;
        self.window_manager.update_resolution(settings.resolution);
        let mut size = self.window_manager.main_window.inner_size();
        if self.window_manager.main_window.is_maximized() {
            self.renderer.maximize(size)?;
        } else {
            size.height = size.width * settings.resolution.height / settings.resolution.width;
            self.window_manager.main_window.set_inner_size(size);
            self.renderer.resize(size)?;
        }
        match &mut self.state {
            State::Rendering(r) => {
                r.parameters.resolution = [
                    settings.resolution.width as f32,
                    settings.resolution.height as f32,
                ];
            }
            State::Error(em) => {
                let dpi = self.window_manager.main_window.dpi();
                let size = size.to_logical(dpi as _).cast::<f32>();
                em.reset(&ui_props, size)?;
            }
            _ => {}
        }
        self.settings = settings;
        self.ui_props = ui_props;
        self.clear_color = clear_color;
        Ok(())
    }
}
