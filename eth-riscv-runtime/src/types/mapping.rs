use core::{
    alloc::{GlobalAlloc, Layout},
    marker::PhantomData,
    ops::{Deref, Index}, 
};

use crate::alloc::GLOBAL;

use super::*;

/// Implements a Solidity-like Mapping type.
#[derive(Default, Clone)]
pub struct Mapping<K, S> {
    id: U256,
    _pd: PhantomData<(K, S)>,
}

impl<K, S> StorageLayout for Mapping<K, S> {
    fn allocate(first: u64, second: u64, third: u64, fourth: u64) -> Self {
        Self {
            id: U256::from_limbs([first, second, third, fourth]),
            _pd: PhantomData::default(),
        }
    }
}

impl<K, S> Mapping<K, S>
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
/// to read from and write to the underlying storage location.
pub struct MappingGuard<S>
where
    S: StorageStorable,
    S::Value: SolValue + core::convert::From<<<S::Value as SolValue>::SolType as SolType>::RustType> + Clone,
{
    storage_key: U256,
    _phantom: PhantomData<S>,
}

impl<S> MappingGuard<S>
where
    S: StorageStorable,
    S::Value: SolValue + core::convert::From<<<S::Value as SolValue>::SolType as SolType>::RustType> + Clone,
{
    pub fn new(storage_key: U256) -> Self {
        Self {
            storage_key,
            _phantom: PhantomData,
        }
    }

    /// Writes the input value to storage (`SSTORE`) at the location specified by this guard.
    pub fn write(&self, value: S::Value) {
        S::__write(self.storage_key, value);
    }

    /// Reads the value from storage (`SLOAD`) at the location specified by this guard.
    pub fn read(&self) -> S::Value {
        S::__read(self.storage_key)
    }
}

/// Index implementation for direct value mappings.
impl<K, S> Index<K> for Mapping<K, S>
where
    K: SolValue + 'static,
    S: StorageStorable + 'static,
    S::Value: SolValue + core::convert::From<<<S::Value as SolValue>::SolType as SolType>::RustType> + Clone + 'static,
{
    type Output = MappingGuard<S>;

    fn index(&self, key: K) -> &Self::Output {
        let storage_key = self.encode_key(key);

        // Create the guard
        let guard = MappingGuard::<S>::new(storage_key);

        // Manually handle memory using the global allocator
        unsafe {
            // Calculate layout for the guard which holds the mapping key
            let layout = Layout::new::<MappingGuard<S>>();

            // Allocate using the `GLOBAL` fixed memory allocator
            let ptr = GLOBAL.alloc(layout) as *mut MappingGuard<S>;

            // Write the guard to the allocated memory
            ptr.write(guard);

            // Return a reference with 'static lifetime (`GLOBAL` never deallocates)
            &*ptr
        }
    }
}

/// Helper struct to deal with nested mappings.
pub struct NestedMapping<K2, S> {
    mapping: Mapping<K2, S>,
}

impl<K2, S> Deref for NestedMapping<K2, S> {
    type Target = Mapping<K2, S>;

    fn deref(&self) -> &Self::Target {
        &self.mapping
    }
}

/// Index implementation for nested mappings.
impl<K1, K2, S> Index<K1> for Mapping<K1, Mapping<K2, S>>
where
    K1: SolValue + 'static,
    K2: SolValue + 'static,
    S: 'static,
{
    type Output = NestedMapping<K2, S>;

    fn index(&self, key: K1) -> &Self::Output {
        let id = self.encode_key(key);

        // Create the nested mapping
        let mapping = Mapping { id, _pd: PhantomData };
        let nested = NestedMapping { mapping };

        // Manually handle memory using the global allocator
        unsafe {
            // Calculate layout for the nested mapping
            // which is an intermediate object that links to the inner-most mapping guard
            let layout = Layout::new::<NestedMapping<K2, S>>();

            // Allocate using the `GLOBAL` fixed memory allocator
            let ptr = GLOBAL.alloc(layout) as *mut NestedMapping<K2, S>;

            // Write the nested mapping to the allocated memory
            ptr.write(nested);

            // Return a reference with 'static lifetime (`GLOBAL` never deallocates)
            &*ptr
        }
    }
}
