mod application;
mod hlsl;
mod renderer;
mod settings;
mod utility;
mod window;

use std::sync::Arc;
use tracing::{error, info, debug};

use application::Application;
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
    info!("start");
    let mut app = Application::new().unwrap();
    app.run().unwrap();
    info!("end");
}
