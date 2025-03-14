//! Auto-generated based on Cargo.toml dependencies
//! This file provides Deployable implementations for contract dependencies
//! TODO (phase-2): rather than using `fn deploy(args: Args)`, figure out the constructor selector from the contract dependency

use alloy_core::primitives::{Address, Bytes};
use eth_riscv_runtime::{create::Deployable, InitInterface, ReadOnly};
use core::include_bytes;

use erc20::IERC20;

const ERC20_BYTECODE: &'static [u8] = include_bytes!("../../../r55-output-bytecode/erc20.bin");

pub struct ERC20;

impl Deployable for ERC20 {
    type Interface = IERC20<ReadOnly>;

    fn __runtime() -> &'static [u8] {
        ERC20_BYTECODE
    }
}

