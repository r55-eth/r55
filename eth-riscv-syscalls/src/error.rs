use alloc::borrow::Cow;

/// Error related to syscall convertions
#[derive(Debug, thiserror_no_std::Error)]
pub enum Error {
    #[error("Unknown syscall opcode: {0}")]
    UnknownOpcode(u8),
    #[error("Parse error for syscall string. Input: {input}")]
    ParseError { input: Cow<'static, str> },
}
