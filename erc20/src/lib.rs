#![no_std]
#![no_main]

use core::default::Default;

use contract_derive::{contract, payable, Event};
use eth_riscv_runtime::types::Mapping;

use alloy_core::primitives::{Address, address, U256};

extern crate alloc;
use alloc::string::String;

#[derive(Default)]
pub struct ERC20 {
    balances: Mapping<Address, u64>,
    allowances: Mapping<Address, Mapping<Address, u64>>,
    total_supply: U256,
    name: String,
    symbol: String,
    decimals: u8,
}

#[derive(Event)]
pub struct Transfer {
    #[indexed]
    pub from: Address,
    #[indexed]
    pub to: Address,
    pub value: u64
}

#[contract]
impl ERC20 {
    pub fn balance_of(&self, owner: Address) -> u64 {
        self.balances.read(owner)
    }

    pub fn transfer(&self, to: Address, value: u64) -> bool {
        let from = msg_sender();
        let from_balance = self.balances.read(from);
        let to_balance = self.balances.read(to);

        if from == to || from_balance < value {
            revert();
        }

        self.balances.write(from, from_balance - value);
        self.balances.write(to, to_balance + value);

        log::emit(Transfer::new(from, to, value));
        true
    }

    pub fn approve(&self, spender: Address, value: u64) -> bool {
        let spender_allowances = self.allowances.read(msg_sender());
        spender_allowances.write(spender, value);
        true
    }

    pub fn transfer_from(&self, sender: Address, recipient: Address, amount: u64) -> bool {
        let allowance = self.allowances.read(sender).read(msg_sender());
        let sender_balance = self.balances.read(sender);
        let recipient_balance = self.balances.read(recipient);

        self.allowances.read(sender).write(msg_sender(), allowance - amount);
        self.balances.write(sender, sender_balance - amount);
        self.balances.write(recipient, recipient_balance + amount);

        true
    }

    pub fn total_supply(&self) -> U256 {
        self.total_supply
    }

    pub fn allowance(&self, owner: Address, spender: Address) -> u64 {
        self.allowances.read(owner).read(spender)
    }

    #[payable]
    pub fn mint(&self, to: Address, value: u64) -> bool {
        let owner = msg_sender();
        if owner != address!("0000000000000000000000000000000000000001") {
            revert();
        }

        let to_balance = self.balances.read(to);
        self.balances.write(to, to_balance + value);

        true
    }
}
