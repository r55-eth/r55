//! RISC-V interpreter crate errors

pub type Result<T> = core::result::Result<T, Error>;

/// Error encountered on RISC-V interpreter setup
#[allow(clippy::enum_variant_names)]
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// [`goblin`] crate error representation
    #[error(transparent)]
    GoblinError(#[from] goblin::error::Error),
}
