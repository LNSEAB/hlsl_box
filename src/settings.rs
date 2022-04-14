use crate::*;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Window {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Shader {
    pub ps: String,
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
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Arc<Self>> {
        let path = path.as_ref();
        if !path.is_file() {
            let file = File::create(path)?;
            let mut writer = BufWriter::new(file);
            writer.write(include_str!("default_settings.toml").as_bytes())?;
            info!("create \"settings.toml\"");
        }
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let mut buffer = String::new();
        reader.read_to_string(&mut buffer)?;
        Ok(Arc::new(toml::from_str(&buffer)?))
    }

    pub fn save(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);
        writer.write(toml::to_string(self)?.as_bytes())?;
        Ok(())
    }
}
