use dwow_sdk::error::ContractError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BoxError {
    #[error("Invalid Merkle root")]
    InvalidMerkleRoot,
    #[error("Duplicate nullifier")]
    DuplicateNullifier,
    #[error("Parameter decode failure: {field}")]
    DecodeFailure { field: String },
    #[error("Not authorized")]
    NotAuthorized,
    #[error("Invalid function or parameters")]
    InvalidFunction,
}

impl From<BoxError> for ContractError {
    fn from(e: BoxError) -> Self {
        match e {
            BoxError::InvalidMerkleRoot => Self::Custom(1),
            BoxError::DuplicateNullifier => Self::Custom(2),
            BoxError::DecodeFailure { .. } => Self::Custom(5),
            BoxError::NotAuthorized => Self::Custom(3),
            BoxError::InvalidFunction => Self::Custom(4),
        }
    }
}
