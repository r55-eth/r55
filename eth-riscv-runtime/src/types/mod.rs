use core::default::Default;
use core::marker::PhantomData;

use crate::*;

use alloy_sol_types::{SolType, SolValue};

extern crate alloc;
use alloc::vec::Vec;

mod mapping;
pub use mapping::Mapping;

mod slot;
pub use slot::Slot;

/// A trait for storage types that require a dedicated slot in the storage layout
pub trait StorageLayout {
    fn allocate(slot: u64) -> Self;
}

/// A trait for types that can be read from and written to storage slots
pub trait StorageStorable {
    fn read(key: u64) -> Self;
    fn write(&mut self, key: u64);
}

impl<V> StorageStorable for V
where
    V: SolValue + core::convert::From<<<V as SolValue>::SolType as SolType>::RustType>,
{
    fn read(slot: u64) -> Self {
        let bytes: [u8; 32] = sload(slot).to_be_bytes();
        Self::abi_decode(&bytes, false).unwrap_or_else(|_| revert())
    }

    fn write(&mut self, slot: u64) {
        let bytes = self.abi_encode();
        let mut padded = [0u8; 32];
        padded[..bytes.len()].copy_from_slice(&bytes);
        sstore(slot, U256::from_be_bytes(padded));
    }
}
