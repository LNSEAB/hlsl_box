#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod application;
mod error;
mod hlsl;
mod monitor;
mod renderer;
mod settings;
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
use window::*;

const TITLE: &str = "HLSL Box";

static LOCALE: Lazy<Option<String>> = Lazy::new(|| unsafe {
    let mut buffer = vec![0u16; 85];
    let size = GetUserDefaultLocaleName(&mut buffer) as usize;
    (size != 0).then(|| String::from_utf16_lossy(&buffer[0..size - 1]))
});

#[derive(Debug, clap::Parser)]
struct EnvArgs {
    #[clap(long)]
    debuglayer: bool,
    #[clap(long)]
    nomodal: bool,
    #[clap(long)]
    debug_error_msg: bool,
    input_file: Option<String>,
}

static ENV_ARGS: Lazy<EnvArgs> = Lazy::new(|| {
    use clap::Parser;
    EnvArgs::parse()
});

static EXE_DIR_PATH: Lazy<std::path::PathBuf> = Lazy::new(|| {
    std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
});

static SETTINGS_PATH: Lazy<std::path::PathBuf> = Lazy::new(|| EXE_DIR_PATH.join("settings.toml"));

fn set_logger() {
    use std::fs::File;
    use tracing_subscriber::{filter::LevelFilter, prelude::*};

    let filter = if cfg!(debug_assertions) {
        LevelFilter::DEBUG
    } else {
        LevelFilter::INFO
    };
    let file = tracing_subscriber::fmt::layer()
        .compact()
        .with_writer(Arc::new(
            File::create(EXE_DIR_PATH.join("hlsl_box.log")).unwrap(),
        ))
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

fn panic_handler(info: &std::panic::PanicInfo) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::*;
    let msg = info
        .payload()
        .downcast_ref::<String>()
        .map(|s| s.as_str())
        .or_else(|| info.payload().downcast_ref::<&str>().copied());
    match msg {
        Some(msg) => {
            let s = match info.location() {
                Some(loc) => format!("{} ({}:{})", msg, loc.file(), loc.line()),
                None => msg.to_string(),
            };
            if !ENV_ARGS.nomodal {
                unsafe {
                    MessageBoxW(
                        HWND(0),
                        s.as_str(),
                        TITLE,
                        MB_OK | MB_ICONERROR | MB_SYSTEMMODAL,
                    );
                }
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
    std::process::exit(1);
}

fn set_locale() {
    unsafe {
        let locale = std::ffi::CString::new(LOCALE.as_ref().map_or("", |l| l.as_str())).unwrap();
        libc::setlocale(libc::LC_ALL, locale.as_ptr());
    }
}

fn main() {
    set_logger();
    let default_handler = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        panic_handler(info);
        default_handler(info);
    }));
    info!("start");
    debug!("ENV_ARGS: {:?}", &*ENV_ARGS);
    set_locale();
    let _coinit = coinit::init(coinit::APARTMENTTHREADED | coinit::DISABLE_OLE1DDE).unwrap();
    let th_handle = Rc::new(RefCell::new(None));
    let th_handle_f = th_handle.clone();
    let f = move || -> Result<WindowHandler, Error> {
        let settings = Settings::load(&*SETTINGS_PATH)?;
        debug!("settings: {:?}", settings);
        let mut key_map = KeyboardMap::new();
        key_map.insert(
            vec![wita::VirtualKey::Ctrl, wita::VirtualKey::Char('O')],
            Method::OpenDialog,
        );
        key_map.insert(
            vec![wita::VirtualKey::Ctrl, wita::VirtualKey::Char('F')],
            Method::FrameCounter,
        );
        let (window, window_manager) = WindowHandler::new(&settings, key_map);
        let th_settings = settings;
        let th = std::thread::spawn(move || {
            info!("start rendering thread");
            let _coinit = coinit::init(coinit::MULTITHREADED | coinit::DISABLE_OLE1DDE).unwrap();
            let main_window = window_manager.main_window.clone();
            let handler = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                handler(info);
                main_window.close();
            }));
            let app = Application::new(th_settings, window_manager).and_then(|mut app| app.run());
            if let Err(e) = app {
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
