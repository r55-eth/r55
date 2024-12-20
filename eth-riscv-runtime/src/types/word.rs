use core::default::Default;
use core::marker::PhantomData;

use super::*;

/// Implements `StorageStorable` EVM words
#[derive(Default)]
pub struct Word<V> {
    slot: u64,
    value: PhantomData<V>,
}

impl<V: StorageStorable> Word<V> {
    pub fn read(&self) -> V {
        V::read(self.slot)
    }

    pub fn write(&mut self, value: V) {
        value.write(self.slot)
    }
}
