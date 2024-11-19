use core::default::Default;
use core::marker::PhantomData;

use crate::*;

use alloy_core::primitives::Address;
use alloy_sol_types::{abi::Decoder, SolType, SolValue};

extern crate alloc;
use alloc::vec::Vec;

/// Implements a Solidity-like Mapping type.
#[derive(Default)]
pub struct Mapping<K, V> {
    id: u64,
    pd: PhantomData<(K, V)>,
}

impl<
        K: SolValue,
        V: SolValue + core::convert::From<<<V as SolValue>::SolType as SolType>::RustType> + ?Sized,
    > Mapping<K, V>
{
    pub fn encode_key(&self, key: K) -> u64 {
        let key_bytes = key.abi_encode(); // Is this padded?
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
        let bytes: [u8; 32] = sload(self.encode_key(key)).to_be_bytes();
        V::abi_decode(&bytes, false).unwrap()
    }

    pub fn write(&self, key: K, value: V) {
        let bytes = value.abi_encode();
        let mut padded = [0u8; 32];
        padded[..bytes.len()].copy_from_slice(&bytes);
        sstore(self.encode_key(key), U256::from_be_bytes(padded));
    }
}


