#![no_std]
#![no_main]

pub mod math;
pub mod pair;
pub mod factory;
pub mod deployable;

use alloy_core::primitives::{Bytes, Address, U256};

#[interface]
trait IUniswapV2Factory {
    fn fee_to(&self) -> Address;
}

#[interface]
trait IUniswapV2Callee {
    fn uniswap_v2_call(&self, sender: Address, amount0: U256, amount1: U256, data: Bytes);
}
