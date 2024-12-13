pub mod exec;

mod error;
mod gas;

pub mod test_utils;

use alloy_primitives::Bytes;
use std::fs::File;
use std::io::Read;
use std::process::Command;
use tracing::{error, info};

fn compile_runtime(path: &str) -> eyre::Result<Vec<u8>> {
    info!("Compiling runtime: {}", path);
    let status = Command::new("cargo")
        .arg("+nightly-2024-02-01")
        .arg("build")
        .arg("-r")
        .arg("--lib")
        .arg("-Z")
        .arg("build-std=core,alloc")
        .arg("--target")
        .arg("riscv64imac-unknown-none-elf")
        .arg("--bin")
        .arg("runtime")
        .current_dir(path)
        .status()
        .expect("Failed to execute cargo command");

    if !status.success() {
        error!("Cargo command failed with status: {}", status);
        std::process::exit(1);
    } else {
        info!("Cargo command completed successfully");
    }

    let path = format!(
        "{}/target/riscv64imac-unknown-none-elf/release/runtime",
        path
    );
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(e) => {
            eyre::bail!("Failed to open file: {}", e);
        }
    };

    // Read the file contents into a vector.
    let mut bytecode = Vec::new();
    if let Err(e) = file.read_to_end(&mut bytecode) {
        eyre::bail!("Failed to read file: {}", e);
    }

    Ok(bytecode)
}

pub fn compile_deploy(path: &str) -> eyre::Result<Vec<u8>> {
    compile_runtime(path)?;
    info!("Compiling deploy: {}", path);
    let status = Command::new("cargo")
        .arg("+nightly-2024-02-01")
        .arg("build")
        .arg("-r")
        .arg("--lib")
        .arg("-Z")
        .arg("build-std=core,alloc")
        .arg("--target")
        .arg("riscv64imac-unknown-none-elf")
        .arg("--bin")
        .arg("deploy")
        .current_dir(path)
        .status()
        .expect("Failed to execute cargo command");

    if !status.success() {
        error!("Cargo command failed with status: {}", status);
        std::process::exit(1);
    } else {
        info!("Cargo command completed successfully");
    }

    let path = format!(
        "{}/target/riscv64imac-unknown-none-elf/release/deploy",
        path
    );
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(e) => {
            eyre::bail!("Failed to open file: {}", e);
        }
    };

    // Read the file contents into a vector.
    let mut bytecode = Vec::new();
    if let Err(e) = file.read_to_end(&mut bytecode) {
        eyre::bail!("Failed to read file: {}", e);
    }

    Ok(bytecode)
}

pub fn compile_with_prefix<F>(compile_fn: F, path: &str) -> eyre::Result<Bytes>
where
    F: FnOnce(&str) -> eyre::Result<Vec<u8>>,
{
    let bytecode = compile_fn(path)?;
    let mut prefixed_bytecode = vec![0xff]; // Add the 0xff prefix
    prefixed_bytecode.extend_from_slice(&bytecode);
    Ok(Bytes::from(prefixed_bytecode))
}

#[cfg(test)]
mod tests {
    use crate::exec::run_tx;
    use crate::{compile_runtime, compile_with_prefix, test_utils::*};

    use alloy_core::hex::{self, ToHexExt};
    use alloy_sol_types::SolValue;
    use revm::primitives::address;

    const ERC20_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../erc20");

    #[test]
    fn test_runtime() {
        initialize_logger();

        const CONTRACT_ADDR: Address = address!("0d4a11d5EEaaC28EC3F61d100daF4d40471f1852");
        let mut db = InMemoryDB::default();

        let bytecode = compile_with_prefix(compile_runtime, ERC20_PATH).unwrap();
        add_contract_to_db(&mut db, CONTRACT_ADDR, bytecode);

        let selector_balance = get_selector_from_sig("balance_of");
        let selector_mint = get_selector_from_sig("mint");
        let selector_transfer = get_selector_from_sig("transfer");
        let selector_approve = get_selector_from_sig("approve");
        let selector_allowance = get_selector_from_sig("allowance");
        let alice: Address = address!("000000000000000000000000000000000000000A");
        let bob: Address = address!("000000000000000000000000000000000000000B");
        let carol: Address = address!("000000000000000000000000000000000000000C");
        let value_mint: u64 = 42;
        let value_transfer: u64 = 21;
        let mut calldata_alice_balance = alice.abi_encode();
        let mut calldata_bob_balance = bob.abi_encode();
        let mut calldata_mint = (alice, value_mint).abi_encode();
        let mut calldata_transfer = (bob, value_transfer).abi_encode();
        let mut calldata_approve = (carol, value_transfer).abi_encode();
        let mut calldata_allowance = (alice, carol).abi_encode();

        add_balance_to_db(&mut db, alice, 1e18 as u64);

        let mut complete_calldata_mint = selector_mint.to_vec();
        complete_calldata_mint.append(&mut calldata_mint);
        let mut complete_calldata_alice_balance = selector_balance.to_vec();
        complete_calldata_alice_balance.append(&mut calldata_alice_balance);
        let mut complete_calldata_bob_balance = selector_balance.to_vec();
        complete_calldata_bob_balance.append(&mut calldata_bob_balance);
        let mut complete_calldata_transfer = selector_transfer.to_vec();
        complete_calldata_transfer.append(&mut calldata_transfer);
        let mut complete_calldata_approve = selector_approve.to_vec();
        complete_calldata_approve.append(&mut calldata_approve);
        let mut complete_calldata_allowance = selector_allowance.to_vec();
        complete_calldata_allowance.append(&mut calldata_allowance);

        // Mint 42 tokens to Alice
        let mint_result = run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_mint.clone()).unwrap();
        assert!(mint_result.status, "Mint transaction failed");

