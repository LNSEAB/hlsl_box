use crate::application::Method;
use crate::*;
use std::{collections::HashMap, path::PathBuf, sync::*};

pub enum WindowEvent {
    LoadFile(PathBuf),
    Resized(wita::PhysicalSize<u32>),
    KeyInput(Method),
    DpiChanged(u32),
    Wheel(i32),
    Closed {
        position: wita::ScreenPosition,
        size: wita::PhysicalSize<u32>,
    },
}

pub struct WindowReceiver {
    pub main_window: wita::Window,
    pub event: mpsc::Receiver<WindowEvent>,
    pub sync_event: mpsc::Receiver<WindowEvent>,
    pub cursor_position: Arc<Mutex<wita::PhysicalPosition<i32>>>,
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

pub struct Window {
    settings: Arc<Settings>,
    main_window: wita::Window,
    event: mpsc::Sender<WindowEvent>,
    sync_event: mpsc::SyncSender<WindowEvent>,
    cursor_position: Arc<Mutex<wita::PhysicalPosition<i32>>>,
    key_map: KeyboardMap,
    keys: Vec<wita::VirtualKey>,
}

impl Window {
    pub fn new(
        settings: Arc<Settings>,
        key_map: KeyboardMap,
    ) -> anyhow::Result<(Self, WindowReceiver)> {
        let main_window = wita::Window::builder()
            .title(TITLE)
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
        let (sync_tx, sync_rx) = mpsc::sync_channel(0);
        let cursor_position = Arc::new(Mutex::new(wita::PhysicalPosition::new(0, 0)));
        Ok((
            Self {
                settings,
                main_window: main_window.clone(),
                event: tx,
                sync_event: sync_tx,
                cursor_position: cursor_position.clone(),
                key_map,
                keys: Vec::with_capacity(5),
            },
            WindowReceiver {
                main_window,
                event: rx,
                sync_event: sync_rx,
                cursor_position,
            },
        ))
    }
}

impl wita::EventHandler for Window {
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
                    self.event.send(WindowEvent::KeyInput(m.clone())).ok();
                }
            }
            debug!("main_window key_input");
        }
    }

    fn cursor_moved(&mut self, ev: wita::event::CursorMoved) {
        if ev.window == &self.main_window {
            let mut cursor_position = self.cursor_position.lock().unwrap();
            *cursor_position = ev.mouse_state.position;
        }
    }

    fn mouse_wheel(&mut self, ev: wita::event::MouseWheel) {
        if ev.window == &self.main_window {
            if ev.axis == wita::MouseWheelAxis::Vertical {
                self.event
                    .send(WindowEvent::Wheel(-ev.distance / wita::WHEEL_DELTA))
                    .ok();
            }
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
            self.sync_event.send(WindowEvent::Closed { position, size }).unwrap_or(());
            debug!("main_window closed");
        }
    }
}
