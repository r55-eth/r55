#![no_std]
#![no_main]

use core::default::Default;

use alloy_core::primitives::{Address, U256};
use contract_derive::contract;

extern crate alloc;

use erc20::IERC20;

#[derive(Default, )]
pub struct ERC20x;

#[contract]
impl ERC20x {
    pub fn x_balance_of(&self, owner: Address, target: Address) -> U256 {
        let token = IERC20::new(target).with_ctx(self);         // IERC20<ReadOnly>
        match token.balance_of(owner) {
            Some(balance) => balance,
            _ => eth_riscv_runtime::revert(),
        }
    }

    pub fn x_mint(&mut self, owner: Address, value: U256, target: Address) -> bool {
        let mut token = IERC20::new(target).with_ctx(self);     // IERC20<ReadWrite>
        match token.mint(owner, U256::from(value)) {
            Some(success) => success,
            _ => eth_riscv_runtime::revert()
        }
    }

    // pub fn x_mint_fails(&self, owner: Address, target: Address) -> bool {
    //     let mut token = IERC20::new(target).with_ctx(self);  // IERC20<ReadOnly>
    //     match token.mint(owner, U256::from(value)) {
    //         Some(success) => success,
    //         _ => eth_riscv_runtime::revert()
    //     }
    // }
}