        // Check Alice's balance
        let alice_balance = run_tx(
            &mut db,
            &CONTRACT_ADDR,
            complete_calldata_alice_balance.clone(),
        )
        .unwrap();
        assert_eq!(
            alice_balance.output,
            Uint::from(42).abi_encode(),
            "Incorrect balance"
        );

        // Transfer 21 tokens from Alice to Bob
        let transfer_result =
            run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_transfer.clone()).unwrap();
        assert!(transfer_result.status, "Transfer transaction failed");

        // Check Alice's and Bob's balance
        let alice_balance = run_tx(
            &mut db,
            &CONTRACT_ADDR,
            complete_calldata_alice_balance.clone(),
        )
        .unwrap()
        .output;
        assert_eq!(alice_balance, Uint::from(21).abi_encode());
        let bob_balance = run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_bob_balance).unwrap();
        assert_eq!(bob_balance.output, Uint::from(21).abi_encode());

        // Approve Carol to spend 21 token from Alice
        let approve_result =
            run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_approve.clone()).unwrap();
        assert!(approve_result.status, "Approve transaction failed");

        // Check Carol's allowance
        let allowance_res =
            run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_allowance.clone()).unwrap();
        assert_eq!(allowance_res.output, Uint::from(21).abi_encode());
    }

    #[test]
    fn test_transfer_logs() {
        initialize_logger();

        const CONTRACT_ADDR: Address = address!("0d4a11d5EEaaC28EC3F61d100daF4d40471f1852");
        let mut db = InMemoryDB::default();

        // Setup contract
        let bytecode = compile_with_prefix(compile_runtime, ERC20_PATH).unwrap();
        add_contract_to_db(&mut db, CONTRACT_ADDR, bytecode);

        // Setup addresses
        let alice: Address = address!("000000000000000000000000000000000000000A");
        let bob: Address = address!("000000000000000000000000000000000000000B");

        // Add balance to Alice's account for gas fees
        add_balance_to_db(&mut db, alice, 1e18 as u64);

        // Mint tokens to Alice
        let selector_mint = get_selector_from_sig("mint");
        let mut calldata_mint = (alice, 100u64).abi_encode();
        let mut complete_mint_calldata = selector_mint.to_vec();
        complete_mint_calldata.append(&mut calldata_mint);

        let mint_result = run_tx(&mut db, &CONTRACT_ADDR, complete_mint_calldata).unwrap();
        assert!(mint_result.status, "Mint transaction failed");

        // Transfer tokens from Alice to Bob
        let selector_transfer = get_selector_from_sig("transfer");
        let mut calldata_transfer = (bob, 50u64).abi_encode();
        let mut complete_transfer_calldata = selector_transfer.to_vec();
        complete_transfer_calldata.append(&mut calldata_transfer);

        let transfer_result = run_tx(&mut db, &CONTRACT_ADDR, complete_transfer_calldata).unwrap();

        // Assert the transfer log
        assert!(
            !transfer_result.logs.is_empty(),
            "No logs found in transfer transaction"
        );
        let log = &transfer_result.logs[0];
        let topics = log.data.topics();

        // Expected event hash for Transfer event
        let expected_event_hash = keccak256("Transfer(address,address,uint64)");
        assert_eq!(
            hex::encode(topics[0]),
            hex::encode(expected_event_hash),
            "Incorrect event hash"
        );

        // Assert "from" address in log
        assert_eq!(
            hex::encode(&topics[1][12..]),
            alice.encode_hex(),
            "Incorrect 'from' address in transfer log"
        );

        // Assert "to" address in log
        assert_eq!(
            hex::encode(&topics[2][12..]),
            bob.encode_hex(),
            "Incorrect 'to' address in transfer log"
        );

        // Assert transfer amount
        let amount = u64::from_be_bytes(log.data.data[24..32].try_into().unwrap());
        assert_eq!(amount, 50, "Incorrect transfer amount in transfer log");
    }
}
