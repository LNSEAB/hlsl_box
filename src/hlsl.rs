use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use windows::core::{Interface, GUID, PWSTR};
use windows::Win32::{Graphics::Direct3D::Dxc::*, Graphics::Direct3D12::D3D12_SHADER_BYTECODE};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{}", .0)]
    Io(std::io::Error),
    #[error("{}", .0)]
    Api(windows::core::Error),
    #[error("{}", .0)]
    Compile(String),
    #[error("file too large")]
    FileTooLarge,
}

impl From<std::io::Error> for Error {
    fn from(src: std::io::Error) -> Self {
        Self::Io(src)
    }
}

impl From<std::io::ErrorKind> for Error {
    fn from(src: std::io::ErrorKind) -> Self {
        Self::Io(src.into())
    }
}

impl From<windows::core::Error> for Error {
    fn from(src: windows::core::Error) -> Self {
        Self::Api(src)
    }
}

impl From<IDxcBlobUtf8> for Error {
    fn from(src: IDxcBlobUtf8) -> Self {
        unsafe {
            let slice = std::slice::from_raw_parts(
                src.GetBufferPointer() as *const u8,
                src.GetBufferSize(),
            );
            Self::Compile(String::from_utf8_lossy(slice).to_string())
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct Blob(IDxcBlob);

impl Blob {
    pub fn as_shader_bytecode(&self) -> D3D12_SHADER_BYTECODE {
        unsafe {
            D3D12_SHADER_BYTECODE {
                pShaderBytecode: self.0.GetBufferPointer() as _,
                BytecodeLength: self.0.GetBufferSize(),
            }
        }
    }
}

fn create_instance<T: Interface>(clsid: &GUID) -> Result<T, Error> {
    unsafe {
        let mut obj: Option<T> = None;
        DxcCreateInstance(clsid, &T::IID, &mut obj as *mut _ as _)
            .map(|_| obj.unwrap())
            .map_err(|e| e.into())
    }
}

fn create_args(entry_point: &str, target: &str, path: Option<&str>) -> (Vec<PWSTR>, Vec<Vec<u16>>) {
    let mut args = vec!["-E", entry_point, "-T", target, "-I", "./include"];
    if let Some(path) = path {
        args.push(path);
    }
    let mut tmp = args
        .iter()
        .map(|a| a.encode_utf16().chain(Some(0)).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let args = tmp
        .iter_mut()
        .map(|t| PWSTR(t.as_mut_ptr()))
        .collect::<Vec<_>>();
    (args, tmp)
}

pub struct Compiler {
    utils: IDxcUtils,
    compiler: IDxcCompiler3,
    default_include_handler: IDxcIncludeHandler,
}

impl Compiler {
    pub fn new() -> Result<Self, Error> {
        unsafe {
            let utils: IDxcUtils = create_instance(&CLSID_DxcLibrary)?;
            let compiler: IDxcCompiler3 = create_instance(&CLSID_DxcCompiler)?;
            let default_include_handler = utils.CreateDefaultIncludeHandler()?;
            Ok(Self {
                utils,
                compiler,
                default_include_handler,
            })
        }
    }

    fn compile_impl(&self, data: &str, args: &[PWSTR]) -> Result<Blob, Error> {
        if data.bytes().len() >= u32::MAX as _ {
            return Err(Error::FileTooLarge);
        }
        if data.chars().find(|&c| c == '\0').is_some() {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        unsafe {
            let src =
                self.utils
                    .CreateBlob(data.as_ptr() as _, data.bytes().len() as _, DXC_CP_UTF8)?;
            let buffer = DxcBuffer {
                Ptr: src.GetBufferPointer(),
                Size: src.GetBufferSize(),
                Encoding: DXC_CP_UTF8.0,
            };
            let result = {
                let mut result: Option<IDxcResult> = None;
                self.compiler
                    .Compile(
                        &buffer,
                        &args,
                        &self.default_include_handler,
                        &IDxcResult::IID,
                        &mut result as *mut _ as _,
                    )
                    .map(|_| result.unwrap())?
            };
            if let Err(e) = result.GetStatus()?.ok() {
                let mut blob: Option<IDxcBlobUtf8> = None;
                result.GetOutput(
                    DXC_OUT_ERRORS,
                    &IDxcBlobUtf8::IID,
                    &mut blob as *mut _ as _,
                    std::ptr::null_mut(),
                )?;
                return Err(blob.map_or_else(|| e.into(), |b| b.into()));
            }
            let mut blob: Option<IDxcBlob> = None;
            result.GetOutput(
                DXC_OUT_OBJECT,
                &IDxcBlob::IID,
                &mut blob as *mut _ as _,
                std::ptr::null_mut(),
            )?;
            Ok(Blob(blob.unwrap()))
        }
    }

    pub fn compile_from_str(
        &self,
        data: &str,
        entry_point: &str,
        target: &str,
    ) -> Result<Blob, Error> {
        let (args, _tmp) = create_args(entry_point, target, None);
        self.compile_impl(data, &args)
    }

    pub fn compile_from_file(
        &self,
        path: impl AsRef<Path>,
        entry_point: &str,
        target: &str,
    ) -> Result<Blob, Error> {
        let path = path.as_ref();
        let data = {
            let file = File::open(path)?;
            let mut reader = BufReader::new(file);
            let mut data = String::new();
            reader.read_to_string(&mut data)?;
            data
        };
        let (args, _tmp) = create_args(entry_point, target, path.to_str());
        self.compile_impl(&data, &args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_from_str() {
        let compiler = Compiler::new().unwrap();
        let data = include_str!("shader/test.hlsl");
        compiler
            .compile_from_str(data, "vs_main", "vs_6_0")
            .unwrap();
        compiler
            .compile_from_str(data, "ps_main", "ps_6_0")
            .unwrap();
    }

    #[test]
    fn compile_from_file() {
        let compiler = Compiler::new().unwrap();
        let path = "src/shader/test.hlsl";
        compiler
            .compile_from_file(path, "vs_main", "vs_6_0")
            .unwrap();
        compiler
            .compile_from_file(path, "ps_main", "ps_6_0")
            .unwrap();
    }
}
