#![no_std]

macro_rules! syscalls {
    ($(($num:expr, $identifier:ident, $name:expr)),* $(,)?) => {
        #[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
        #[repr(u32)]
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
            type Err = ();
            fn from_str(input: &str) -> Result<Self, Self::Err> {
                match input {
                    $($name => Ok(Syscall::$identifier)),*,
                    _ => Err(()),
                }
            }
        }

        impl From<Syscall> for u32 {
            fn from(syscall: Syscall) -> Self {
                syscall as Self
            }
        }

        impl core::convert::TryFrom<u32> for Syscall {
            type Error = ();
            fn try_from(value: u32) -> Result<Self, Self::Error> {
                match value {
                    $($num => Ok(Syscall::$identifier)),*,
                    _ => Err(()),
                }
            }
        }
    }
}

// Generate `Syscall` enum with supported syscalls and their numbers.
// t0: 0, opcode for return, a0: memory address of data, a1: length of data, in bytes, doesn't return
// t0: 1, opcode for sload, a0: storage key, returns 64-bit value in a0
// t0: 2, opcode for sstore, a0: storage key, a1: storage value, returns nothing
// t0: 3, opcode for call, args: TODO
// t0: 4, opcode for revert, doesn't return
// t0: 6, opcode for log, a0: data_ptr, a1: data_size, a2: topics_ptr, a3: topics_size, returns nothing
syscalls!(
    (0, Return, "return"),
    (1, SLoad, "sload"),
    (2, SStore, "sstore"),
    (3, Call, "call"),
    (4, Revert, "revert"),
    (5, Caller, "caller"),
    (0xA0, Log, "log"),
);
