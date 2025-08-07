#![no_std]
#![no_main]

extern crate alloc;
use core::default::Default;
use core::str::FromStr;

use contract_derive::{contract, interface, payable, storage, Error, Event};
use eth_riscv_runtime::{types::*, *};

use alloy_core::primitives::{address, keccak256 as alloy_keccak, Address, Bytes, B256, U256, U8};
use crate::deployable::UniswapV2Pair;

// -- EVENTS -------------------------------------------------------------------
#[derive(Event)]
pub struct PairCreated {
    #[indexed]
    pub token0: Address,
    #[indexed]
    pub token1: Address,
    pub pair: Address,
    pub total_pairs: U256,
}

// -- ERRORS -------------------------------------------------------------------
#[derive(Error)]
pub enum UniswapV2FactoryError {
    SameToken,
    ZeroAddress,
    Unauthorized,
    PairExists,
    InitializationFailed,
}

// -- CONTRACT -----------------------------------------------------------------
#[storage]
pub struct UniswapV2Factory {
    // Fee configuration
    fee_to: Slot<Address>,
    fee_to_setter: Slot<Address>,

    // Pair storage
    pairs: Mapping<Address, Mapping<Address, Slot<Address>>>,
    // TODO: handle Vec storage 
    // all_pairs: Vec<Address>,
}

#[contract]
impl UniswapV2Factory {
    // -- CONSTRUCTOR ----------------------------------------------------------
    pub fn new(fee_to_setter: Address) -> Self {
        let mut factory = UniswapV2Factory::default();

        // Set factory as deployer
        factory.fee_to_setter.write(fee_to_setter);

        factory
    }

    // -- STATE MODIFYING FUNCTIONS -------------------------------------------
    pub fn create_pair(&mut self, token_a: Address, token_b: Address) -> Result<Address, UniswapV2FactoryError> {
        // Perform token checks
        if token_a == token_b {
            return Err(UniswapV2FactoryError::SameToken);
        }

        let (token0, token1) = if token_a < token_b {
            (token_a, token_b)
        } else {
            (token_b, token_a)
        };

        if token0 == Address::ZERO {
            return Err(UniswapV2FactoryError::ZeroAddress);
        }
        if self.pairs[token0][token1].read() != Address::ZERO {
            return Err(UniswapV2FactoryError::PairExists)
        }

        // Deploy pair contract --> TODO: impl CREATE2
        let mut pair = UniswapV2Pair::deploy(()).with_ctx(&mut *self);
        pair.initialize(token0, token1).map_err(|_| UniswapV2FactoryError::InitializationFailed)?;

        // Update storage
        self.pairs[token0][token1].write(pair.address());
        self.pairs[token1][token0].write(pair.address());

        // Emit event and return the pair address
        log::emit(PairCreated::new(token0, token1, pair.address(), U256::ZERO));

        Ok(pair.address())
    }

    pub fn set_fee_to(&mut self, fee_to: Address) -> Result<(), UniswapV2FactoryError> {
        if msg_sender() != self.fee_to_setter.read() {
            return Err(UniswapV2FactoryError::Unauthorized);
        }

        // Update state
        self.fee_to.write(fee_to); 
        Ok(())
    }

    pub fn set_fee_to_setter(&mut self, fee_to_setter: Address) -> Result<(), UniswapV2FactoryError> {
        if msg_sender() != self.fee_to_setter.read() {
            return Err(UniswapV2FactoryError::Unauthorized);
        }

        // Update state
        self.fee_to_setter.write(fee_to_setter); 
        Ok(())
    }

    // -- READ-ONLY FUNCTIONS -------------------------------------------------
    pub fn fee_to(&self) -> Address {
        self.fee_to.read()
    }

    pub fn fee_to_setter(&self) -> Address {
        self.fee_to_setter.read()
    }

    pub fn pair(&self, token0: Address, token1: Address) -> Address {
        self.pairs[token0][token1].read()
    }
}

