mod mapping;
pub use mapping::Mapping;

mod word;
pub use word::Word;

use crate::*;
use alloy_sol_types::{SolType, SolValue};

/// A trait for types that can be read from and written to storage slots
pub trait StorageStorable {
    fn read(key: u64) -> Self;
    fn write(&self, key: u64);
}

impl<V> StorageStorable for V
where
    V: SolValue + core::convert::From<<<V as SolValue>::SolType as SolType>::RustType>,
{
    fn read(encoded_key: u64) -> Self {
        let bytes: [u8; 32] = sload(encoded_key).to_be_bytes();
        Self::abi_decode(&bytes, false).unwrap_or_else(|_| revert())
    }

    fn write(&self, key: u64) {
        let bytes = self.abi_encode();
        let mut padded = [0u8; 32];
        padded[..bytes.len()].copy_from_slice(&bytes);
        sstore(key, U256::from_be_bytes(padded));
    }
}
