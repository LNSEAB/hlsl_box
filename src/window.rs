use crate::application::Method;
use crate::*;
use std::{collections::HashMap, path::PathBuf, sync::*};

pub enum WindowEvent {
    LoadFile(PathBuf),
    Resized(wita::PhysicalSize<u32>),
    KeyInput(Method),
    DpiChanged(u32),
    Wheel(i32),
    MouseInput(wita::MouseButton, wita::KeyState),
    Restored(wita::PhysicalSize<u32>),
    Minimized,
    Maximized(wita::PhysicalSize<u32>),
    Closed(settings::Window),
}

pub struct WindowManager {
    pub main_window: wita::Window,
    event: mpsc::Receiver<WindowEvent>,
    sync_event: mpsc::Receiver<WindowEvent>,
    cursor_position: Arc<Mutex<wita::PhysicalPosition<i32>>>,
    resolution: Arc<Mutex<settings::Resolution>>,
}

impl WindowManager {
    pub fn try_recv(&self) -> Option<WindowEvent> {
        self.sync_event
            .try_recv()
            .ok()
            .or_else(|| self.event.try_recv().ok())
    }

    pub fn get_cursor_position(&self) -> wita::PhysicalPosition<i32> {
        *self.cursor_position.lock().unwrap()
    }

    pub fn update_resolution(&self, resolution: settings::Resolution) {
        let mut r = self.resolution.lock().unwrap();
        *r = resolution;
    }
}

pub struct KeyboardMap(HashMap<Vec<wita::VirtualKey>, Method>);

impl KeyboardMap {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn insert(&mut self, keys: Vec<wita::VirtualKey>, v: Method) {
        let mut special_key = |sk, l, r| {
            if let Some(p) = keys.iter().position(|k| k == &sk) {
                let mut tmp = keys.clone();
                tmp[p] = l;
                self.0.insert(tmp.clone(), v);
                tmp[p] = r;
                self.0.insert(tmp, v);
            }
        };
        special_key(
            wita::VirtualKey::Ctrl,
            wita::VirtualKey::LCtrl,
            wita::VirtualKey::RCtrl,
        );
        special_key(
            wita::VirtualKey::Alt,
            wita::VirtualKey::LAlt,
            wita::VirtualKey::RAlt,
        );
        special_key(
            wita::VirtualKey::Shift,
            wita::VirtualKey::LShift,
            wita::VirtualKey::RShift,
        );
        self.0.insert(keys, v);
    }
}

struct Window {
    window: wita::Window,
    position: wita::ScreenPosition,
    prev_position: wita::ScreenPosition,
    size: wita::PhysicalSize<u32>,
    maximized: bool,
}

impl Window {
    fn new(window: wita::Window) -> Self {
        let position = window.position();
        let size = window.inner_size();
        let maximized = window.is_maximized();
        Self {
            window,
            position,
            prev_position: position,
            size,
            maximized,
        }
    }
}

impl PartialEq<Window> for wita::Window {
    fn eq(&self, rhs: &Window) -> bool {
        self == &rhs.window
    }
}

pub struct WindowHandler {
    resolution: Arc<Mutex<settings::Resolution>>,
    main_window: Window,
    event: mpsc::Sender<WindowEvent>,
    sync_event: mpsc::SyncSender<WindowEvent>,
    cursor_position: Arc<Mutex<wita::PhysicalPosition<i32>>>,
    key_map: KeyboardMap,
    keys: Vec<wita::VirtualKey>,
}

impl WindowHandler {
    pub fn new(
        settings: &Result<Settings, Error>,
        window_setting: &settings::Window,
        key_map: KeyboardMap,
    ) -> (Self, WindowManager) {
        let main_window = wita::Window::builder()
            .title(TITLE)
            .position(wita::ScreenPosition::new(
                window_setting.x,
                window_setting.y,
            ))
            .inner_size(wita::PhysicalSize::new(
                window_setting.width,
                settings.as_ref().map_or(
                    window_setting.height,
                    |settings| {
                        window_setting.width * settings.resolution.height
                            / settings.resolution.width
                    },
                ),
            ))
            .accept_drag_files(true)
            .build()
            .unwrap();
        if window_setting.maximized {
            main_window.maximize();
        }
        let (tx, rx) = mpsc::channel();
        let (sync_tx, sync_rx) = mpsc::sync_channel(0);
        let cursor_position = Arc::new(Mutex::new(wita::PhysicalPosition::new(0, 0)));
        let resolution = Arc::new(Mutex::new(settings.as_ref().map_or_else(
            |_| settings::Resolution {
                width: 640,
                height: 480,
            },
            |settings| settings.resolution,
        )));
        (
            Self {
                resolution: resolution.clone(),
                main_window: Window::new(main_window.clone()),
                event: tx,
                sync_event: sync_tx,
                cursor_position: cursor_position.clone(),
                key_map,
                keys: Vec::with_capacity(5),
            },
            WindowManager {
                main_window,
                event: rx,
                sync_event: sync_rx,
                cursor_position,
                resolution,
            },
        )
    }
}

