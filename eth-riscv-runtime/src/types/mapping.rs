use core::default::Default;
use core::marker::PhantomData;

use super::*;

use alloy_sol_types::{SolType, SolValue};

extern crate alloc;
use alloc::vec::Vec;

/// Implements a Solidity-like Mapping type.
#[derive(Default)]
pub struct Mapping<K, V> {
    id: u64,
    pd: PhantomData<(K, V)>,
}

impl<K: SolValue, V: StorageStorable> StorageStorable for Mapping<K, V> {
    fn read(encoded_key: u64) -> Self {
        Self {
            id: encoded_key,
            pd: PhantomData,
        }
    }

    fn write(&self, _key: u64) {
        // Mapping types can not directly be written to a storage slot
        // Instead the elements they contain need to be individually written to their own slots
        revert();
    }
}

impl<K: SolValue, V: StorageStorable> Mapping<K, V> {
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
        V::read(self.encode_key(key))
    }

    pub fn write(&mut self, key: K, value: V) {
        value.write(self.encode_key(key));
    }
}
