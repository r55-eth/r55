#![no_std]
#![no_main]

use core::default::Default;

use alloy_core::primitives::{address, Bytes, Address, U256};
use contract_derive::{contract, interface};

extern crate alloc;
use alloc::{string::String, vec::Vec};

#[derive(Default)]
pub struct EVMCaller;

#[interface("camelCase")]
trait ISimpleStorage {
    fn get(&self) -> U256;
    fn set(&mut self, value: U256);
}

#[contract]
impl EVMCaller {
    pub fn x_set(&mut self, target: Address, value: U256) {
        ISimpleStorage::new(target).with_ctx(self).set(value);
    }

    pub fn x_get(&self, target: Address) -> U256 {
        match ISimpleStorage::new(target).with_ctx(self).get() {
            Some(value) => value,
            // easily add fallback logic if desired
            _ => eth_riscv_runtime::revert(),
        }
    }
}
