#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod application;
mod error;
mod hlsl;
mod monitor;
mod renderer;
mod settings;
mod utility;
mod window;

use once_cell::sync::Lazy;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use tracing::{debug, error, info};
use windows::Win32::Globalization::*;

use application::{Application, Method};
use error::Error;
use monitor::*;
use renderer::*;
use settings::Settings;
use utility::*;
use window::*;

const SETTINGS_PATH: &str = "./settings.toml";
const TITLE: &str = "HLSL Box";

static LOCALE: Lazy<Option<String>> = Lazy::new(|| unsafe {
    let mut buffer = vec![0u16; 85];
    let size = GetUserDefaultLocaleName(&mut buffer) as usize;
    (size != 0).then(|| String::from_utf16_lossy(&buffer[0..size - 1]))
});

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
        let locale = std::ffi::CString::new(LOCALE.as_ref().map_or("", |l| l.as_str())).unwrap();
        libc::setlocale(libc::LC_ALL, locale.as_ptr());
    }
    logger();
    std::panic::set_hook(Box::new(|info| unsafe {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::*;

        let args = std::env::args().collect::<Vec<_>>();
        match info.payload().downcast_ref::<String>() {
            Some(msg) => {
                let s = match info.location() {
                    Some(loc) => format!("{} ({}:{})", msg, loc.file(), loc.line()),
                    None => msg.to_string(),
                };
                if !args.iter().any(|arg| arg == "--nomodal") {
                    MessageBoxW(
                        HWND(0),
                        s.as_str(),
                        TITLE,
                        MB_OK | MB_ICONERROR | MB_SYSTEMMODAL,
                    );
                }
                error!("panic: {}", s);
            }
            None => {
                let e = Error::UnknownError;
                match info.location() {
                    Some(loc) => error!("panic: {} ({}:{})", e, loc.file(), loc.line()),
                    None => error!("panic: {}", e),
                }
            }
        };
    }));
    info!("start");
    let _coinit = coinit::init(coinit::APARTMENTTHREADED | coinit::DISABLE_OLE1DDE).unwrap();
    let th_handle = Rc::new(RefCell::new(None));
    let th_handle_f = th_handle.clone();
    let f = move || -> Result<WindowManager, Error> {
        let settings = Settings::load(SETTINGS_PATH)?;
        let mut key_map = KeyboardMap::new();
        key_map.insert(
            vec![wita::VirtualKey::Ctrl, wita::VirtualKey::Char('O')],
            Method::OpenDialog,
        );
        key_map.insert(
            vec![wita::VirtualKey::Ctrl, wita::VirtualKey::Char('F')],
            Method::FrameCounter,
        );
        let (window, window_receiver) = WindowManager::new(&settings, key_map);
        let th_settings = settings;
        let th = std::thread::spawn(move || {
            info!("start rendering thread");
            let _coinit = coinit::init(coinit::MULTITHREADED | coinit::DISABLE_OLE1DDE).unwrap();
            let main_window = window_receiver.main_window.clone();
            let app = Application::new(th_settings, window_receiver).and_then(|mut app| app.run());
            if let Err(e) = app {
                main_window.close();
                panic!("panic rendering thread: {}", e);
            }
            info!("end rendering thread");
        });
        *th_handle_f.borrow_mut() = Some(th);
        Ok(window)
    };
    if let Err(e) = wita::run(wita::RunType::Wait, f) {
        error!("{}", e);
    }
    th_handle.borrow_mut().take().unwrap().join().unwrap();
    info!("end");
}
