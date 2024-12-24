#![no_std]
#![no_main]

use core::default::Default;

use contract_derive::{contract, payable, storage, Event};
use eth_riscv_runtime::types::{Mapping, Slot, StorageLayout};

use alloy_core::primitives::{address, Address, U256};

extern crate alloc;
use alloc::string::String;

#[storage]
pub struct ERC20 {
    total_supply: Slot<U256>,
    balances: Mapping<Address, U256>,
    allowances: Mapping<Address, Mapping<Address, U256>>,
    // name: String,
    // symbol: String,
    // decimals: u8,
}

#[derive(Event)]
pub struct Transfer {
    #[indexed]
    pub from: Address,
    #[indexed]
    pub to: Address,
    pub value: U256,
}

#[derive(Event)]
pub struct Mint {
    #[indexed]
    pub from: Address,
    #[indexed]
    pub to: Address,
    pub value: U256,
}

#[contract]
impl ERC20 {
    // -- STATE MODIFYING FUNCTIONS -------------------------------------------
    #[payable]
    pub fn mint(&mut self, to: Address, value: U256) -> bool {
        // TODO: implement constructors and store contract owner
        let _owner = msg_sender();

        // increase user balance
        let to_balance = self.balances.read(to);
        self.balances.write(to, to_balance + value);
        log::emit(Transfer::new(
            address!("0000000000000000000000000000000000000000"),
            to,
            value,
        ));

        // increase total supply
        self.total_supply += value;

        true
    }

    pub fn approve(&mut self, spender: Address, value: U256) -> bool {
        let mut spender_allowances = self.allowances.read(msg_sender());
        spender_allowances.write(spender, value);
        true
    }

    pub fn transfer(&mut self, to: Address, value: U256) -> bool {
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

    pub fn transfer_from(&mut self, sender: Address, recipient: Address, amount: U256) -> bool {
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

    // -- GETTER FUNCTIONS ----------------------------------------------------

    pub fn total_supply(&self) -> U256 {
        self.total_supply.read()
    }

    pub fn balance_of(&self, owner: Address) -> U256 {
        self.balances.read(owner)
    }

    pub fn allowance(&self, owner: Address, spender: Address) -> U256 {
        self.allowances.read(owner).read(spender)
    }
}
