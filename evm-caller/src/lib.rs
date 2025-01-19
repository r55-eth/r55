#![no_std]
#![no_main]

use core::default::Default;

use alloy_core::primitives::{address, Address, U256};
use contract_derive::{contract, interface};

extern crate alloc;
use alloc::{string::String, vec::Vec};

#[derive(Default)]
pub struct EVMCaller;

#[interface(target = "evm", rename = "camelCase")]
trait ISimpleStorage {
    fn get(&self) -> U256;
    fn set(&mut self, value: U256);
    fn get_with_caller_address(&self) -> (U256, Address);
}

#[contract]
impl EVMCaller {
    pub fn x_set(&self, target: Address, value: U256) {
        ISimpleStorage::new(target).set(value);
    }

    pub fn x_get(&self, target: Address) -> U256 {
        match ISimpleStorage::new(target).get() {
            Some(value) => value,
            // easily add fallback logic if desired
            _ => eth_riscv_runtime::revert(),
        }
    }

    pub fn x_get_with_caller(&self, target: Address) -> (U256, Address) {
        match ISimpleStorage::new(target).get_with_caller_address() {
            Some((value, addr)) => (value, addr),
            // easily add fallback logic if desired
            _ => eth_riscv_runtime::revert(),
        }
    }
}
