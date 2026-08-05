use dwow_sdk::ContractError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MultiSigError {
    #[error("Group not found")]
    GroupNotFound,

    #[error("Group already exists")]
    GroupAlreadyExists,

    #[error("Invalid threshold: must be >= 1 and <= total keys")]
    InvalidThreshold,

    #[error("Empty key list")]
    EmptyKeyList,

    #[error("Public key not in group")]
    KeyNotInGroup,

    #[error("Duplicate partial signature")]
    DuplicateNullifier,

    #[error("Insufficient signatures for threshold")]
    InsufficientSignatures,

    #[error("Invalid function")]
    InvalidFunction,

}

impl From<MultiSigError> for ContractError {
    fn from(e: MultiSigError) -> Self {
        match e {
            MultiSigError::GroupNotFound => Self::Custom(1),
            MultiSigError::GroupAlreadyExists => Self::Custom(2),
            MultiSigError::InvalidThreshold => Self::Custom(3),
            MultiSigError::EmptyKeyList => Self::Custom(4),
            MultiSigError::KeyNotInGroup => Self::Custom(5),
            MultiSigError::DuplicateNullifier => Self::Custom(6),
            MultiSigError::InsufficientSignatures => Self::Custom(7),
            MultiSigError::InvalidFunction => Self::Custom(8),
        }
    }
}
