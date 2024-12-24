use super::*;

/// Implements a Solidity-like Mapping type.
#[derive(Default)]
pub struct Mapping<K, V> {
    id: u64,
    _pd: PhantomData<(K, V)>,
}

impl<K, V> StorageLayout for Mapping<K, V> {
    fn allocate(slot: u64) -> Self {
        Self {
            id: slot,
            _pd: PhantomData::default(),
        }
    }
}

impl<K: SolValue, V: StorageStorable> StorageStorable for Mapping<K, V> {
    fn read(encoded_key: u64) -> Self {
        Self {
            id: encoded_key,
            _pd: PhantomData,
        }
    }

    fn write(&mut self, _key: u64) {
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

    pub fn write(&mut self, key: K, mut value: V) {
        value.write(self.encode_key(key));
    }
}
