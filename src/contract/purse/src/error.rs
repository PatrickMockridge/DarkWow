use dwow_sdk::error::ContractError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PurseError {
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

impl From<PurseError> for ContractError {
    fn from(e: PurseError) -> Self {
        match e {
            PurseError::InvalidMerkleRoot => Self::Custom(1),
            PurseError::DuplicateNullifier => Self::Custom(2),
            PurseError::DecodeFailure { .. } => Self::Custom(5),
            PurseError::NotAuthorized => Self::Custom(3),
            PurseError::InvalidFunction => Self::Custom(4),
        }
    }
}
