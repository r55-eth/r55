mod exec;
use exec::{deploy_contract, run_tx};

mod error;
use error::Result;

use std::fs::File;
use std::io::Read;
use std::process::Command;

use alloy_core::hex;
use alloy_sol_types::SolValue;
use revm::{
    primitives::{address, keccak256, ruint::Uint, AccountInfo, Address, Bytecode, Bytes, U256},
    InMemoryDB,
};

fn compile_runtime(path: &str) -> eyre::Result<Vec<u8>> {
    println!("Compiling runtime: {}", path);
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
        eprintln!("Cargo command failed with status: {}", status);
        std::process::exit(1);
    } else {
        println!("Cargo command completed successfully");
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

fn compile_deploy(path: &str) -> eyre::Result<Vec<u8>> {
    compile_runtime(path)?;
    println!("Compiling deploy: {}", path);
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
        eprintln!("Cargo command failed with status: {}", status);
        std::process::exit(1);
    } else {
        println!("Cargo command completed successfully");
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

fn add_contract_to_db(db: &mut InMemoryDB, addr: Address, bytecode: Bytes) {
    let account = AccountInfo::new(
        Uint::from(0),
        0,
        keccak256(&bytecode),
        Bytecode::new_raw(bytecode),
    );
    db.insert_account_info(addr, account);
}

fn add_balance_to_db(db: &mut InMemoryDB, addr: Address, value: u64) {
    db.insert_account_info(addr, AccountInfo::from_balance(U256::from(value)));
}

fn test_runtime_from_binary() -> eyre::Result<()> {
    let rv_bytecode = compile_runtime("erc20")?;

    const CONTRACT_ADDR: Address = address!("0d4a11d5EEaaC28EC3F61d100daF4d40471f1852");
    let mut db = InMemoryDB::default();

    let mut bytecode = vec![0xff];
    bytecode.extend_from_slice(&rv_bytecode);

    let bytecode = Bytes::from(bytecode);

    add_contract_to_db(&mut db, CONTRACT_ADDR, bytecode);

    let selector_balance = &keccak256("balance_of")[0..4];
    let selector_mint = &keccak256("mint")[0..4];
    let selector_transfer = &keccak256("transfer")[0..4];
    let selector_approve = &keccak256("approve")[0..4];
    let selector_allowance = &keccak256("allowance")[0..4];
    let alice: Address = address!("0000000000000000000000000000000000000001");
    let bob: Address = address!("0000000000000000000000000000000000000002");
    let carol: Address = address!("0000000000000000000000000000000000000003");
    let value_mint: u64 = 42;
    let value_transfer: u64 = 21;
    let mut calldata_balance = alice.abi_encode();
    let mut calldata_mint = (alice, value_mint).abi_encode();
    let mut calldata_transfer = (bob, value_transfer).abi_encode();
    let mut calldata_approve = (carol, value_transfer).abi_encode();
    let mut calldata_allowance = (alice, carol).abi_encode();

    add_balance_to_db(&mut db, alice, 1e18 as u64);

    let mut complete_calldata_balance = selector_balance.to_vec();
    complete_calldata_balance.append(&mut calldata_balance);

    let mut complete_calldata_mint = selector_mint.to_vec();
    complete_calldata_mint.append(&mut calldata_mint);
    let mut complete_calldata_transfer = selector_transfer.to_vec();
    complete_calldata_transfer.append(&mut calldata_transfer);

    let mut complete_calldata_approve = selector_approve.to_vec();
    complete_calldata_approve.append(&mut calldata_approve);

    let mut complete_calldata_allowance = selector_allowance.to_vec();
    complete_calldata_allowance.append(&mut calldata_allowance);

    // Mint 42 tokens to Alice
    run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_mint.clone())?;
    // Check Alice's balance
    run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_balance.clone())?;
    // Transfer 21 tokens from Alice to Bob
    run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_transfer.clone())?;
    // Check Alice's balance
    run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_balance.clone())?;
    // Approve Carol to spend 21 token from Alice
    run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_approve.clone())?;
    // Check Carol's allowance
    run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_allowance.clone())?;

    Ok(())
    /*
    let account_db = &evm.db().accounts[&CONTRACT_ADDR];
    println!("Account storage: {:?}", account_db.storage);
    let slot_42 = account_db.storage[&U256::from(42)];
    assert_eq!(slot_42.as_limbs()[0], 0xdeadbeef);
    */
}

