#![no_std]
#![no_main]

use core::default::Default;

use contract_derive::{contract, payable, Event};
use eth_riscv_runtime::types::Mapping;

use alloy_core::primitives::{Address, address, U256};
extern crate alloc;

#[derive(Default)]
pub struct ERC20 {
    balance: Mapping<Address, u64>,
    paused: bool,
    metadata: [u8; 32],
}

#[derive(Event)]
pub struct Transfer {
    #[indexed]
    pub from: Address,
    #[indexed]
    pub to: Address,
    pub value: u64
}

#[derive(Event)]
pub struct Mint {
    #[indexed]
    pub to: Address,
    pub value: u64
}

#[derive(Event)]
pub struct Burn {
    #[indexed]
    pub from: Address,
    pub value: u64
}

#[derive(Event)]
pub struct PauseChanged {
    pub paused: bool
}

#[derive(Event)]
pub struct MetadataUpdated {
    pub data: [u8; 32]
}

#[contract]
impl ERC20 {
    pub fn balance_of(&self, owner: Address) -> u64 {
        self.balance.read(owner)
    }

    pub fn transfer(&self, to: Address, value: u64) -> bool {
        let from = msg_sender();
        let from_balance = self.balance.read(from);
        let to_balance = self.balance.read(to);

        if from == to || from_balance < value {
            revert();
        }

        self.balance.write(from, from_balance - value);
        self.balance.write(to, to_balance + value);

        emit!(Transfer, from, to, value);
        true
    }

    #[payable]
    pub fn mint(&self, to: Address, value: u64) -> bool {
        let owner = msg_sender();
        if owner != address!("0000000000000000000000000000000000000007") {
            revert();
        }

        let to_balance = self.balance.read(to);
        self.balance.write(to, to_balance + value);

        emit!(Mint, to, value);
        true
    }

    pub fn burn(&self, from: Address, value: u64) -> bool {
        let from_balance = self.balance.read(from);
        if from_balance < value {
            revert();
        }
        
        self.balance.write(from, from_balance - value);

        emit!(Burn, from, value);
        true
    }

    pub fn set_paused(&self, paused: bool) -> bool {

        emit!(PauseChanged, paused);
        true
    }

    pub fn update_metadata(&self, data: [u8; 32]) -> bool {

        emit!(MetadataUpdated, data);
        true
    }
}