impl wita::EventHandler for WindowHandler {
    fn key_input(&mut self, ev: wita::event::KeyInput) {
        if ev.window == &self.main_window {
            if ev.state == wita::KeyState::Released {
                wita::keyboard_state(&mut self.keys);
                self.keys.retain(|k| {
                    if let wita::VirtualKey::Other(a) = k {
                        *a < 240
                    } else {
                        let ctrl = k == &wita::VirtualKey::Ctrl;
                        let alt = k == &wita::VirtualKey::Alt;
                        let shift = k == &wita::VirtualKey::Shift;
                        !(ctrl || alt || shift)
                    }
                });
                self.keys.push(ev.key_code.vkey);
                debug!("keys: {:?}", &self.keys);
                if let Some(m) = self.key_map.0.get(&self.keys) {
                    self.event.send(WindowEvent::KeyInput(*m)).ok();
                }
            }
            debug!("main_window key_input");
        }
    }

    fn mouse_input(&mut self, ev: wita::event::MouseInput) {
        if ev.window == &self.main_window {
            self.event
                .send(WindowEvent::MouseInput(ev.button, ev.button_state))
                .ok();
        }
    }

    fn cursor_moved(&mut self, ev: wita::event::CursorMoved) {
        if ev.window == &self.main_window {
            let mut cursor_position = self.cursor_position.lock().unwrap();
            *cursor_position = ev.mouse_state.position;
        }
    }

    fn mouse_wheel(&mut self, ev: wita::event::MouseWheel) {
        if ev.window == &self.main_window && ev.axis == wita::MouseWheelAxis::Vertical {
            self.event
                .send(WindowEvent::Wheel(-ev.distance / wita::WHEEL_DELTA))
                .ok();
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

    fn moved(&mut self, ev: wita::event::Moved) {
        if ev.window == &self.main_window {
            self.main_window.prev_position = self.main_window.position;
            self.main_window.position = ev.position;
        }
    }

    fn resizing(&mut self, ev: wita::event::Resizing) {
        if ev.window == &self.main_window {
            let resolution = self.resolution.lock().unwrap();
            match ev.edge {
                wita::ResizingEdge::Top | wita::ResizingEdge::Bottom => {
                    ev.size.width = ev.size.height * resolution.width / resolution.height;
                }
                _ => {
                    ev.size.height = ev.size.width * resolution.height / resolution.width;
                }
            }
        }
    }

    fn resized(&mut self, ev: wita::event::Resized) {
        if ev.window == &self.main_window {
            self.event.send(WindowEvent::Resized(ev.size)).ok();
            self.main_window.size = ev.size;
            debug!("main_window resized");
        }
    }

    fn restored(&mut self, ev: wita::event::Restored) {
        if ev.window == &self.main_window {
            let resolution = self.resolution.lock().unwrap();
            let mut size = ev.size;
            size.height = size.width * resolution.height / resolution.width;
            self.event.send(WindowEvent::Restored(size)).ok();
            self.main_window.size = size;
            self.main_window.window.set_inner_size(size);
            self.main_window.maximized = self.main_window.window.is_maximized();
            debug!("main_window restored");
        }
    }

    fn minimized(&mut self, ev: wita::event::Minimized) {
        if ev.window == &self.main_window {
            self.event.send(WindowEvent::Minimized).ok();
            self.main_window.position = self.main_window.prev_position;
            debug!("main_window minimized");
        }
    }

    fn maximized(&mut self, ev: wita::event::Maximized) {
        if ev.window == &self.main_window {
            self.event.send(WindowEvent::Maximized(ev.size)).ok();
            self.main_window.position = self.main_window.prev_position;
            self.main_window.maximized = true;
            debug!("main_window maximized");
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
            let params = settings::Window {
                x: self.main_window.position.x,
                y: self.main_window.position.y,
                width: self.main_window.size.width,
                height: self.main_window.size.height,
                maximized: self.main_window.maximized,
            };
            self.sync_event
                .send(WindowEvent::Closed(params))
                .unwrap_or(());
            debug!("main_window closed");
        }
    }
}
