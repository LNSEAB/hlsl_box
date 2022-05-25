use crate::*;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

const DEFAULT_SETTINGS: &str = include_str!("default_settings.toml");
const DEFAULT_WINDOW: &str = include_str!("default_window.toml");

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(into = "[u32; 2]")]
pub struct Version {
    pub major: u32,
    pub minor: u32,
}

impl From<Version> for [u32; 2] {
    fn from(src: Version) -> Self {
        [src.major, src.minor]
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(fmt, "{}.{}", self.major, self.minor)
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

impl From<Resolution> for wita::PhysicalSize<u32> {
    fn from(src: Resolution) -> Self {
        Self::new(src.width, src.height)
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Shader {
    pub version: Option<String>,
    pub vs_args: Vec<String>,
    pub ps_args: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ScrollBar {
    pub width: f32,
    pub bg_color: [f32; 4],
    pub thumb_color: [f32; 4],
    pub thumb_hover_color: [f32; 4],
    pub thumb_moving_color: [f32; 4],
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Appearance {
    pub clear_color: [f32; 3],
    pub font: String,
    pub font_size: f32,
    pub text_color: [f32; 4],
    pub background_color: [f32; 4],
    pub error_label_color: [f32; 4],
    pub warn_label_color: [f32; 4],
    pub info_label_color: [f32; 4],
    pub under_line_color: [f32; 4],
    pub scroll_bar: ScrollBar,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    pub version: Version,
    pub frame_counter: bool,
    pub auto_play: bool,
    pub resolution: Resolution,
    pub shader: Shader,
    pub appearance: Appearance,
}

fn load_file(path: &Path, default: &str) -> Result<String, Error> {
    if !path.is_file() {
        let file = File::create(path).map_err(|_| Error::CreateFile)?;
        let mut writer = BufWriter::new(file);
        writer
            .write_all(default.as_bytes())
            .map_err(|_| Error::CreateFile)?;
        info!("create \"{}\"", path.display());
    }
    let file = File::open(path).map_err(|_| Error::ReadFile(path.into()))?;
    let mut reader = BufReader::new(file);
    let mut buffer = String::new();
    reader
        .read_to_string(&mut buffer)
        .map_err(|_| Error::ReadFile(path.into()))?;
    Ok(buffer)
}

fn save_file<T>(path: &Path, this: &T) -> Result<(), Error>
where
    T: serde::Serialize,
{
    let file = File::create(path).map_err(|_| Error::CreateFile)?;
    let mut writer = BufWriter::new(file);
    writer
        .write_all(toml::to_string(this)?.as_bytes())
        .map_err(|_| Error::CreateFile)?;
    Ok(())
}

impl Settings {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Error> {
        Ok(toml::from_str(&load_file(
            path.as_ref(),
            DEFAULT_SETTINGS,
        )?)?)
    }
}

impl Default for Settings {
    fn default() -> Self {
        toml::from_str(DEFAULT_SETTINGS).unwrap()
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Window {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

impl Window {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Error> {
        Ok(toml::from_str(&load_file(path.as_ref(), DEFAULT_WINDOW)?)?)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        save_file(path.as_ref(), self)
    }
}

impl Default for Window {
    fn default() -> Self {
        toml::from_str(DEFAULT_WINDOW).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings() {
        Settings::default();
    }

    #[test]
    fn default_window_setting() {
        Window::default();
    }
}
