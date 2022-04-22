#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod application;
mod hlsl;
mod monitor;
mod renderer;
mod settings;
mod utility;
mod window;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use tracing::{debug, error, info};

use application::{Application, Method};
use monitor::*;
use renderer::*;
use settings::Settings;
use utility::*;
use window::*;

const SETTINGS_PATH: &str = "./settings.toml";

fn logger() {
    use std::fs::File;
    use tracing_subscriber::{filter::LevelFilter, prelude::*};

    let filter = if cfg!(debug_assertions) {
        LevelFilter::DEBUG
    } else {
        LevelFilter::INFO
    };
    let file = tracing_subscriber::fmt::layer()
        .compact()
        .with_writer(Arc::new(File::create("hlsl_box.log").unwrap()))
        .with_ansi(false)
        .with_line_number(true)
        .with_filter(filter);
    let console = tracing_subscriber::fmt::layer()
        .compact()
        .with_line_number(true)
        .with_filter(LevelFilter::TRACE);
    tracing_subscriber::registry()
        .with(file)
        .with(console)
        .init();
}

fn main() {
    unsafe {
        libc::setlocale(libc::LC_ALL, b"\0".as_ptr() as _);
    }
    logger();
    std::panic::set_hook(Box::new(|info| unsafe {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::*;
        match info.payload().downcast_ref::<&str>() {
            Some(&msg) => {
                let s = match info.location() {
                    Some(loc) => format!("{} ({}:{})", msg, loc.file(), loc.line()),
                    None => msg.to_string(),
                };
                MessageBoxW(HWND(0), s.as_str(), "HLSLBox", MB_OK | MB_ICONERROR);
                error!("panic: {}", s);
            },
            None => {
                error!("panic: unknown error");
            }
        };
    }));
    let _coinit = coinit::init(coinit::APARTMENTTHREADED | coinit::DISABLE_OLE1DDE).unwrap();
    let th_handle = Rc::new(RefCell::new(None));
    let th_handle_f = th_handle.clone();
    info!("start");
    let f = move || -> anyhow::Result<Window> {
        let settings = Arc::new(Settings::load(SETTINGS_PATH)?);
        let mut key_map = KeyboardMap::new();
        key_map.insert(
            vec![wita::VirtualKey::Ctrl, wita::VirtualKey::Char('O')],
            Method::OpenDialog,
        );
        key_map.insert(
            vec![wita::VirtualKey::Ctrl, wita::VirtualKey::Char('F')],
            Method::FrameCounter,
        );
        let (window, window_receiver) = Window::new(settings.clone(), key_map).unwrap();
        let th_settings = settings.clone();
        let th = std::thread::spawn(move || {
            info!("start rendering thread");
            let _coinit = coinit::init(coinit::MULTITHREADED | coinit::DISABLE_OLE1DDE).unwrap();
            let main_window = window_receiver.main_window.clone();
            let app = Application::new(th_settings, window_receiver).and_then(|mut app| app.run());
            if let Err(e) = app {
                main_window.close();
                panic!("{}", e);
            }
            info!("end rendering thread");
        });
        *th_handle_f.borrow_mut() = Some(th);
        Ok(window)
    };
    if let Err(e) = wita::run(wita::RunType::Wait, f) {
        error!("{}\n{}", e, e.backtrace());
    }
    th_handle.borrow_mut().take().unwrap().join().ok();
    info!("end");
}
