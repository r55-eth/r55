use core::{
    alloc::{GlobalAlloc, Layout},
    marker::PhantomData,
    ops::{Deref, DerefMut, Index, IndexMut}, 
};

use crate::alloc::GLOBAL;

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
    fn encode_key(&self, key: K) -> U256 {
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

/// A guard that manages state interactions for Solidity-like mappings.
/// 
/// This type is returned when indexing into a `Mapping` and provides methods
/// to read/write from the underlying storage location.
pub struct MappingGuard<V>
where
    V: StorageStorable,
    V::Value: SolValue + core::convert::From<<<V::Value as SolValue>::SolType as SolType>::RustType>,
{
    storage_key: U256,
    _phantom: PhantomData<V>,
}

impl<V> MappingGuard<V>
where
    V: StorageStorable,
    V::Value: SolValue + core::convert::From<<<V::Value as SolValue>::SolType as SolType>::RustType>,
{
    pub fn new(storage_key: U256) -> Self {
        Self {
            storage_key,
            _phantom: PhantomData,
        }
    }
}

impl<V> IndirectStorage<V> for MappingGuard<V>
where
    V: StorageStorable,
    V::Value: SolValue + core::convert::From<<<V::Value as SolValue>::SolType as SolType>::RustType>,
{
    /// Writes the input value to storage (`SSTORE`) at the location specified by this guard.
    fn write(&mut self, value: V::Value) {
        V::__write(self.storage_key, value);
    }

    /// Reads the value from storage (`SLOAD`) at the location specified by this guard.
    fn read(&self) -> V::Value {
        V::__read(self.storage_key)
    }
}

/// Index implementation for simple mappings.
impl<K, V> Index<K> for Mapping<K, V>
where
    K: SolValue + 'static,
    V: StorageStorable + 'static,
    V::Value: SolValue + core::convert::From<<<V::Value as SolValue>::SolType as SolType>::RustType> + 'static,
{
    type Output = MappingGuard<V>;

    fn index(&self, key: K) -> &Self::Output {
        let storage_key = self.encode_key(key);

        // Create the guard
        let guard = MappingGuard::<V>::new(storage_key);

        // Manually handle memory using the global allocator
        unsafe {
            // Calculate layout for the guard which holds the mapping key
            let layout = Layout::new::<MappingGuard<V>>();

            // Allocate using the `GLOBAL` fixed memory allocator
            let ptr = GLOBAL.alloc(layout) as *mut MappingGuard<V>;

            // Write the guard to the allocated memory
            ptr.write(guard);

            // Return a reference with 'static lifetime (`GLOBAL` never deallocates)
            &*ptr
        }
    }
}

/// Index implementation for simple mappings.
impl<K, V> IndexMut<K> for Mapping<K, V>
where
    K: SolValue + 'static,
    V: StorageStorable + 'static,
    V::Value: SolValue + core::convert::From<<<V::Value as SolValue>::SolType as SolType>::RustType> + 'static,
{
    fn index_mut(&mut self, key: K) -> &mut Self::Output {
        let storage_key = self.encode_key(key);

        // Create the guard
        let guard = MappingGuard::<V>::new(storage_key);

        // Manually handle memory using the global allocator
        unsafe {
            // Calculate layout for the guard which holds the mapping key
            let layout = Layout::new::<MappingGuard<V>>();

            // Allocate using the `GLOBAL` fixed memory allocator
            let ptr = GLOBAL.alloc(layout) as *mut MappingGuard<V>;

            // Write the guard to the allocated memory
            ptr.write(guard);

            // Return a reference with 'static lifetime (`GLOBAL` never deallocates)
            &mut *ptr
        }
    }
}

/// Helper struct to deal with nested mappings.
pub struct NestedMapping<K2, V> {
    mapping: Mapping<K2, V>,
}

impl<K2, V> Deref for NestedMapping<K2, V> {
    type Target = Mapping<K2, V>;

    fn deref(&self) -> &Self::Target {
        &self.mapping
    }
}

impl<K2, V> DerefMut for NestedMapping<K2, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.mapping
    }
}

/// Index implementation for nested mappings.
impl<K1, K2, V> Index<K1> for Mapping<K1, Mapping<K2, V>>
where
    K1: SolValue + 'static,
    K2: SolValue + 'static,
    V: 'static,
{
    type Output = NestedMapping<K2, V>;

    fn index(&self, key: K1) -> &Self::Output {
        let id = self.encode_key(key);

        // Create the nested mapping
        let mapping = Mapping { id, _pd: PhantomData };
        let nested = NestedMapping { mapping };

        // Manually handle memory using the global allocator
        unsafe {
            // Calculate layout for the nested mapping
            // which is an intermediate object that links to the inner-most mapping guard
            let layout = Layout::new::<NestedMapping<K2, V>>();

            // Allocate using the `GLOBAL` fixed memory allocator
            let ptr = GLOBAL.alloc(layout) as *mut NestedMapping<K2, V>;

            // Write the nested mapping to the allocated memory
            ptr.write(nested);

            // Return a reference with 'static lifetime (`GLOBAL` never deallocates)
            &*ptr
        }
    }
}

/// Index implementation for nested mappings.
impl<K1, K2, V> IndexMut<K1> for Mapping<K1, Mapping<K2, V>>
where
    K1: SolValue + 'static,
    K2: SolValue + 'static,
    V: 'static,
{
    fn index_mut(&mut self, key: K1) -> &mut Self::Output {
        let id = self.encode_key(key);

        // Create the nested mapping
        let mapping = Mapping { id, _pd: PhantomData };
        let nested = NestedMapping { mapping };

        // Manually handle memory using the global allocator
        unsafe {
            // Calculate layout for the nested mapping
            // which is an intermediate object that links to the inner-most mapping guard
            let layout = Layout::new::<NestedMapping<K2, V>>();

            // Allocate using the `GLOBAL` fixed memory allocator
            let ptr = GLOBAL.alloc(layout) as *mut NestedMapping<K2, V>;

            // Write the nested mapping to the allocated memory
            ptr.write(nested);

            // Return a reference with 'static lifetime (`GLOBAL` never deallocates)
            &mut *ptr
        }
    }
}
