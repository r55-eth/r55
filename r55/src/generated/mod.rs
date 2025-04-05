//! This module contains auto-generated code.
//! Do not edit manually!

use alloy_core::primitives::Bytes;
use core::include_bytes;

pub const ERC721_BYTECODE: &[u8] = include_bytes!("../../../r55-output-bytecode/erc721.bin");
pub const EVM_CALLER_BYTECODE: &[u8] = include_bytes!("../../../r55-output-bytecode/evm-caller.bin");
pub const ERC20_BYTECODE: &[u8] = include_bytes!("../../../r55-output-bytecode/erc20.bin");
pub const ERC20X_BYTECODE: &[u8] = include_bytes!("../../../r55-output-bytecode/erc20x.bin");

pub fn get_bytecode(contract_name: &str) -> Bytes {
    let initcode = match contract_name {
        "erc721" => ERC721_BYTECODE,
        "evm_caller" => EVM_CALLER_BYTECODE,
        "erc20" => ERC20_BYTECODE,
        "erc20x" => ERC20X_BYTECODE,
        _ => return Bytes::new(),
    };

    Bytes::from(initcode)
}
