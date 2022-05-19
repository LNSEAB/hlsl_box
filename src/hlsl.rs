use crate::*;
use regex::Regex;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use windows::core::{Interface, GUID, PWSTR};
use windows::Win32::{
    Foundation::E_INVALIDARG,
    Graphics::{Direct3D::Dxc::*, Direct3D12::*},
};

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
    unsafe { DxcCreateInstance(clsid).map_err(|e| e.into()) }
}

fn create_args(
    entry_point: &str,
    target: Target,
    path: Option<&str>,
    opts: &[String],
) -> (Vec<PWSTR>, Vec<Vec<u16>>) {
    static INCLUDE: Lazy<String> =
        Lazy::new(|| EXE_DIR_PATH.join("include").to_string_lossy().to_string());
    let target = target.to_string();
    let mut args = vec![
        "-E",
        entry_point,
        "-T",
        &target,
        "-I",
        &INCLUDE,
        "-I",
        "./include",
    ];
    for opt in opts.iter() {
        args.push(opt.as_ref());
    }
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

const SHADER_MODELS: &[D3D_SHADER_MODEL] = &[
    D3D_SHADER_MODEL_5_1,
    D3D_SHADER_MODEL_6_0,
    D3D_SHADER_MODEL_6_1,
    D3D_SHADER_MODEL_6_2,
    D3D_SHADER_MODEL_6_3,
    D3D_SHADER_MODEL_6_4,
    D3D_SHADER_MODEL_6_5,
    D3D_SHADER_MODEL_6_6,
    D3D_SHADER_MODEL_6_7,
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ShaderModel(D3D_SHADER_MODEL);

impl ShaderModel {
    pub fn new<T>(device: &ID3D12Device, version: Option<&T>) -> Result<Self, Error>
    where
        T: AsRef<str>,
    {
        version.map_or_else(
            || Self::highest(device),
            |version| Self::specify(version.as_ref()),
        )
    }

    fn specify(version: &str) -> Result<Self, Error> {
        let re = Regex::new(r"(\d+)_(\d+)").unwrap();
        let cap = re.captures(version).ok_or(Error::InvalidVersion)?;
        let v = i32::from_str_radix(&format!("{}{}", &cap[1], &cap[2]), 16).unwrap();
        if !SHADER_MODELS.contains(&D3D_SHADER_MODEL(v)) {
            return Err(Error::UnsupportedVersion);
        }
        Ok(Self(D3D_SHADER_MODEL(v)))
    }

    fn highest(device: &ID3D12Device) -> Result<Self, Error> {
        unsafe {
            let mut data = D3D12_FEATURE_DATA_SHADER_MODEL::default();
            for sm in SHADER_MODELS.iter().rev() {
                data.HighestShaderModel = *sm;
                let ret = device.CheckFeatureSupport(
                    D3D12_FEATURE_SHADER_MODEL,
                    &mut data as *mut _ as _,
                    std::mem::size_of_val(&data) as _,
                );
                match ret {
                    Ok(_) => return Ok(Self(data.HighestShaderModel)),
                    Err(e) if e.code() != E_INVALIDARG => return Err(e.into()),
                    _ => {}
                }
            }
            Err(Error::UnsupportedVersion)
        }
    }

    fn as_str(&self) -> &str {
        match self.0 {
            D3D_SHADER_MODEL_5_1 => "5_1",
            D3D_SHADER_MODEL_6_0 => "6_0",
            D3D_SHADER_MODEL_6_1 => "6_1",
            D3D_SHADER_MODEL_6_2 => "6_2",
            D3D_SHADER_MODEL_6_3 => "6_3",
            D3D_SHADER_MODEL_6_4 => "6_4",
            D3D_SHADER_MODEL_6_5 => "6_5",
            D3D_SHADER_MODEL_6_6 => "6_6",
            D3D_SHADER_MODEL_6_7 => "6_7",
            _ => unimplemented!(),
        }
    }
}

impl std::fmt::Display for ShaderModel {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(fmt, "{}", self.as_str())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    VS(ShaderModel),
    PS(ShaderModel),
}

impl ToString for Target {
    fn to_string(&self) -> String {
        match self {
            Self::VS(version) => format!("vs_{}", version),
            Self::PS(version) => format!("ps_{}", version),
        }
    }
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
        if data.chars().any(|c| c == '\0') {
            return Err(Error::UnexceptedEof);
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
                        args,
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
        target: Target,
        args: &[String],
    ) -> Result<Blob, Error> {
        let (args, _tmp) = create_args(entry_point, target, None, args);
        self.compile_impl(data, &args)
    }

    pub fn compile_from_file(
        &self,
        path: impl AsRef<Path>,
        entry_point: &str,
        target: Target,
        args: &[String],
    ) -> Result<Blob, Error> {
        let path = path.as_ref();
        let data = {
            let file = File::open(path).map_err(|_| Error::ReadFile(path.into()))?;
            let mut reader = BufReader::new(file);
            let mut data = String::new();
            reader
                .read_to_string(&mut data)
                .map_err(|_| Error::ReadFile(path.into()))?;
            data
        };
        let (args, _tmp) = create_args(entry_point, target, path.to_str(), args);
        self.compile_impl(&data, &args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATA_PATH: &'static str = "src/shader/copy_texture.hlsl";
    const DATA: &'static str = include_str!("shader/copy_texture.hlsl");

    #[test]
    fn compile_from_str() {
        let compiler = Compiler::new().unwrap();
        let version = ShaderModel::specify("6_0").unwrap();
        compiler
            .compile_from_str(DATA, "vs_main", Target::VS(version), &[])
            .unwrap();
        compiler
            .compile_from_str(DATA, "ps_main", Target::PS(version), &[])
            .unwrap();
    }

    #[test]
    fn compile_from_file() {
        let compiler = Compiler::new().unwrap();
        let version = ShaderModel::specify("6_0").unwrap();
        compiler
            .compile_from_file(DATA_PATH, "vs_main", Target::VS(version), &[])
            .unwrap();
        compiler
            .compile_from_file(DATA_PATH, "ps_main", Target::PS(version), &[])
            .unwrap();
    }

    #[test]
    fn specify_target_version() {
        assert!(ShaderModel::specify("6_0").is_ok());
        assert!(ShaderModel::specify("5_0").is_err());
    }

    #[test]
    fn highest_target_version() {
        use windows::Win32::Graphics::{Direct3D::*, Dxgi::*};

        let adapter: IDXGIAdapter = unsafe {
            let factory: IDXGIFactory4 = CreateDXGIFactory1().unwrap();
            factory.EnumWarpAdapter().unwrap()
        };
        let device = unsafe {
            let mut device: Option<ID3D12Device> = None;
            D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_12_1, &mut device).unwrap();
            device.unwrap()
        };
        let version = ShaderModel::highest(&device).unwrap();
        assert!(version.0 .0 >= D3D_SHADER_MODEL_5_1.0);
    }
}
