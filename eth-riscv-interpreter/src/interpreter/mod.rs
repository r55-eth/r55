use thiserror::Error;
use core::result::Result;

#[cfg(feature = "std")]
use std::string::String;
#[cfg(feature = "std")]
use std::vec::Vec;

#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

/// Errors that can occur during interpreter operations
#[derive(Debug, Error)]
pub enum InterpreterError {
    #[error("Failed to load ELF: {0}")]
    ElfLoadError(String),
    
    #[error("Memory access error: {0}")]
    MemoryError(String),
    
    #[error("Register access error: {0}")]
    RegisterError(String),
    
    #[error("Emulation error: {0}")]
    EmulationError(String),
    
    #[error("Invalid syscall: {0}")]
    SyscallError(String),
}

/// Trait defining the necessary interface for a RISC-V interpreter
pub trait RiscVInterpreter {
    /// Load an ELF program into the interpreter's memory
    fn load_elf(&mut self, elf_data: &[u8]) -> Result<(), InterpreterError>;
    
    /// Read from a register
    fn read_register(&self, reg: u32) -> Result<u64, InterpreterError>;
    
    /// Write to a register
    fn write_register(&mut self, reg: u32, value: u64) -> Result<(), InterpreterError>;
    
    /// Read from memory
    fn read_memory(&self, address: u64, size: usize) -> Result<Vec<u8>, InterpreterError>;
    
    /// Write to memory
    fn write_memory(&mut self, address: u64, data: &[u8]) -> Result<(), InterpreterError>;
    
    /// Add a system call interrupt hook
    fn add_syscall_hook<F>(&mut self, syscall_num: u64, handler: F) -> Result<(), InterpreterError>
    where
        F: FnMut(&mut Self, u64) -> Result<u64, InterpreterError> + 'static;
    
    /// Start emulation from a given address
    fn start(&mut self, start_addr: u64, end_addr: u64) -> Result<(), InterpreterError>;
}

#[cfg(feature = "rvemu-backend")]
pub mod rvemu;

#[cfg(feature = "unicorn-backend")]
pub mod unicorn;