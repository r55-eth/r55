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
    // Deploys a new ERC20 token instance
    pub fn x_deploy(&mut self, owner: Address) -> (Address, Address) {
        let token = ERC20::deploy(owner).with_ctx(self);            // IERC20<ReadWrite>
        let owner = token.owner().expect("Unable to get owner");

        (token.address(), owner)
    }

    // Performs a staticcall to an ERC20
    pub fn x_balance_of(&self, owner: Address, token_addr: Address) -> Option<U256> {
        let token = IERC20::new(token_addr).with_ctx(self);         // IERC20<ReadOnly>
        token.balance_of(owner)
    }

    // Performs a (mutable) call to an ERC20
    pub fn x_mint(&mut self, to: Address, amount: U256, token_addr: Address) -> Result<bool, ERC20Error> {
        let mut token = IERC20::new(token_addr).with_ctx(self);     // IERC20<ReadWrite>
        token.mint(to, amount)
    }

    // Fails to perform a (mutable) call to an ERC20, due to the lack of mutability in the ERC20x method
    // pub fn x_mint_fails(&self, to: Address, token_addr: Address) -> Result<bool, ERC20Error> {
    //     let mut token = IERC20::new(token_addr).with_ctx(self);  // IERC20<ReadOnly>
    //     token.mint(to, amount)
    // }

    // If the call fails, it re-attemps another call based on the error type. 
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

    // Always reverts with a str msg
    pub fn panics(&self) { panic!("This function always panics"); }

    // If the call fails, reverts with a str msg
    pub fn x_mint_panics(&mut self, to: Address, amount: U256, token_addr: Address) -> bool {
        let mut token = IERC20::new(token_addr).with_ctx(self);     // IERC20<ReadWrite>
        token.mint(to, amount).expect("ERC20::mint() failed!")
    }
}
