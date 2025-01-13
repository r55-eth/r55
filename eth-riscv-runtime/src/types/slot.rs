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

impl<V: StorageStorable> Slot<V> {
    pub fn read(&self) -> V {
        V::read(self.id)
    }

    pub fn write(&mut self, mut value: V) {
        value.write(self.id);
    }
}

// Implementation of several std traits to improve dev-ex
impl<V> Add<V> for Slot<V>
where
    V: StorageStorable + core::ops::Add<Output = V>,
{
    type Output = V;
    fn add(self, rhs: V) -> V {
        self.read() + rhs
    }
}

impl<V> AddAssign<V> for Slot<V>
where
    V: StorageStorable + core::ops::Add<Output = V>,
{
    fn add_assign(&mut self, rhs: V) {
        self.write(self.read() + rhs)
    }
}

impl<V> Sub<V> for Slot<V>
where
    V: StorageStorable + core::ops::Sub<Output = V>,
{
    type Output = V;
    fn sub(self, rhs: V) -> V {
        self.read() - rhs
    }
}

impl<V> SubAssign<V> for Slot<V>
where
    V: StorageStorable + core::ops::Sub<Output = V>,
{
    fn sub_assign(&mut self, rhs: V) {
        self.write(self.read() - rhs)
    }
}

impl<V> PartialEq for Slot<V>
where
    V: StorageStorable + PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.read() == other.read()
    }
}
