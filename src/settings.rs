use crate::*;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

const DEFAULT_SETTINGS: &str = include_str!("default_settings.toml");

#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
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

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Window {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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
pub struct Appearance {
    pub clear_color: [f32; 3],
    pub font: String,
    pub font_size: f32,
    pub text_color: [f32; 4],
    pub background_color: [f32; 4],
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    pub version: Version,
    pub frame_counter: bool,
    pub window: Window,
    pub resolution: Resolution,
    pub shader: Shader,
    pub appearance: Appearance,
}

impl Settings {
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        if !path.is_file() {
            let file = File::create(path)?;
            let mut writer = BufWriter::new(file);
            writer.write_all(DEFAULT_SETTINGS.as_bytes())?;
            info!("create \"settings.toml\"");
        }
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let mut buffer = String::new();
        reader.read_to_string(&mut buffer)?;
        Ok(toml::from_str(&buffer)?)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);
        writer.write_all(toml::to_string(self)?.as_bytes())?;
        Ok(())
    }
}

impl Default for Settings {
    fn default() -> Self {
        toml::from_str(DEFAULT_SETTINGS).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings() {
        Settings::default();
    }
}
