#![no_std]
#![no_main]

use core::default::Default;

use contract_derive::{contract, payable, Event};
use eth_riscv_runtime::types::{Mapping, Word};

use alloy_core::primitives::{address, Address, U256};

extern crate alloc;
use alloc::string::String;

#[derive(Default)]
pub struct ERC20 {
    balances: Mapping<Address, u64>,
    allowances: Mapping<Address, Mapping<Address, u64>>,
    total_supply: Word<U256>,
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
    pub value: u64,
}

#[derive(Event)]
pub struct Mint {
    #[indexed]
    pub from: Address,
    #[indexed]
    pub to: Address,
    pub value: u64,
}

#[contract]
impl ERC20 {
    pub fn balance_of(&self, owner: Address) -> u64 {
        self.balances.read(owner)
    }

    pub fn transfer(&mut self, to: Address, value: u64) -> bool {
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

    pub fn approve(&mut self, spender: Address, value: u64) -> bool {
        let mut spender_allowances = self.allowances.read(msg_sender());
        spender_allowances.write(spender, value);
        true
    }

    pub fn transfer_from(&mut self, sender: Address, recipient: Address, amount: u64) -> bool {
        let allowance = self.allowances.read(sender).read(msg_sender());
        let sender_balance = self.balances.read(sender);
        let recipient_balance = self.balances.read(recipient);

        self.allowances
            .read(sender)
            .write(msg_sender(), allowance - amount);
        self.balances.write(sender, sender_balance - amount);
        self.balances.write(recipient, recipient_balance + amount);

        true
    }

    pub fn total_supply(&self) -> U256 {
        self.total_supply.read()
    }

    pub fn allowance(&self, owner: Address, spender: Address) -> u64 {
        self.allowances.read(owner).read(spender)
    }

    #[payable]
    pub fn mint(&mut self, to: Address, value: u64) -> bool {
        let owner = msg_sender();

        let to_balance = self.balances.read(to);
        self.balances.write(to, to_balance + value);

        let prev_supply = self.total_supply.read();
        self.total_supply.write(prev_supply + U256::from(value));

        log::emit(Transfer::new(
            address!("0000000000000000000000000000000000000000"),
            to,
            value,
        ));
        true
    }
}
