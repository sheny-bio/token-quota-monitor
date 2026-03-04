use std::fmt;

#[derive(Debug)]
pub enum Error {
    ConfigNotFound(String),
    AccountNotFound(String),
    NoDefaultAccount,
    Api(reqwest::Error),
    ApiMessage(String),
    SessionExpired(String),
    Cache(String),
    Io(std::io::Error),
    Json(serde_json::Error),
    Toml(toml::de::Error),
    TomlSer(toml::ser::Error),
    UrlParse(url::ParseError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::ConfigNotFound(p) => write!(f, "Config not found: {p}"),
            Error::AccountNotFound(n) => write!(f, "Account not found: {n}"),
            Error::NoDefaultAccount => write!(f, "No default account configured"),
            Error::Api(e) => write!(f, "API error: {e}"),
            Error::ApiMessage(m) => write!(f, "API error: {m}"),
            Error::SessionExpired(n) => write!(f, "Session expired, run: tqm login {n}"),
            Error::Cache(m) => write!(f, "Cache error: {m}"),
            Error::Io(e) => write!(f, "IO error: {e}"),
            Error::Json(e) => write!(f, "JSON error: {e}"),
            Error::Toml(e) => write!(f, "TOML parse error: {e}"),
            Error::TomlSer(e) => write!(f, "TOML serialize error: {e}"),
            Error::UrlParse(e) => write!(f, "URL parse error: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self { Error::Api(e) }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self { Error::Io(e) }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self { Error::Json(e) }
}
impl From<toml::de::Error> for Error {
    fn from(e: toml::de::Error) -> Self { Error::Toml(e) }
}
impl From<toml::ser::Error> for Error {
    fn from(e: toml::ser::Error) -> Self { Error::TomlSer(e) }
}
impl From<url::ParseError> for Error {
    fn from(e: url::ParseError) -> Self { Error::UrlParse(e) }
}

pub type Result<T> = std::result::Result<T, Error>;
