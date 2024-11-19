use core::default::Default;
use core::marker::PhantomData;

use crate::*;

use alloy_sol_types::{SolType, SolValue};

extern crate alloc;
use alloc::vec::Vec;

/// Implements a Solidity-like Mapping type.
#[derive(Default)]
pub struct Mapping<K, V> {
    id: u64,
    pd: PhantomData<(K, V)>,
}

/// A trait for types that can be read from and written to storage slots
pub trait SlotStorable {
    fn read_from_storage(key: u64) -> Self;
    fn write_to_storage(&self, key: u64);
}

impl<V> SlotStorable for V
where
    V: SolValue + core::convert::From<<<V as SolValue>::SolType as SolType>::RustType>,
{
    fn read_from_storage(encoded_key: u64) -> Self {
        let bytes: [u8; 32] = sload(encoded_key).to_be_bytes();
        Self::abi_decode(&bytes, false).unwrap_or_else(|_| revert())
    }

    fn write_to_storage(&self, key: u64) {
        let bytes = self.abi_encode();
        let mut padded = [0u8; 32];
        padded[..bytes.len()].copy_from_slice(&bytes);
        sstore(key, U256::from_be_bytes(padded));
    }
}

impl<K: SolValue, V: SlotStorable> SlotStorable for Mapping<K, V> {
    fn read_from_storage(encoded_key: u64) -> Self {
        Self {
            id: encoded_key,
            pd: PhantomData,
        }
    }

    fn write_to_storage(&self, _key: u64) {
        // Mapping types can not directly be written to a storage slot
        // Instead the elements they contain, need to be individually written to their own slots
        revert();
    }
}

impl<K: SolValue, V: SlotStorable> Mapping<K, V> {
    pub fn encode_key(&self, key: K) -> u64 {
        let key_bytes = key.abi_encode();
        let id_bytes = self.id.to_le_bytes();

        // Concatenate the key bytes and id bytes
        let mut concatenated = Vec::with_capacity(key_bytes.len() + id_bytes.len());
        concatenated.extend_from_slice(&key_bytes);
        concatenated.extend_from_slice(&id_bytes);

        // Call the keccak256 syscall with the concatenated bytes
        let offset = concatenated.as_ptr() as u64;
        let size = concatenated.len() as u64;
        let output = keccak256(offset, size);

        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&output[..8]);
        u64::from_le_bytes(bytes)
    }

    pub fn read(&self, key: K) -> V {
        V::read_from_storage(self.encode_key(key))
    }

    pub fn write(&self, key: K, value: V) {
        value.write_to_storage(self.encode_key(key));
    }
}
