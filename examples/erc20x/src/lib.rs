#![no_std]
#![no_main]

use core::default::Default;

use alloy_core::primitives::{Address, U256, Bytes};
use contract_derive::{contract, show_streams};

extern crate alloc;

use erc20::{ERC20Error, IERC20};

mod deployable;
use deployable::ERC20;

#[derive(Default, )]
pub struct ERC20x;

#[contract]
impl ERC20x {
    pub fn x_bytecode(&mut self) -> Bytes {
        ERC20::bytecode()
    }

    pub fn x_deploy(&mut self, owner: Address) -> (Address, Address) {
        let token = ERC20::deploy(owner);
        let owner = IERC20::new(token).with_ctx(self).owner().expect("Unable to get owner");

        (token, owner)
    }

    pub fn x_balance_of(&self, owner: Address, token_addr: Address) -> Option<U256> {
        let token = IERC20::new(token_addr).with_ctx(self);         // IERC20<ReadOnly>
        token.balance_of(owner)
    }

    pub fn x_mint(&mut self, to: Address, amount: U256, token_addr: Address) -> Result<bool, ERC20Error> {
        let mut token = IERC20::new(token_addr).with_ctx(self);     // IERC20<ReadWrite>
        token.mint(to, amount)
    }

    // pub fn x_mint_fails(&self, to: Address, token_addr: Address) -> Result<bool, ERC20Error> {
    //     let mut token = IERC20::new(token_addr).with_ctx(self);  // IERC20<ReadOnly>
    //     token.mint(to, amount)
    // }

    pub fn x_transfer_from(
        &mut self,
        from: Address,
        amount: U256,
        token_addr: Address
    ) -> Result<bool, ERC20Error> {
        let mut token = IERC20::new(token_addr).with_ctx(self);     // IERC20<ReadWrite>
        let to = msg_sender();

        // easily leverage rust's `Result<T, E>` enum to deal with call reverts
        match token.transfer_from(from, to, amount) {
            Err(ERC20Error::InsufficientBalance(max)) => token.transfer_from(from, to, max),
            Err(ERC20Error::InsufficientAllowance(max)) => token.transfer_from(from, to, max),
            other => other
        }
    }

    pub fn panics(&self) { panic!("This function always panics"); }

    pub fn x_mint_panics(&mut self, to: Address, amount: U256, token_addr: Address) -> bool {
        let mut token = IERC20::new(token_addr).with_ctx(self);     // IERC20<ReadWrite>
        token.mint(to, amount).expect("ERC20::mint() failed!")
    }
}
