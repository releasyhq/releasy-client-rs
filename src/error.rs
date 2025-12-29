use std::fmt;

use crate::models::ErrorBody;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Api {
        status: u16,
        error: Option<ErrorBody>,
        body: Option<String>,
    },
    Transport(ureq::Error),
    InvalidBaseUrl(String),
    MissingLocationHeader,
}

impl Error {
    pub fn status(&self) -> Option<u16> {
        match self {
            Error::Api { status, .. } => Some(*status),
            _ => None,
        }
    }

    pub fn api_error(&self) -> Option<&ErrorBody> {
        match self {
            Error::Api { error, .. } => error.as_ref(),
            _ => None,
        }
    }

    pub fn body(&self) -> Option<&str> {
        match self {
            Error::Api { body, .. } => body.as_deref(),
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Api {
                status,
                error: Some(error),
                ..
            } => write!(
                f,
                "api error (status {}): {} ({})",
                status, error.error.code, error.error.message
            ),
            Error::Api { status, .. } => write!(f, "api error (status {})", status),
            Error::Transport(err) => write!(f, "transport error: {}", err),
            Error::InvalidBaseUrl(url) => write!(f, "invalid base url: {}", url),
            Error::MissingLocationHeader => {
                write!(f, "missing Location header in redirect response")
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<ureq::Error> for Error {
    fn from(err: ureq::Error) -> Self {
        Error::Transport(err)
    }
}
