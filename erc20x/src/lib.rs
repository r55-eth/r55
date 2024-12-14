#![no_std]
#![no_main]

use core::default::Default;

use contract_derive::{contract, interface, payable, Event};
use eth_riscv_runtime::types::Mapping;

use alloy_core::primitives::{address, Address, U256};

extern crate alloc;
use alloc::{string::String, vec::Vec};

#[derive(Default)]
pub struct ERC20x;

#[interface]
trait IERC20 {
    fn balance_of(&self, owner: Address) -> u64;
}

#[contract]
impl ERC20x {
    pub fn x_balance_of(&self, owner: Address, target: Address) -> u64 {
        let token = IERC20::new(target);
        match token.balance_of(owner) {
            Some(balance) => balance,
            _ => eth_riscv_runtime::revert(),
        }
    }
}
