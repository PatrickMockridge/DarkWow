use dwow_sdk::error::ContractError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BoxError {
    #[error("Box not found")]
    BoxNotFound,
    #[error("Box is not empty")]
    BoxNotEmpty,
    #[error("Box is empty")]
    BoxEmpty,
    #[error("Duplicate nullifier")]
    DuplicateNullifier,
    #[error("Not authorized")]
    NotAuthorized,
    #[error("Invalid function or parameters")]
    InvalidFunction,
}

impl From<BoxError> for ContractError {
    fn from(e: BoxError) -> Self {
        match e {
            BoxError::BoxNotFound => Self::Custom(1),
            BoxError::BoxNotEmpty => Self::Custom(2),
            BoxError::BoxEmpty => Self::Custom(3),
            BoxError::DuplicateNullifier => Self::Custom(4),
            BoxError::NotAuthorized => Self::Custom(5),
            BoxError::InvalidFunction => Self::Custom(6),
        }
    }
}
