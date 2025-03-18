use alloc::boxed::Box;
use core::{
    alloc::{GlobalAlloc, Layout},
    marker::PhantomData,
    ops::{Deref, DerefMut, Index},
};

use crate::alloc::GLOBAL;

use super::*;

/// Implements a Solidity-like Mapping type.
#[derive(Default, Clone)]
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

// Read guard - provides immutable access to a value
pub struct MappingReadGuard<'a, V> {
    value: Box<V>,
    _phantom: PhantomData<&'a V>,
}

impl<'a, V> Deref for MappingReadGuard<'a, V> {
    type Target = V;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

// Write guard - provides mutable access and writes back on drop
pub struct MappingWriteGuard<'a, T, V>
where
    T: StorageStorable<Value = V>,
    V: SolValue + core::convert::From<<<V as SolValue>::SolType as SolType>::RustType> + Clone,
{
    value: Box<V>,
    storage_key: U256,
    dirty: bool,
    _phantom: PhantomData<&'a mut T>,
}

impl<'a, T, V> Deref for MappingWriteGuard<'a, T, V>
where
    T: StorageStorable<Value = V>,
    V: SolValue + core::convert::From<<<V as SolValue>::SolType as SolType>::RustType> + Clone,
{
    type Target = V;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<'a, T, V> DerefMut for MappingWriteGuard<'a, T, V>
where
    T: StorageStorable<Value = V>,
    V: SolValue + core::convert::From<<<V as SolValue>::SolType as SolType>::RustType> + Clone,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.dirty = true;
        &mut self.value
    }
}

impl<'a, T, V> Drop for MappingWriteGuard<'a, T, V>
where
    T: StorageStorable<Value = V>,
    V: SolValue + core::convert::From<<<V as SolValue>::SolType as SolType>::RustType> + Clone,
{
    fn drop(&mut self) {
        if self.dirty {
            // Only write to storage if the value was modified
            T::__write(self.storage_key, *self.value.clone());
        }
    }
}

// Accessor struct that provides read() and write() methods
pub struct MappingProxy<T, V> {
    storage_key: U256,
    _pd: PhantomData<(T, V)>,
}

impl<T, V> MappingProxy<T, V>
where
    T: StorageStorable<Value = V>,
    V: SolValue + core::convert::From<<<V as SolValue>::SolType as SolType>::RustType> + Clone,
{
    pub fn read(&self) -> MappingReadGuard<V> {
        let value = T::__read(self.storage_key);
        MappingReadGuard {
            value: Box::new(value),
            _phantom: PhantomData,
        }
    }

    pub fn write(&self) -> MappingWriteGuard<T, V> {
        let value = T::__read(self.storage_key);
        MappingWriteGuard {
            value: Box::new(value),
            storage_key: self.storage_key,
            dirty: false,
            _phantom: PhantomData,
        }
    }
}

// Implementation for direct value mappings (e.g., Mapping<Address, Slot<U256>>)
impl<K, T, V> Index<K> for Mapping<K, T>
where
    K: SolValue + 'static,
    T: StorageStorable<Value = V>,
    V: SolValue
        + core::convert::From<<<V as SolValue>::SolType as SolType>::RustType>
        + Clone
        + 'static,
{
    type Output = MappingProxy<T, V>;

    fn index(&self, key: K) -> &Self::Output {
        let storage_key = self.encode_key(key);

        // Create the accessor
        let accessor = MappingProxy {
            storage_key,
            _pd: PhantomData,
        };

        // Allocate memory directly using the global allocator
        unsafe {
            // Calculate layout for the accessor
            let layout = Layout::new::<MappingProxy<T, V>>();

            // Allocate memory using GLOBAL
            let ptr = GLOBAL.alloc(layout) as *mut MappingProxy<T, V>;

            // Write the accessor to the allocated memory
            ptr.write(accessor);

            // Return a reference with 'static lifetime
            // This is safe because our allocator never deallocates
            &*ptr
        }
    }
}

// Nested mapping accessor
pub struct NestedMappingProxy<K2, V> {
    mapping: Mapping<K2, V>,
}

impl<K2, V> Deref for NestedMappingProxy<K2, V> {
    type Target = Mapping<K2, V>;

    fn deref(&self) -> &Self::Target {
        &self.mapping
    }
}

// Implementation for nested mappings (e.g., Mapping<Address, Mapping<Address, Slot<U256>>>)
impl<K1, K2, V> Index<K1> for Mapping<K1, Mapping<K2, V>>
where
    K1: SolValue + 'static,
    K2: SolValue + 'static,
    V: 'static,
{
    type Output = NestedMappingProxy<K2, V>;

    fn index(&self, key: K1) -> &Self::Output {
        let nested_id = self.encode_key(key);

        // Create the nested mapping
        let nested_mapping = Mapping {
            id: nested_id,
            _pd: PhantomData,
        };

        // Create the nested proxy
        let proxy = NestedMappingProxy {
            mapping: nested_mapping,
        };

        // Allocate memory directly using the global allocator
        unsafe {
            // Calculate layout for the nested proxy
            let layout = Layout::new::<NestedMappingProxy<K2, V>>();

            // Allocate memory using GLOBAL
            let ptr = GLOBAL.alloc(layout) as *mut NestedMappingProxy<K2, V>;

            // Write the nested proxy to the allocated memory
            ptr.write(proxy);

            // Return a reference with 'static lifetime
            &*ptr
        }
    }
}
