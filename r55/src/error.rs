//! R55 crate errors

use core::fmt;

use alloy_primitives::{keccak256, Bytes};
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
    RvEmuException(Exception),
    /// EVM error
    EvmError(#[from] EVMError<DB::Error>),
    /// Error returned when a conversion from a slice to an array fails
    TryFromSliceError(#[from] std::array::TryFromSliceError),
    /// Unhandled syscall error
    SyscallError(eth_riscv_syscalls::Error),
    /// Unexpected result of the transaction execution error
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

impl fmt::Display for TxResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Tx Result:\n> success: {}\n> gas used: {}\n> outcome: {}\n> logs: {:#?}\n",
            self.status,
            self.gas_used,
            revm::primitives::Bytes::from(self.output.clone()),
            self.logs,
        )
    }
}

impl<DB: Database> fmt::Display for Error<DB>
where
    DB::Error: std::error::Error + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedExecResult(ExecutionResult::Revert { gas_used, output }) => {
                write!(
                    f,
                    "Unexpected result of the transaction execution:\n REVERT:\n > output [hex]: {}\n > output [str]: {}\n > gas used: {}",
                    output,
                    String::from_utf8(output.to_vec()).unwrap_or_default(),
                    gas_used
                )
            }
            Self::RvEmuException(e) => write!(f, "Got RISC-V emulator exception: {:?}", e),
            Self::EvmError(e) => write!(f, "{}", e),
            Self::TryFromSliceError(e) => write!(f, "{}", e),
            Self::SyscallError(e) => write!(f, "Syscall error: {}", e),
            Self::UnexpectedExecResult(other) => write!(
                f,
                "Unexpected result of the transaction execution: {:?}",
                other
            ),
        }
    }
}

impl<DB: Database> Error<DB>
where
    DB::Error: std::error::Error + 'static,
{
    pub fn matches_string_error(&self, err: &'static str) -> bool {
        if let Error::UnexpectedExecResult(ExecutionResult::Revert {
            gas_used: _,
            output,
        }) = &self
        {
            if &Bytes::from(err) != output {
                return false;
            }

            true
        } else {
            false
        }
    }

    pub fn matches_custom_error(&self, err: &'static str) -> bool {
        if let Error::UnexpectedExecResult(ExecutionResult::Revert {
            gas_used: _,
            output,
        }) = &self
        {
            if keccak256(err)[..4].to_vec() != output[..4] {
                return false;
            }

            true
        } else {
            false
        }
    }

    pub fn matches_custom_error_with_args(&self, err: &'static str, args: Vec<u8>) -> bool {
        if let Error::UnexpectedExecResult(ExecutionResult::Revert {
            gas_used: _,
            output,
        }) = &self
        {
            let err = Bytes::from(keccak256(err)[..4].to_vec());
            if err != output[..4] {
                return false;
            }

            if !args.is_empty() && output[4..].to_vec() != args {
                return false;
            }

            true
        } else {
            false
        }
    }
}
