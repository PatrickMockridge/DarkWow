use thiserror::Error;
use dwow_sdk::error::ContractError;

#[derive(Error, Debug)]
pub enum EntropyError {
    #[error("Entropy module not yet implemented — see doc/src/contract/entropy.md")]
    NotImplemented,
}

impl From<EntropyError> for ContractError {
    fn from(e: EntropyError) -> Self {
        ContractError::Custom(1 + e as u32)
    }
}