fn test_runtime(addr: &Address, db: &mut InMemoryDB) -> Result<()> {
    let selector_balance = &keccak256("balance_of")[0..4];
    let selector_mint = &keccak256("mint")[0..4];
    let alice: Address = address!("0000000000000000000000000000000000000001");
    let value_mint: u64 = 42;
    let mut calldata_balance = alice.abi_encode();
    let mut calldata_mint = (alice, value_mint).abi_encode();

    add_balance_to_db(db, alice, 1e18 as u64);

    let mut complete_calldata_balance = selector_balance.to_vec();
    complete_calldata_balance.append(&mut calldata_balance);

    let mut complete_calldata_mint = selector_mint.to_vec();
    complete_calldata_mint.append(&mut calldata_mint);

    run_tx(db, addr, complete_calldata_mint.clone())?;
    run_tx(db, addr, complete_calldata_balance.clone())?;

    Ok(())
}

fn test_deploy() -> eyre::Result<()> {
    let rv_bytecode = compile_deploy("erc20")?;
    let mut db = InMemoryDB::default();

    let mut bytecode = vec![0xff];
    bytecode.extend_from_slice(&rv_bytecode);

    let bytecode = Bytes::from(bytecode);

    let addr = deploy_contract(&mut db, bytecode)?;

    test_runtime(&addr, &mut db)?;
    Ok(())
}

fn test_transfer_logs() -> eyre::Result<()> {
    let rv_bytecode = compile_runtime("erc20")?;
    const CONTRACT_ADDR: Address = address!("0d4a11d5EEaaC28EC3F61d100daF4d40471f1852");
    let mut db = InMemoryDB::default();

    // Setup contract
    let mut bytecode = vec![0xff];
    bytecode.extend_from_slice(&rv_bytecode);
    add_contract_to_db(&mut db, CONTRACT_ADDR, Bytes::from(bytecode));

    // Setup addresses
    let alice: Address = address!("0000000000000000000000000000000000000001");
    let bob: Address = address!("0000000000000000000000000000000000000002");

    // Add balance to Alice's account for gas fees
    add_balance_to_db(&mut db, alice, 1e18 as u64);

    // Mint tokens to Alice
    let selector_mint = &keccak256("mint")[0..4];
    let mut calldata_mint = (alice, 100u64).abi_encode();
    let mut complete_mint_calldata = selector_mint.to_vec();
    complete_mint_calldata.append(&mut calldata_mint);

    let mint_result = run_tx(&mut db, &CONTRACT_ADDR, complete_mint_calldata)?;
    println!("Mint result status: {}", mint_result.status);

    println!("\n=== Transfer Logs Test ===");
    println!("Expected Transfer Event:");
    println!("- From: {}", alice);
    println!("- To: {}", bob);
    println!("- Amount: 50 tokens");

    let selector_transfer = &keccak256("transfer")[0..4];
    let mut calldata_transfer = (bob, 50u64).abi_encode();
    let mut complete_transfer_calldata = selector_transfer.to_vec();
    complete_transfer_calldata.append(&mut calldata_transfer);

    let transfer_result = run_tx(&mut db, &CONTRACT_ADDR, complete_transfer_calldata)?;

    println!("\nActual Transfer Log:");
    if let Some(log) = transfer_result.logs.first() {
        let topics = log.data.topics();
        println!("- Event Hash: {}", hex::encode(topics[0]));
        println!("- From: 0x{}", hex::encode(&topics[1][12..]));
        println!("- To: 0x{}", hex::encode(&topics[2][12..]));
        let amount = u64::from_be_bytes(log.data.data[24..32].try_into().unwrap());
        println!("- Amount: {} tokens", amount);
    }

    Ok(())
}

fn main() -> eyre::Result<()> {
    test_runtime_from_binary()?;
    test_deploy();
    test_transfer_logs()
}
