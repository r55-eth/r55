use super::*;

use core::ops::{Add, AddAssign, Sub, SubAssign};

/// Wrapper around `alloy::primitives` that can be written in a single slot (single EVM word).
#[derive(Default)]
pub struct Slot<V> {
    id: U256,
    _pd: PhantomData<V>,
}

impl<V> StorageLayout for Slot<V> {
    fn allocate(first: u64, second: u64, third: u64, fourth: u64) -> Self {
        Self {
            id: U256::from_limbs([first, second, third, fourth]),
            _pd: PhantomData::default(),
        }
    }
}

impl<V> StorageStorable for Slot<V>
where
    V: SolValue + core::convert::From<<<V as SolValue>::SolType as SolType>::RustType>,
{
    type Value = V;

    fn __read(key: U256) -> Self::Value {
        let bytes: [u8; 32] = sload(key).to_be_bytes();
        V::abi_decode(&bytes, false).unwrap_or_else(|_| revert())
    }

    fn __write(key: U256, value: Self::Value) {
        let bytes = value.abi_encode();
        let mut padded = [0u8; 32];
        padded[..bytes.len()].copy_from_slice(&bytes);
        sstore(key, U256::from_be_bytes(padded));
    }
}

impl<V> DirectStorage<V> for Slot<V>
where
    Self: StorageStorable<Value = V>,
{
    fn read(&self) -> V {
        Self::__read(self.id)
    }

    fn write(&mut self, value: V) {
        Self::__write(self.id, value)
    }
}

// Implementation of several std traits to improve dev-ex
impl<V> Add<V> for Slot<V>
where
    Self: StorageStorable<Value = V>,
    V: core::ops::Add<Output = V>,
{
    type Output = V;
    fn add(self, rhs: V) -> V {
        self.read() + rhs
    }
}

impl<V> AddAssign<V> for Slot<V>
where
    Self: StorageStorable<Value = V>,
    V: core::ops::Add<Output = V>,
{
    fn add_assign(&mut self, rhs: V) {
        self.write(self.read() + rhs)
    }
}

impl<V> Sub<V> for Slot<V>
where
    Self: StorageStorable<Value = V>,
    V: core::ops::Sub<Output = V>,
{
    type Output = V;
    fn sub(self, rhs: V) -> V {
        self.read() - rhs
    }
}

impl<V> SubAssign<V> for Slot<V>
where
    Self: StorageStorable<Value = V>,
    V: core::ops::Sub<Output = V>,
{
    fn sub_assign(&mut self, rhs: V) {
        self.write(self.read() - rhs)
    }
}

impl<V> PartialEq for Slot<V>
where
    Self: StorageStorable<Value = V>,
    V: StorageStorable + PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.read() == other.read()
    }
}
