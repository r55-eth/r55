#![no_std]

extern crate alloc;

mod error;
pub use error::Error;

macro_rules! syscalls {
    ($(($num:expr, $identifier:ident, $name:expr)),* $(,)?) => {
        #[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
        #[repr(u8)]
        pub enum Syscall {
            $($identifier = $num),*
        }

        impl core::fmt::Display for Syscall {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                write!(f, "{}", match self {
                    $(Syscall::$identifier => $name),*
                })
            }
        }

        impl core::str::FromStr for Syscall {
            type Err = Error;
            fn from_str(input: &str) -> Result<Self, Self::Err> {
                match input {
                    $($name => Ok(Syscall::$identifier)),*,
                    name => Err(Error::ParseError { input: alloc::string::String::from(name).into() }),
                }
            }
        }

        impl From<Syscall> for u8 {
            fn from(syscall: Syscall) -> Self {
                syscall as Self
            }
        }

        impl core::convert::TryFrom<u8> for Syscall {
            type Error = Error;
            fn try_from(value: u8) -> Result<Self, Self::Error> {
                match value {
                    $($num => Ok(Syscall::$identifier)),*,
                    num => Err(Error::UnknownOpcode(num)),
                }
            }
        }
    }
}

// Generate `Syscall` enum with supported syscalls and their ids.
//
// The opcode for each syscall matches the corresponding EVM opcode,
// as described on https://www.evm.codes.
//
// t0: 0x20, opcode for keccak256, a0: offset, a1: size, returns keccak256 hash
// t0: 0x32, opcode for origin, returns an address
// t0: 0x33, opcode for caller, returns an address
// t0: 0x34, opcode for callvalue, a0: first limb, a1: second limb, a2: third limb, a3: fourth limb, returns 256-bit value
// t0: 0x3A, opcode for gasprice, returns 256-bit value
// t0: 0x54, opcode for sload, a0: storage key, returns 64-bit value in a0
// t0: 0x55, opcode for sstore, a0: storage key, a1: storage value, returns nothing
// t0: 0xf1, opcode for call, args: TODO
// t0: 0xf3, opcode for return, a0: memory address of data, a1: length of data in bytes, doesn't return
// t0: 0xfd, opcode for revert, doesn't return
syscalls!(
    (0x20, Keccak256, "keccak256"),
    (0x32, Origin, "origin"),
    (0x33, Caller, "caller"),
    (0x34, CallValue, "callvalue"),
    (0x3A, GasPrice, "gasprice"),
    (0x42, Timestamp, "timestamp"),
    (0x43, Number, "number"),
    (0x45, GasLimit, "gaslimit"),
    (0x46, ChainId, "chainid"),
    (0x48, BaseFee, "basefee"),
    (0x54, SLoad, "sload"),
    (0x55, SStore, "sstore"),
    (0xf1, Call, "call"),
    (0xf3, Return, "return"),
    (0xfd, Revert, "revert"),
);
