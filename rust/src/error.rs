use thiserror::Error;

#[derive(Error, Debug)]
pub enum AimarketError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Protocol error: {0}")]
    Protocol(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Payment error: {0}")]
    Payment(String),
    #[error("Safety blocked: {0}")]
    Safety(String),
}

impl AimarketError {
    pub fn status_code(&self) -> Option<u16> {
        match self {
            Self::Payment(_) => Some(402),
            Self::Safety(_) => Some(403),
            _ => None,
        }
    }
}
