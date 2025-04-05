#![no_std]
#![no_main]

use core::default::Default;

use contract_derive::{contract, payable, storage, Event, Error};
use eth_riscv_runtime::types::*;

use alloy_core::primitives::{Address, U256};

extern crate alloc;

// -- EVENTS -------------------------------------------------------------------
#[derive(Event)]
pub struct Transfer {
    #[indexed]
    pub from: Address,
    #[indexed]
    pub to: Address,
    pub amount: U256,
}

#[derive(Event)]
pub struct Approval {
    #[indexed]
    pub owner: Address,
    #[indexed]
    pub spender: Address,
    pub amount: U256,
}

#[derive(Event)]
pub struct OwnershipTransferred {
    #[indexed]
    pub from: Address,
    #[indexed]
    pub to: Address,
}

// -- ERRORS -------------------------------------------------------------------
#[derive(Error)]
pub enum ERC20Error {
    OnlyOwner,
    InsufficientBalance(U256),
    InsufficientAllowance(U256),
    SelfApproval,
    SelfTransfer,
    ZeroAmount,
    ZeroAddress,
}

// -- CONTRACT -----------------------------------------------------------------
#[storage]
pub struct ERC20 {
    total_supply: Slot<U256>,
    balance_of: Mapping<Address, Slot<U256>>,
    allowance_of: Mapping<Address, Mapping<Address, Slot<U256>>>,
    owner: Slot<Address>,
    // TODO: handle string storage
    // name: String, 
    // symbol: String,
    // decimals: u8,
}

#[contract]
impl ERC20 {
    // -- CONSTRUCTOR ----------------------------------------------------------
    pub fn new(owner: Address) -> Self {
        // Init the contract
        let mut erc20 = ERC20::default();

        // Update state
        erc20.owner.write(owner);

        // Return the initialized contract
        erc20
    }

    // -- STATE MODIFYING FUNCTIONS --------------------------------------------
    #[payable]
    pub fn mint(&mut self, to: Address, amount: U256) -> Result<bool, ERC20Error> {
        // Perform sanity checks
        if msg_sender() != self.owner.read() { return Err(ERC20Error::OnlyOwner) }; 
        if amount == U256::ZERO { return Err(ERC20Error::ZeroAmount) };
        if to == Address::ZERO { return Err(ERC20Error::ZeroAddress) };

        // Increase user balance
        let to_balance = self.balance_of[to].read();
        self.balance_of[to].write(to_balance + amount);

        // Increase total supply
        self.total_supply += amount;
        
        // Emit event + return `true` to stick to (EVM) ERC20 convention
        log::emit(Transfer::new(Address::ZERO, to, amount));
        Ok(true)
    }

    pub fn approve(&mut self, spender: Address, amount: U256) -> Result<bool, ERC20Error> {
        let owner = msg_sender();

        // Perform sanity checks
        if spender == Address::ZERO { return Err(ERC20Error::ZeroAddress) };
        if spender == owner { return Err(ERC20Error::SelfApproval) };

        // Update state
        self.allowance_of[owner][spender].write(amount);

        // Emit event + return 
        log::emit(Approval::new(owner, spender, amount));
        Ok(true)
    }

    pub fn transfer(&mut self, to: Address, amount: U256) -> Result<bool, ERC20Error> {
        let from = msg_sender();

        // Perform sanity checks
        if to == Address::ZERO { return Err(ERC20Error::ZeroAddress) };
        if amount == U256::ZERO { return Err(ERC20Error::ZeroAmount) };
        if from == to { return Err(ERC20Error::SelfTransfer) };

        // Read user balances
        let from_balance = self.balance_of[from].read();
        let to_balance = self.balance_of[to].read();

        // Ensure enough balance
        if from_balance < amount { return Err(ERC20Error::InsufficientBalance(from_balance)) }

        // Update state
        self.balance_of[from].write(from_balance - amount);
        self.balance_of[to].write(to_balance + amount);

        // Emit event + return 
        log::emit(Transfer::new(from, to, amount));
        Ok(true)
    }

    pub fn transfer_from(&mut self, from: Address, to: Address, amount: U256) -> Result<bool, ERC20Error> {
        let msg_sender = msg_sender();

        // Perform sanity checks
        if to == Address::ZERO { return Err(ERC20Error::ZeroAddress) };
        if amount == U256::ZERO { return Err(ERC20Error::ZeroAmount) };
        if from == to { return Err(ERC20Error::SelfTransfer) };

        // Ensure enough allowance
        let allowance = self.allowance_of[from][msg_sender].read();
        if allowance < amount { return Err(ERC20Error::InsufficientAllowance(allowance)) };

        // Ensure enough balance
        let from_balance = self.balance_of[from].read();
        if from_balance < amount { return Err(ERC20Error::InsufficientBalance(from_balance)) };

        // Update state
        self.allowance_of[from][msg_sender].write(allowance - amount);
        self.balance_of[from].write(from_balance - amount);
        
        let to_balance = self.balance_of[to].read();
        self.balance_of[to].write(to_balance + amount);

        // Emit event + return 
        log::emit(Transfer::new(from, to, amount));
        Ok(true)
    }

    pub fn transfer_ownership(&mut self, new_owner: Address) -> Result<bool, ERC20Error> {
        let from = msg_sender();

        // Perform safety check 
        if from != self.owner.read() { return Err(ERC20Error::OnlyOwner) }; 
        if from == new_owner { return Err(ERC20Error::SelfTransfer) }; 

        // Update state
        self.owner.write(new_owner);

        // Emit event + return 
        log::emit(OwnershipTransferred::new(from, new_owner));
        Ok(true)
    }

    // -- READ-ONLY FUNCTIONS --------------------------------------------------
    pub fn owner(&self) -> Address {
        self.owner.read()
    }

    pub fn total_supply(&self) -> U256 {
        self.total_supply.read()
    }

    pub fn balance_of(&self, owner: Address) -> U256 {
        self.balance_of[owner].read()
    }

    pub fn allowance(&self, owner: Address, spender: Address) -> U256 {
        self.allowance_of[owner][spender].read()
    }
}
