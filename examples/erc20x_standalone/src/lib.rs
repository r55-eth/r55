#![no_std]
#![no_main]

use core::default::Default;

use alloy_core::primitives::{address, Address, U256};
use contract_derive::{contract, interface};

extern crate alloc;
use alloc::{string::String, vec::Vec};

#[derive(Default)]
pub struct ERC20x;

#[interface]
trait IERC20 {
    fn balance_of(&self, owner: Address) -> U256;
}

#[contract]
impl ERC20x {
    pub fn x_balance_of(&self, owner: Address, target: Address) -> U256 {
        let token = IERC20::new(target);
        match token.balance_of(owner) {
            Some(balance) => balance,
            _ => eth_riscv_runtime::revert(),
        }
    }
}
