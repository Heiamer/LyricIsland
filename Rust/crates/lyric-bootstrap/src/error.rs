use std::fmt;

const CANCEL_TAG: char = '\u{1}';

#[derive(Debug)]
pub struct Error(pub String);

impl Error {
    pub fn cancelled(message: &str) -> Self { Error(format!("{CANCEL_TAG}{message}")) }
    pub fn is_cancelled(&self) -> bool { self.0.starts_with(CANCEL_TAG) }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(self.0.trim_start_matches(CANCEL_TAG)) }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self { Error(value.to_string()) }
}
impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self { Error(value.to_string()) }
}
impl From<lyric_archive::Error> for Error {
    fn from(value: lyric_archive::Error) -> Self { Error(value.0) }
}
impl From<windows::core::Error> for Error {
    fn from(value: windows::core::Error) -> Self { Error(value.message().to_string()) }
}

pub type Result<T> = std::result::Result<T, Error>;

#[macro_export]
macro_rules! fail {
    ($($arg:tt)*) => { return Err($crate::error::Error(format!($($arg)*))) };
}

pub fn context<T>(result: Result<T>, text: &str) -> Result<T> {
    result.map_err(|e| Error(format!("{text}：{e}")))
}
