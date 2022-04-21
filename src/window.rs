use crate::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::*;

pub enum WindowEvent {
    LoadFile(PathBuf),
    Resized(wita::PhysicalSize<u32>),
    DpiChanged(u32),
    Closed {
        position: wita::ScreenPosition,
        size: wita::PhysicalSize<u32>,
    },
}

pub struct WindowReceiver {
    pub main_window: wita::Window,
    pub event: mpsc::Receiver<WindowEvent>,
    pub cursor_position: Arc<Mutex<wita::PhysicalPosition<i32>>>,
}

pub struct Window {
    settings: Arc<Settings>,
    main_window: wita::Window,
    event: mpsc::Sender<WindowEvent>,
    cursor_position: Arc<Mutex<wita::PhysicalPosition<i32>>>,
}

impl Window {
    pub fn new(settings: Arc<Settings>) -> anyhow::Result<(Self, WindowReceiver)> {
        let main_window = wita::Window::builder()
            .title("HLSLBox")
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
                settings,
                main_window: main_window.clone(),
                event: tx,
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
                    debug!("open dialog: {}", path.display());
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
            debug!("main_window drop_files");
        }
    }

    fn resizing(&mut self, ev: wita::event::Resizing) {
        if ev.window == &self.main_window {
            ev.size.height =
                ev.size.width * self.settings.resolution.height / self.settings.resolution.width;
        }
    }

    fn resized(&mut self, ev: wita::event::Resized) {
        if ev.window == &self.main_window {
            self.event.send(WindowEvent::Resized(ev.size)).ok();
            debug!("main_window resized");
        }
    }

    fn dpi_changed(&mut self, ev: wita::event::DpiChanged) {
        if ev.window == &self.main_window {
            self.event.send(WindowEvent::DpiChanged(ev.new_dpi)).ok();
            debug!("main_window dpi changed");
        }
    }

    fn closed(&mut self, ev: wita::event::Closed) {
        if ev.window == &self.main_window {
            let position = self.main_window.position();
            let size = self.main_window.inner_size();
            self.event.send(WindowEvent::Closed { position, size }).ok();
            debug!("main_window closed");
        }
    }
}
