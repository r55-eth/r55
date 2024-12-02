//! R55 crate errors

use revm::{
    primitives::{EVMError, ExecutionResult, Log},
    Database, InMemoryDB,
};
use rvemu::exception::Exception;

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug)]
pub struct TxResult {
    pub output: Vec<u8>,
    pub logs: Vec<Log>,
    pub gas_used: u64,
    pub status: bool,
}

/// Error encountered on RISC-V execution
#[allow(clippy::enum_variant_names)]
#[derive(Debug, thiserror::Error)]
pub enum Error<DB: Database = InMemoryDB>
where
    DB::Error: std::error::Error + 'static,
{
    /// The exception kind on RISC-V emulator
    #[error("Got RISC-V emulator exception: {0:?}")]
    RvEmuException(Exception),
    /// EVM error
    #[error(transparent)]
    EvmError(#[from] EVMError<DB::Error>),
    /// Error returned when a conversion from a slice to an array fails
    #[error(transparent)]
    TryFromSliceError(#[from] std::array::TryFromSliceError),
    /// Unhandled syscall error
    #[error("Syscall error: {0}")]
    SyscallError(eth_riscv_syscalls::Error),
    /// Unexpected result of the transaction execution error
    #[error("Unexpected result of the transaction execution : {0:?}")]
    UnexpectedExecResult(ExecutionResult),
}

// Note: this `From` implementation here because `rvemu::exception::Exception`
// doesn't implements std error trait.
impl From<Exception> for Error {
    #[inline]
    fn from(exception: Exception) -> Self {
        Self::RvEmuException(exception)
    }
}

// Note: this `From` implementation here because `eth_riscv_syscalls::Error`
// doesn't implements std error trait.
impl From<eth_riscv_syscalls::Error> for Error {
    #[inline]
    fn from(err: eth_riscv_syscalls::Error) -> Self {
        Self::SyscallError(err)
    }
}

impl<E> From<Error> for EVMError<E> {
    #[inline]
    fn from(err: Error) -> Self {
        EVMError::Custom(err.to_string())
    }
}
