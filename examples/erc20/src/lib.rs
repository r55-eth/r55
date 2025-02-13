#![no_std]
#![no_main]

use core::default::Default;

use contract_derive::{contract, payable, show_streams, storage, Event, CustomError};
use eth_riscv_runtime::types::*;

use alloy_core::primitives::{address, Address, U256, Bytes};

extern crate alloc;
use alloc::string::String;

#[storage]
pub struct ERC20 {
    total_supply: Slot<U256>,
    balances: Mapping<Address, Slot<U256>>,
    allowances: Mapping<Address, Mapping<Address, Slot<U256>>>,
    owner: Slot<Address>,
    // name: String,
    // symbol: String,
    // decimals: u8,
}

#[derive(CustomError)]
#[show_streams]
pub enum ERC20Error {
    OnlyOwner,
    ZeroAmount,
    InsufficientBalance(U256),
    InsufficientAllowance(U256)
}

#[derive(Event)]
pub struct Transfer {
    #[indexed]
    pub from: Address,
    #[indexed]
    pub to: Address,
    pub amount: U256,
}

#[derive(Event)]
pub struct Mint {
    #[indexed]
    pub from: Address,
    #[indexed]
    pub to: Address,
    pub amount: U256,
}

#[contract]
impl ERC20 {
    // -- CONSTRUCTOR ---------------------------------------------------------
    pub fn new(owner: Address) -> Self {
        // init the contract
        let mut erc20 = ERC20::default();

        // store the owner
        erc20.owner.write(owner);

        // return the initialized contract
        erc20
    }

    // -- STATE MODIFYING FUNCTIONS -------------------------------------------
    #[payable]
    pub fn mint(&mut self, to: Address, amount: U256) -> Result<bool, ERC20Error> {
        if msg_sender() != self.owner.read() { return Err(ERC20Error::OnlyOwner) }; 
        if amount == U256::ZERO { return Err(ERC20Error::ZeroAmount) };

        // increase user balance
        let to_balance = self.balances.read(to);
        self.balances.write(to, to_balance + amount);
        log::emit(Transfer::new(
            address!("0000000000000000000000000000000000000000"),
            to,
            amount,
        ));

        // increase total supply
        self.total_supply += amount;
        
        Ok(true)
    }

    pub fn approve(&mut self, spender: Address, amount: U256) -> bool {
        let mut spender_allowances = self.allowances.read(msg_sender());
        spender_allowances.write(spender, amount);
        true
    }

    pub fn transfer(&mut self, to: Address, amount: U256) -> Result<bool, ERC20Error> {
        if amount == U256::ZERO { return Err(ERC20Error::ZeroAmount) };

        let from = msg_sender();
        let from_balance = self.balances.read(from);
        let to_balance = self.balances.read(to);

        if from_balance < amount { return Err(ERC20Error::InsufficientBalance(from_balance)) }

        self.balances.write(from, from_balance - amount);
        self.balances.write(to, to_balance + amount);

        log::emit(Transfer::new(from, to, amount));
        Ok(true)
    }

    pub fn transfer_from(&mut self, sender: Address, recipient: Address, amount: U256) -> Result<bool, ERC20Error> {
        if amount == U256::ZERO { return Err(ERC20Error::ZeroAmount) };

        let allowance = self.allowances.read(sender).read(msg_sender());
        if allowance < amount { return Err(ERC20Error::InsufficientAllowance(allowance)) };

        let sender_balance = self.balances.read(sender);
        if allowance < amount { return Err(ERC20Error::InsufficientBalance(sender_balance)) };

        self.allowances
            .read(sender)
            .write(msg_sender(), allowance - amount);
        self.balances.write(sender, sender_balance - amount);
        self.balances.write(recipient, self.balances.read(recipient) + amount);

        Ok(true)
    }

    pub fn transfer_ownership(&mut self, new_owner: Address) -> Result<bool, ERC20Error> {
       if msg_sender() != self.owner.read() { return Err(ERC20Error::OnlyOwner) }; 
        self.owner.write(new_owner);

        Ok(true)
    }

    // -- READ-ONLY FUNCTIONS --------------------------------------------------
    fn check_only_owner(&self) -> Result<(), ERC20Error> {
       if msg_sender() != self.owner.read() { Err(ERC20Error::OnlyOwner) } else { Ok(()) } 
    }

    pub fn owner(&self) -> Address {
        self.owner.read()
    }

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
