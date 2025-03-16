use super::*;

/// Implements a Solidity-like Mapping type
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

/// Helper trait for encoding keys
pub trait KeyEncoder {
    fn encode_key(self, base: U256) -> U256;
}

// Implement `KeyEncoder` for any `SolValue` type
impl<K: SolValue> KeyEncoder for K {
    fn encode_key(self, base: U256) -> U256 {
        let key_bytes = self.abi_encode();
        let id_bytes: [u8; 32] = base.to_be_bytes();

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

// Base impl for `Mapping` to encode keys based on its `StorageLayout` ID
impl<K, V> Mapping<K, V> {
    fn encode_key<T: KeyEncoder>(&self, key: T) -> U256 {
        key.encode_key(self.id)
    }
}

// --- KEY-VALUE-STORAGE IMPLEMENTATION ----------------------------------------

impl<K, V> KeyValueStorage for Mapping<K, V>
where
    K: SolValue,
    V: StorageStorable,
{
    type Key = K;
    type Value = V::Value;

    fn read(&self, key: K) -> V::Value {
        let key = self.encode_key(key);
        V::__read(key)
    }

    fn write(&mut self, key: K, value: V::Value) {
        let key = self.encode_key(key);
        V::__write(key, value)
    }
}

impl<K1, K2, V> KeyValueStorage for Mapping<K1, Mapping<K2, V>>
where
    K1: SolValue,
    K2: SolValue,
    V: StorageStorable,
{
    type Key = (K1, K2);
    type Value = V::Value;

    fn read(&self, keys: Self::Key) -> V::Value {
        let (k1, k2) = keys;
        let int = self.encode_key(k1);
        let key = k2.encode_key(int);
        V::__read(key)
    }

    fn write(&mut self, keys: Self::Key, value: V::Value) {
        let (k1, k2) = keys;
        let int = self.encode_key(k1);
        let key = k2.encode_key(int);
        V::__write(key, value)
    }
}

impl<K1, K2, K3, V> KeyValueStorage for Mapping<K1, Mapping<K2, Mapping<K3, V>>>
where
    K1: SolValue,
    K2: SolValue,
    K3: SolValue,
    V: StorageStorable,
{
    type Key = (K1, K2, K3);
    type Value = V::Value;

    fn read(&self, keys: Self::Key) -> V::Value {
        let (k1, k2, k3) = keys;
        let int1 = self.encode_key(k1);
        let int2 = k2.encode_key(int1);
        let key = k3.encode_key(int2);
        V::__read(key)
    }

    fn write(&mut self, keys: Self::Key, value: V::Value) {
        let (k1, k2, k3) = keys;
        let int1 = self.encode_key(k1);
        let int2 = k2.encode_key(int1);
        let key = k3.encode_key(int2);
        V::__write(key, value)
    }
}

// --- CONVENIENCE METHODS ----------------------------------------------------

// No convenience methods needed for the single-key case,
// as the trait impl is directly usable without needing a tuple

impl<K1, K2, V> Mapping<K1, Mapping<K2, V>>
where
    K1: SolValue,
    K2: SolValue,
    V: StorageStorable,
{
    pub fn read(&self, k1: K1, k2: K2) -> V::Value {
        <Self as KeyValueStorage>::read(self, (k1, k2))
    }

    pub fn write(&mut self, k1: K1, k2: K2, value: V::Value) {
        <Self as KeyValueStorage>::write(self, (k1, k2), value)
    }
}

impl<K1, K2, K3, V> Mapping<K1, Mapping<K2, Mapping<K3, V>>>
where
    K1: SolValue,
    K2: SolValue,
    K3: SolValue,
    V: StorageStorable,
{
    pub fn read(&self, k1: K1, k2: K2, k3: K3) -> V::Value {
        <Self as KeyValueStorage>::read(self, (k1, k2, k3))
    }

    pub fn write(&mut self, k1: K1, k2: K2, k3: K3, value: V::Value) {
        <Self as KeyValueStorage>::write(self, (k1, k2, k3), value)
    }
}
