mod error_message;
mod frame_counter;

use crate::*;
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
}

#[derive(Clone)]
struct ScrollBarProperties {
    width: f32,
    bg_color: mltg::Brush,
    thumb_color: mltg::Brush,
    thumb_hover_color: mltg::Brush,
    thumb_moving_color: mltg::Brush,
}

#[derive(Clone)]
struct UiProperties {
    factory: mltg::Factory,
    text_format: mltg::TextFormat,
    text_color: mltg::Brush,
    bg_color: mltg::Brush,
    scroll_bar: ScrollBarProperties,
    line_height: f32,
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
    settings: Settings,
    shader_model: hlsl::ShaderModel,
    compiler: hlsl::Compiler,
    window_receiver: WindowReceiver,
    renderer: Renderer,
    clear_color: [f32; 4],
    mouse: [f32; 2],
    start_time: std::time::Instant,
    dir_monitor: Option<DirMonitor>,
    state: State,
    ui_props: UiProperties,
    show_frame_counter: Rc<Cell<bool>>,
}

impl Application {
    pub fn new(settings: Settings, window_receiver: WindowReceiver) -> Result<Self, Error> {
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
        let scroll_bar = {
            let bg_color =
                factory.create_solid_color_brush(settings.appearance.scroll_bar.bg_color)?;
            let thumb_color =
                factory.create_solid_color_brush(settings.appearance.scroll_bar.thumb_color)?;
            let thumb_hover_color = factory
                .create_solid_color_brush(settings.appearance.scroll_bar.thumb_hover_color)?;
            let thumb_moving_color = factory
                .create_solid_color_brush(settings.appearance.scroll_bar.thumb_moving_color)?;
            ScrollBarProperties {
                width: settings.appearance.scroll_bar.width,
                bg_color,
                thumb_color,
                thumb_hover_color,
                thumb_moving_color,
            }
        };
        let line_height = {
            let layout = factory.create_text_layout(
                "A",
                &text_format,
                mltg::TextAlignment::Leading,
                None,
            )?;
            layout.size().height
        };
        let ui_props = UiProperties {
            factory,
            text_format,
            text_color,
            bg_color,
            scroll_bar,
            line_height,
        };
        let show_frame_counter = Rc::new(Cell::new(settings.frame_counter));
        let mut this = Self {
            settings,
            window_receiver,
            shader_model,
            compiler,
            renderer,
            clear_color,
            mouse: [0.0, 0.0],
            start_time: std::time::Instant::now(),
            dir_monitor: None,
            state: State::Init,
            ui_props,
            show_frame_counter,
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
            this.set_error(&Path::new("./this_is_test"), Error::TestErrorMessage(msg))?;
        }
        Ok(this)
    }

    fn load_file(&mut self, path: &Path) -> Result<(), Error> {
        assert!(path.is_file());
        let parent = path.parent().unwrap();
        let same_dir_monitor = self
            .dir_monitor
            .as_ref()
            .map_or(true, |d| d.path() != parent);
        if same_dir_monitor {
            debug!("load_file: DirMonitor::new: {}", parent.display());
            self.dir_monitor = Some(DirMonitor::new(parent)?);
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
        let resolution = self.settings.resolution.clone();
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
        self.start_time = std::time::Instant::now();
        self.window_receiver
            .main_window
            .set_title(format!("{} {}", TITLE, path.display()));
        info!("load file: {}", path.display());
        Ok(())
    }

    pub fn run(&mut self) -> Result<(), Error> {
        loop {
            let cursor_position = self.window_receiver.get_cursor_position();
            match self.window_receiver.try_recv() {
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
                    }
                }
                Some(WindowEvent::MouseInput(button, state)) => {
                    debug!("WindowEvent::MouseInput");
                    if let State::Error(em) = &mut self.state {
                        let main_window = &self.window_receiver.main_window;
                        let dpi = main_window.dpi();
                        let size = main_window.inner_size().to_logical(dpi).cast::<f32>();
                        let mouse_pos = cursor_position.to_logical(dpi as _).cast::<f32>();
                        em.mouse_event(size, mouse_pos, Some((button, state)))?;
                    }
                }
                Some(WindowEvent::Wheel(d)) => {
                    debug!("WindowEvent::Wheel");
                    if let State::Error(em) = &mut self.state {
                        let main_window = &self.window_receiver.main_window;
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
                        let main_window = &self.window_receiver.main_window;
                        let dpi = main_window.dpi();
                        let size = main_window.inner_size().to_logical(dpi).cast::<f32>();
                        e.recreate(size)?;
                    }
                }
                Some(WindowEvent::Restored(size)) => {
                    debug!("WindowEvent::Restored");
                    if let Err(e) = self.renderer.restore(size) {
                        error!("{}", e);
                    }
                    if let State::Error(e) = &mut self.state {
                        let main_window = &self.window_receiver.main_window;
                        let dpi = main_window.dpi();
                        let size = size.to_logical(dpi).cast::<f32>();
                        e.recreate(size)?;
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
                        let main_window = &self.window_receiver.main_window;
                        let dpi = main_window.dpi();
                        let size = size.to_logical(dpi).cast::<f32>();
                        e.recreate(size)?;
                    }
                }
                Some(WindowEvent::DpiChanged(dpi)) => {
                    debug!("WindowEvent::DpiChanged");
                    if let Err(e) = self.renderer.change_dpi(dpi) {
                        error!("{}", e);
                    }
                    let size = self.window_receiver.main_window.inner_size();
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
                        let main_window = &self.window_receiver.main_window;
                        let dpi = main_window.dpi();
                        let size = main_window.inner_size().to_logical(dpi).cast::<f32>();
                        let mouse_pos = cursor_position.to_logical(dpi as _).cast::<f32>();
                        em.mouse_event(size, mouse_pos, None)?;
                    }
                }
            }
            if let Some(path) = self.dir_monitor.as_ref().and_then(|dir| dir.try_recv()) {
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
                r.parameters.mouse = {
                    let size = self.window_receiver.main_window.inner_size().cast::<f32>();
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

    fn set_error(&mut self, path: &Path, e: Error) -> Result<(), Error> {
        let dpi = self.window_receiver.main_window.dpi();
        let size = self
            .window_receiver
            .main_window
            .inner_size()
            .to_logical(dpi)
            .cast::<f32>();
        self.set_state(State::Error(ErrorMessage::new(
            path.to_path_buf(),
            self.window_receiver.main_window.clone(),
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
}
