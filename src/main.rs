#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod application;
mod hlsl;
mod monitor;
mod renderer;
mod settings;
mod utility;
mod window;

use std::sync::{mpsc, Arc};
use tracing::{debug, error, info};

use application::Application;
use monitor::*;
use renderer::*;
use settings::Settings;
use utility::*;
use window::*;

const SETTINGS_PATH: &str = "./settings.toml";

fn logger() {
    use std::fs::File;
    use tracing_subscriber::{filter::LevelFilter, prelude::*};

    let file = tracing_subscriber::fmt::layer()
        .compact()
        .with_writer(Arc::new(File::create("hlsl_box.log").unwrap()))
        .with_ansi(false)
        .with_line_number(true)
        .with_filter(LevelFilter::DEBUG);
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
    let _coinit = coinit::init(coinit::APARTMENTTHREADED | coinit::DISABLE_OLE1DDE).unwrap();
    let (th_tx, th_rx) = mpsc::channel();
    info!("start");
    let f = move || -> anyhow::Result<Window> {
        let settings = Arc::new(Settings::load(SETTINGS_PATH)?);
        let (window, window_receiver) = Window::new(settings.clone())?;
        let th_settings = settings.clone();
        let th = std::thread::spawn(move || {
            info!("start rendering thread");
            let _coinit = coinit::init(coinit::MULTITHREADED | coinit::DISABLE_OLE1DDE).unwrap();
            let app = Application::new(th_settings, window_receiver).and_then(|mut app| app.run());
            if let Err(e) = app {
                error!("{}\n{}", e, e.backtrace());
            }
            info!("end rendering thread");
        });
        th_tx.send(th).ok();
        Ok(window)
    };
    if let Err(e) = wita::run(wita::RunType::Wait, f) {
        error!("{}", e);
    }
    th_rx.recv().unwrap().join().unwrap();
    info!("end");
}
