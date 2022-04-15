use crate::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::*;

pub enum WindowEvent {
    LoadFile(PathBuf),
    Resized(wita::PhysicalSize<u32>),
    Closed,
}

pub struct WindowReceiver {
    pub main_window: wita::Window,
    pub event: mpsc::Receiver<WindowEvent>,
    pub cursor_position: Arc<Mutex<wita::PhysicalPosition<i32>>>,
}

struct Window {
    main_window: wita::Window,
    event: mpsc::Sender<WindowEvent>,
    settings: Arc<Settings>,
    cursor_position: Arc<Mutex<wita::PhysicalPosition<i32>>>,
}

impl Window {
    fn new(settings: Arc<Settings>) -> anyhow::Result<(Self, WindowReceiver)> {
        let main_window = wita::Window::builder()
            .title("HLSL Box")
            .position(wita::ScreenPosition::new(
                settings.window.x,
                settings.window.y,
            ))
            .inner_size(wita::PhysicalSize::new(
                settings.window.width,
                settings.window.height,
            ))
            .accept_drag_files(true)
            .build()?;
        let (tx, rx) = mpsc::channel();
        let cursor_position = Arc::new(Mutex::new(wita::PhysicalPosition::new(0, 0)));
        Ok((
            Self {
                main_window: main_window.clone(),
                event: tx,
                settings,
                cursor_position: cursor_position.clone(),
            },
            WindowReceiver {
                main_window,
                event: rx,
                cursor_position,
            },
        ))
    }
}

impl wita::EventHandler for Window {
    fn key_input(&mut self, ev: wita::event::KeyInput) {
        if ev.window == &self.main_window {
            let ctrl = wita::get_key_state(wita::VirtualKey::Ctrl);
            let released = ev.state == wita::KeyState::Released;
            let o = ev.key_code.vkey == wita::VirtualKey::Char('O');
            if ctrl && released && o {
                let dialog = ifdlg::FileOpenDialog::new();
                if let Ok(Some(path)) = dialog.show::<PathBuf>() {
                    self.event.send(WindowEvent::LoadFile(path)).ok();
                }
            }
        }
    }

    fn cursor_moved(&mut self, ev: wita::event::CursorMoved) {
        if ev.window == &self.main_window {
            let mut cursor_position = self.cursor_position.lock().unwrap();
            *cursor_position = ev.mouse_state.position;
        }
    }

    fn drop_files(&mut self, ev: wita::event::DropFiles) {
        if ev.window == &self.main_window {
            self.event
                .send(WindowEvent::LoadFile(ev.paths[0].to_path_buf()))
                .ok();
            trace!("main_window drop_files");
        }
    }

    fn resized(&mut self, ev: wita::event::Resized) {
        if ev.window == &self.main_window {
            self.event.send(WindowEvent::Resized(ev.size)).ok();
            trace!("main_window resized");
        }
    }

    fn closed(&mut self, ev: wita::event::Closed) {
        if ev.window == &self.main_window {
            self.event.send(WindowEvent::Closed).ok();
            let position = self.main_window.position();
            let size = self.main_window.inner_size();
            let settings = Settings {
                window: settings::Window {
                    x: position.x,
                    y: position.y,
                    width: size.width,
                    height: size.height,
                },
                shader: settings::Shader {
                    ps: self.settings.shader.ps.clone(),
                    ps_args: self.settings.shader.ps_args.clone(),
                },
                appearance: settings::Appearance {
                    clear_color: self.settings.appearance.clear_color,
                },
            };
            if let Err(e) = settings.save(SETTINGS_PATH) {
                error!("save settings: {}", e);
            }
        }
    }
}

pub fn run_window_thread(
    settings: Arc<Settings>,
) -> anyhow::Result<(WindowReceiver, std::thread::JoinHandle<()>)> {
    let _coinit = coinit::init(coinit::APARTMENTTHREADED | coinit::DISABLE_OLE1DDE)?;
    let (window_tx, window_rx) = std::sync::mpsc::channel::<WindowReceiver>();
    let th = std::thread::spawn(move || {
        info!("spawn window thread");
        wita::run(wita::RunType::Wait, move || -> anyhow::Result<Window> {
            let (main_window, rx) = Window::new(settings)?;
            window_tx.send(rx).ok();
            Ok(main_window)
        })
        .unwrap();
        info!("end window thread");
    });
    Ok((window_rx.recv().unwrap(), th))
}
