//! Auto-generated based on Cargo.toml dependencies
//! This file provides `Deployable` implementations for contract dependencies
//! TODO (phase-2): rather than using `fn deploy(args: Args)`, figure out the constructor selector from the contract dependency

use alloy_core::primitives::{Address, Bytes};
use eth_riscv_runtime::{create::Deployable, InitInterface, ReadOnly};
use core::include_bytes;

use crate::pair::IUniswapV2Pair;

const UNISWAP_V2_PAIR_BYTECODE: &'static [u8] = include_bytes!("../../../../r55-output-bytecode/uniswap-v2-pair.bin");

pub struct UniswapV2Pair;

impl Deployable for UniswapV2Pair {
    type Interface = IUniswapV2Pair<ReadOnly>;

    fn __runtime() -> &'static [u8] {
        UNISWAP_V2_PAIR_BYTECODE
    }
}

