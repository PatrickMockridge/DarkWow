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
    // `NotAuthorized` (`Custom(3)`) was retired here. It was declared for a host-level owner check that
    // was never written and **cannot be written**: a deposit or withdrawal carries no owner to check,
    // and the owner the circuit binds is not recoverable from the leaf. To construct this variant a
    // caller-supplied owner field would have to be added to a genesis contract's call data, which is
    // what `privacy.md` §2 and §5.5 exist to prevent — and it would be a second source of truth beside
    // the proof that already binds the owner (`OBL-C101`'s argument). The number is left unassigned
    // rather than reused, so a recorded code keeps its meaning.
    #[error("Invalid function or parameters")]
    InvalidFunction,
}

impl From<PurseError> for ContractError {
    fn from(e: PurseError) -> Self {
        match e {
            PurseError::InvalidMerkleRoot => Self::Custom(1),
            PurseError::DuplicateNullifier => Self::Custom(2),
            PurseError::DecodeFailure { .. } => Self::Custom(5),
            PurseError::InvalidFunction => Self::Custom(4),
        }
    }
}
