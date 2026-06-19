use dwow_sdk::error::ContractError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PurseError {
    #[error("Purse not found")]
    PurseNotFound,
    #[error("Insufficient balance")]
    InsufficientBalance,
    #[error("Invalid deposit amount")]
    InvalidDepositAmount,
    #[error("Invalid withdraw amount")]
    InvalidWithdrawAmount,
    #[error("Duplicate nullifier")]
    DuplicateNullifier,
    #[error("Not authorized")]
    NotAuthorized,
    #[error("Invalid function or parameters")]
    InvalidFunction,
}

impl From<PurseError> for ContractError {
    fn from(e: PurseError) -> Self {
        match e {
            PurseError::PurseNotFound => Self::Custom(1),
            PurseError::InsufficientBalance => Self::Custom(2),
            PurseError::InvalidDepositAmount => Self::Custom(3),
            PurseError::InvalidWithdrawAmount => Self::Custom(4),
            PurseError::DuplicateNullifier => Self::Custom(5),
            PurseError::NotAuthorized => Self::Custom(6),
            PurseError::InvalidFunction => Self::Custom(7),
        }
    }
}
