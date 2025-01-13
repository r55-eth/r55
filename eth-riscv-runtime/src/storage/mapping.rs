use super::*;

/// Implements a Solidity-like Mapping type.
#[derive(Default)]
pub struct Mapping<K, V> {
    id: U256,
    _pd: PhantomData<(K, V)>,
}

impl<K, V> StorageLayout for Mapping<K, V> {
    fn allocate(first: u64, second: u64, third: u64, fourth: u64) -> Self {
        Self {
            id: U256::from_limbs([first, second, third, fourth]),
            _pd: PhantomData::default(),
        }
    }
}

impl<K: SolValue, V: StorageStorable> StorageStorable for Mapping<K, V> {
    fn read(encoded_key: U256) -> Self {
        Self {
            id: encoded_key,
            _pd: PhantomData,
        }
    }

    fn write(&mut self, _key: U256) {
        // Mapping types can not directly be written to a storage slot
        // Instead the elements they contain need to be individually written to their own slots
        revert();
    }
}

impl<K: SolValue, V: StorageStorable> Mapping<K, V> {
    pub fn encode_key(&self, key: K) -> U256 {
        let key_bytes = key.abi_encode();
        let id_bytes: [u8; 32] = self.id.to_be_bytes();

        // Concatenate the key bytes and id bytes
        let mut concatenated = Vec::with_capacity(key_bytes.len() + id_bytes.len());
        concatenated.extend_from_slice(&key_bytes);
        concatenated.extend_from_slice(&id_bytes);

        // Call the keccak256 syscall with the concatenated bytes
        let offset = concatenated.as_ptr() as u64;
        let size = concatenated.len() as u64;

        keccak256(offset, size)
    }

    pub fn read(&self, key: K) -> V {
        V::read(self.encode_key(key))
    }

    pub fn write(&mut self, key: K, mut value: V) {
        value.write(self.encode_key(key));
    }
}
