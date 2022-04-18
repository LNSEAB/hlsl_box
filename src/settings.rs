use crate::*;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

const DEFAULT_SETTINGS: &'static str = include_str!("default_settings.toml");

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct General {
    pub version: Version,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Window {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Shader {
    pub version: Option<String>,
    pub vs_args: Vec<String>,
    pub ps_args: Vec<String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Appearance {
    pub clear_color: [f32; 3],
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    pub window: Window,
    pub shader: Shader,
    pub appearance: Appearance,
}

impl Settings {
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        if !path.is_file() {
            let file = File::create(path)?;
            let mut writer = BufWriter::new(file);
            writer.write(DEFAULT_SETTINGS.as_bytes())?;
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
        writer.write(toml::to_string(self)?.as_bytes())?;
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
