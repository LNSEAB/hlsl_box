use crate::*;
use std::path::PathBuf;
use windows::Win32::Graphics::Direct3D::Dxc::*;

struct ErrorMessages {
    read_file: &'static str,
    create_file: &'static str,
    remove_file: &'static str,
    file_too_large: &'static str,
    unsupported_version: &'static str,
    invalid_version: &'static str,
    unexpected_eof: &'static str,
    unknown_error: &'static str,
}

impl ErrorMessages {
    fn new(loc: Option<&str>) -> Self {
        match loc {
            Some("ja-JP") => Self {
                read_file: "ファイルを読み込めません",
                create_file: "ファイルを作成できません",
                remove_file: "ファイルを削除できません",
                file_too_large: "ファイルが大き過ぎます",
                unsupported_version: "サポートされていないバージョンです",
                invalid_version: "settings.tomlにおけるバージョンの書き方に誤りがあります",
                unexpected_eof: "ファイルの途中に終端記号がありました",
                unknown_error: "特定できないエラーです",
            },
            _ => Self {
                read_file: "cannot read the file",
                create_file: "cannot create the file",
                remove_file: "cannot remove the file",
                file_too_large: "file too large",
                unsupported_version: "unsupporrted version",
                invalid_version: "invalid the version written in settings.toml",
                unexpected_eof: "unexpected EOF",
                unknown_error: "unknown error",
            },
        }
    }
}

static ERROR_MESSAGES: Lazy<ErrorMessages> =
    Lazy::new(|| ErrorMessages::new(LOCALE.as_ref().map(|l| l.as_str())));

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Api(#[from] windows::core::Error),
    #[error("mltg: {0}")]
    Mltg(#[from] mltg::Error),
    #[error("{}", .0)]
    Serialize(#[from] toml::ser::Error),
    #[error("{}", .0)]
    Deserialize(#[from] toml::de::Error),
    #[error("{0}")]
    Compile(String),
    #[error("{}({})", ERROR_MESSAGES.read_file, .0.display())]
    ReadFile(PathBuf),
    #[error("{}({})", ERROR_MESSAGES.create_file, .0.display())]
    CreateFile(PathBuf),
    #[error("{}({})", ERROR_MESSAGES.remove_file, .0.display())]
    RemoveFile(PathBuf),
    #[error("{}", ERROR_MESSAGES.file_too_large)]
    FileTooLarge,
    #[error("{}", ERROR_MESSAGES.unsupported_version)]
    UnsupportedVersion,
    #[error("{}", ERROR_MESSAGES.invalid_version)]
    InvalidVersion,
    #[error("{}", ERROR_MESSAGES.unexpected_eof)]
    UnexceptedEof,
    #[error("{}", ERROR_MESSAGES.unknown_error)]
    UnknownError,
    #[error("{}", .0)]
    TestErrorMessage(String),
}

impl From<IDxcBlobUtf8> for Error {
    fn from(src: IDxcBlobUtf8) -> Self {
        unsafe {
            let slice = std::slice::from_raw_parts(
                src.GetBufferPointer() as *const u8,
                src.GetBufferSize() - 1,
            );
            Self::Compile(String::from_utf8_lossy(slice).to_string())
        }
    }
}
