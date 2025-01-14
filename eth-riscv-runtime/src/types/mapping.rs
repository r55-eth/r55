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

impl<K, V> Mapping<K, V>
where
    K: SolValue,
{
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
}

// Implementation for mappings that store direct storage values
impl<K, T, V> KeyValueStorage<K> for Mapping<K, T>
where
    K: SolValue,
    T: DirectStorage<V> + StorageStorable<Value = V>,
    V: SolValue + core::convert::From<<<V as SolValue>::SolType as SolType>::RustType>,
{
    type ReadValue = V;
    type WriteValue = V;

    fn read(&self, key: K) -> Self::ReadValue {
        T::__read(self.encode_key(key))
    }

    fn write(&mut self, key: K, value: Self::WriteValue) {
        T::__write(self.encode_key(key), value)
    }
}

// Implementation for nested mappings
impl<K1, K2, V> KeyValueStorage<K1> for Mapping<K1, Mapping<K2, V>>
where
    K1: SolValue,
{
    type ReadValue = Mapping<K2, V>;
    type WriteValue = ();

    fn read(&self, key: K1) -> Self::ReadValue {
        Mapping {
            id: self.encode_key(key),
            _pd: PhantomData,
        }
    }

    fn write(&mut self, _key: K1, _value: Self::WriteValue) {
        // Mappings that store other mappings cannot be written to directly
        revert();
    }
}
