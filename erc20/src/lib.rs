#![no_std]
#![no_main]

use core::default::Default;
use contract_derive::contract;
use eth_riscv_runtime::{types::Mapping};
use alloy_core::primitives::{Address, address};
use alloy_sol_types::sol;

sol! {
    #[derive(Debug)]
    event Transfer(address indexed from, address indexed to, uint64 value);
}
#[derive(Default)]
pub struct ERC20 {
    balance: Mapping<Address, u64>,
}

#[contract]
impl ERC20 {
    pub fn balance_of(&self, owner: Address) -> u64 {
        self.balance.read(owner)
    }

    pub fn transfer(&self, from: Address, to: Address, value: u64) -> bool {
        let from_balance = self.balance.read(from);
        let to_balance = self.balance.read(to);

        if from == to || from_balance < value {
            revert();
        }

        self.balance.write(from, from_balance - value);
        self.balance.write(to, to_balance + value);

        emit(Transfer {
            from,
            to,
            value,
        });
        
        true
    }

    pub fn mint(&self, to: Address, value: u64) -> bool  {
        let owner = msg_sender();
        if owner != address!("0000000000000000000000000000000000000007") {
            revert();
        }

        let to_balance = self.balance.read(to);
        self.balance.write(to, to_balance + value);
        true
    }
}
