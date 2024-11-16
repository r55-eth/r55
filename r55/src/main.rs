mod exec;
use exec::{deploy_contract, run_tx};

use std::fs::File;
use std::io::Read;
use std::process::Command;

use alloy_sol_types::SolValue;
use revm::{
    primitives::{address, keccak256, ruint::Uint, AccountInfo, Address, Bytecode, Bytes},
    InMemoryDB,
};
use alloy_core::hex;

fn compile_runtime(path: &str) -> Result<Vec<u8>, ()> {
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
            eprintln!("Failed to open file: {}", e);
            return Err(());
        }
    };

    // Read the file contents into a vector.
    let mut bytecode = Vec::new();
    if let Err(e) = file.read_to_end(&mut bytecode) {
        eprintln!("Failed to read file: {}", e);
        return Err(());
    }

    Ok(bytecode)
}

fn compile_deploy(path: &str) -> Result<Vec<u8>, ()> {
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
            eprintln!("Failed to open file: {}", e);
            return Err(());
        }
    };

    // Read the file contents into a vector.
    let mut bytecode = Vec::new();
    if let Err(e) = file.read_to_end(&mut bytecode) {
        eprintln!("Failed to read file: {}", e);
        return Err(());
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

fn test_runtime_from_binary() {
    let rv_bytecode = compile_runtime("erc20").unwrap();

    const CONTRACT_ADDR: Address = address!("0d4a11d5EEaaC28EC3F61d100daF4d40471f1852");
    let mut db = InMemoryDB::default();

    let mut bytecode = vec![0xff];
    bytecode.extend_from_slice(&rv_bytecode);

    let bytecode = Bytes::from(bytecode);

    add_contract_to_db(&mut db, CONTRACT_ADDR, bytecode);

    let selector_balance = &keccak256("balance_of")[0..4];
    let selector_mint = &keccak256("mint")[0..4];
    let to: Address = address!("0000000000000000000000000000000000000001");
    let value_mint: u64 = 42;
    let mut calldata_balance = to.abi_encode();
    let mut calldata_mint = (to, value_mint).abi_encode();

    let mut complete_calldata_balance = selector_balance.to_vec();
    complete_calldata_balance.append(&mut calldata_balance);

    let mut complete_calldata_mint = selector_mint.to_vec();
    complete_calldata_mint.append(&mut calldata_mint);

    run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_mint.clone());
    run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_balance.clone());

    /*
    let account_db = &evm.db().accounts[&CONTRACT_ADDR];
    println!("Account storage: {:?}", account_db.storage);
    let slot_42 = account_db.storage[&U256::from(42)];
    assert_eq!(slot_42.as_limbs()[0], 0xdeadbeef);
    */
}

fn test_runtime(addr: &Address, db: &mut InMemoryDB) {
    let selector_balance = &keccak256("balance_of")[0..4];
    let selector_mint = &keccak256("mint")[0..4];
    let to: Address = address!("0000000000000000000000000000000000000001");
    let value_mint: u64 = 42;
    let mut calldata_balance = to.abi_encode();
    let mut calldata_mint = (to, value_mint).abi_encode();

    let mut complete_calldata_balance = selector_balance.to_vec();
    complete_calldata_balance.append(&mut calldata_balance);

    let mut complete_calldata_mint = selector_mint.to_vec();
    complete_calldata_mint.append(&mut calldata_mint);

    run_tx(db, addr, complete_calldata_mint.clone());
    run_tx(db, addr, complete_calldata_balance.clone());
}

fn test_deploy() {
    let rv_bytecode = compile_deploy("erc20").unwrap();
    let mut db = InMemoryDB::default();

    let mut bytecode = vec![0xff];
    bytecode.extend_from_slice(&rv_bytecode);

    let bytecode = Bytes::from(bytecode);

    let addr = deploy_contract(&mut db, bytecode);

    test_runtime(&addr, &mut db);
}


//////////////////////////
///     TESTS           //
//////////////////////////

fn parse_hex_result(result: &[u8]) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&result[24..32]); 
    u64::from_be_bytes(bytes)
}

fn parse_bool_result(hex_result: &[u8]) -> bool {
    if hex_result.len() == 32 {
        hex_result[31] == 1
    } else {
        false
    }
}

fn test_erc20_mint_and_transfer() {
    let rv_bytecode = compile_runtime("erc20").unwrap();
    println!("Bytecode length: {}", rv_bytecode.len());

    const CONTRACT_ADDR: Address = address!("0d4a11d5EEaaC28EC3F61d100daF4d40471f1852");
    let mut db = InMemoryDB::default();

    let mut bytecode = vec![0xff];
    bytecode.extend_from_slice(&rv_bytecode);
    let bytecode = Bytes::from(bytecode);
    add_contract_to_db(&mut db, CONTRACT_ADDR, bytecode);

    let selector_balance = &keccak256("balance_of")[0..4];
    let selector_transfer = &keccak256("transfer")[0..4];
    let selector_mint = &keccak256("mint")[0..4];
    
    let address1: Address = address!("0000000000000000000000000000000000000001");
    let address2: Address = address!("0000000000000000000000000000000000000002");
    
    // Mint 100 tokens
    let mint_value: u64 = 100;
    let mut calldata_mint = (address1, mint_value).abi_encode();
    let mut complete_calldata_mint = selector_mint.to_vec();
    complete_calldata_mint.append(&mut calldata_mint);
    
    println!("\n=== Minting tokens ===");
    println!("Minting {} tokens to address1...", mint_value);
    let mint_result = run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_mint);
    println!("Mint tx result: {}", parse_bool_result(&mint_result.output));
    
    // Print mint logs
    println!("\nMint Event Logs:");
    for log in mint_result.logs {
        println!("    Event Signature: 0x{}", hex::encode(&log.topics()[0]));
        if log.topics().len() > 1 {
            println!("    Address: 0x{}", hex::encode(&log.topics()[1]));
        }
        println!("    Data: 0x{}", hex::encode(&log.data.data));
    }

    // Check initial balance
    let mut calldata_balance = address1.abi_encode();
    let mut complete_calldata_balance = selector_balance.to_vec();
    complete_calldata_balance.append(&mut calldata_balance);
    
    println!("\n=== Checking initial balance ===");
    let balance_result = run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_balance.clone());
    let initial_balance = parse_hex_result(&balance_result.output);
    println!("Address1 balance: {} (hex: 0x{})", initial_balance, hex::encode(&balance_result.output));
    assert_eq!(initial_balance, mint_value, "Initial balance should be {}", mint_value);

    // Transfer 30 tokens
    let transfer_value: u64 = 30;
    let mut calldata_transfer = (address1, address2, transfer_value).abi_encode();
    let mut complete_calldata_transfer = selector_transfer.to_vec();
    complete_calldata_transfer.append(&mut calldata_transfer);
    
    println!("\n=== Transferring tokens ===");
    println!("Transferring {} tokens from address1 to address2...", transfer_value);
    let transfer_result = run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_transfer);
    println!("Transfer tx result: {}", parse_bool_result(&transfer_result.output));

    // Print transfer logs
    println!("\nTransfer Event Logs:");
    for log in transfer_result.logs {
        println!("    Event Signature: 0x{}", hex::encode(&log.topics()[0]));
        for (i, topic) in log.topics().iter().enumerate().skip(1) {
            println!("    Topic {}: 0x{}", i, hex::encode(topic));
        }
        println!("    Data: 0x{}", hex::encode(&log.data.data));
    }

    // Check final balances
    println!("\n=== Checking final balances ===");
    let balance_result1 = run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_balance.clone());
    let final_balance1 = parse_hex_result(&balance_result1.output);
    println!("Address1 final balance: {} (hex: 0x{})", final_balance1, hex::encode(&balance_result1.output));
    assert_eq!(final_balance1, mint_value - transfer_value, 
        "Address1 should have {} tokens", mint_value - transfer_value);

    let mut calldata_balance2 = address2.abi_encode();
    let mut complete_calldata_balance2 = selector_balance.to_vec();
    complete_calldata_balance2.append(&mut calldata_balance2);
    
    let balance_result2 = run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_balance2);
    let final_balance2 = parse_hex_result(&balance_result2.output);
    println!("Address2 final balance: {} (hex: 0x{})", final_balance2, hex::encode(&balance_result2.output));
    assert_eq!(final_balance2, transfer_value, 
        "Address2 should have {} tokens", transfer_value);

    println!("\n=== Test Summary ===");
    println!("✓ Initial mint: {} tokens to address1", mint_value);
    println!("✓ Initial balance of address1: {} tokens", initial_balance);
    println!("✓ Transfer: {} tokens from address1 to address2", transfer_value);
    println!("✓ Final balance of address1: {} tokens", final_balance1);
    println!("✓ Final balance of address2: {} tokens", final_balance2);
}

fn test_transfer_event_values() {
    let rv_bytecode = compile_runtime("erc20").unwrap();
    const CONTRACT_ADDR: Address = address!("0d4a11d5EEaaC28EC3F61d100daF4d40471f1852");
    let mut db = InMemoryDB::default();

    let mut bytecode = vec![0xff];
    bytecode.extend_from_slice(&rv_bytecode);
    let bytecode = Bytes::from(bytecode);
    add_contract_to_db(&mut db, CONTRACT_ADDR, bytecode);

    let from_addr = address!("0000000000000000000000000000000000000001");
    let to_addr = address!("0000000000000000000000000000000000000002");
    let mint_amount = 1000u64;
    let transfer_amount = 500u64;

    // Mint 
    let selector_mint = &keccak256("mint")[0..4];
    let mut calldata_mint = (from_addr, mint_amount).abi_encode();
    let mut complete_calldata_mint = selector_mint.to_vec();
    complete_calldata_mint.append(&mut calldata_mint);
    let mint_result = run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_mint);
    assert!(parse_bool_result(&mint_result.output), "Mint failed");

    // Transfer 
    let selector_transfer = &keccak256("transfer")[0..4];
    let mut calldata_transfer = (from_addr, to_addr, transfer_amount).abi_encode();
    let mut complete_calldata_transfer = selector_transfer.to_vec();
    complete_calldata_transfer.append(&mut calldata_transfer);
    let transfer_result = run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_transfer);

    // Print events
    let log = &transfer_result.logs[0];
    let log_mint = &mint_result.logs[0];

    println!("\n=== Transfer Event - Expected vs Actual Values ===");
    println!("Contract Address:");
    println!("  Expected: {:?}", CONTRACT_ADDR);
    println!("  Actual  : {:?}", log.address);
    println!();
    
    println!("From Address:");
    println!("  Expected: {:?}", from_addr);
    println!("  Actual  : 0x{}", hex::encode(&log.topics()[1]));
    println!();
    
    println!("To Address:");
    println!("  Expected: {:?}", to_addr);
    println!("  Actual  : 0x{}", hex::encode(&log.topics()[2]));
    println!();
    
    println!("Transfer Amount:");
    println!("  Expected: {} tokens", transfer_amount);
    println!("  Actual  : {} tokens", parse_hex_result(&log.data.data));

    println!("\n=== MINT Event - Expected vs Actual Values ===");
    println!("Contract Address:");
    println!("  Expected: {:?}", CONTRACT_ADDR);
    println!("  Actual  : {:?}", log_mint.address);
    println!();

    println!("TO Address:");
    println!("  Expected: {:?}", from_addr);
    println!("  Actual  : 0x{}", hex::encode(&log_mint.topics()[1]));
    println!();

    println!("Transfer Amount:");
    println!("  Expected: {} tokens", mint_amount);
    println!("  Actual  : {} tokens", parse_hex_result(&log_mint.data.data));

    assert_eq!(log.address, CONTRACT_ADDR, "Contract address mismatch");
    assert_eq!(parse_hex_result(&log.data.data), transfer_amount, "Transfer amount mismatch");
}

fn test_burn_event() {
    let rv_bytecode = compile_runtime("erc20").unwrap();
    const CONTRACT_ADDR: Address = address!("0d4a11d5EEaaC28EC3F61d100daF4d40471f1852");
    let mut db = InMemoryDB::default();

    let mut bytecode = vec![0xff];
    bytecode.extend_from_slice(&rv_bytecode);
    let bytecode = Bytes::from(bytecode);
    add_contract_to_db(&mut db, CONTRACT_ADDR, bytecode);

    let address1 = address!("0000000000000000000000000000000000000001");
    let burn_amount = 500u64;
    
    // Mint
    let selector_mint = &keccak256("mint")[0..4];
    let mut calldata_mint = (address1, 1000u64).abi_encode();
    let mut complete_calldata_mint = selector_mint.to_vec();
    complete_calldata_mint.append(&mut calldata_mint);
    run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_mint);

    // Burn
    let selector_burn = &keccak256("burn")[0..4];
    let mut calldata_burn = (address1, burn_amount).abi_encode();
    let mut complete_calldata_burn = selector_burn.to_vec();
    complete_calldata_burn.append(&mut calldata_burn);
    
    let burn_result = run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_burn);
    let log = &burn_result.logs[0];

    println!("\nEmitted Events:");
    println!("    Burn(uint64)");
    println!("        value: {} (input: {})", parse_hex_result(&log.data.data), burn_amount);

    assert_eq!(parse_hex_result(&log.data.data), burn_amount, "Event value doesn't match input value");
}

fn test_pause_event() {
    let rv_bytecode = compile_runtime("erc20").unwrap();
    const CONTRACT_ADDR: Address = address!("0d4a11d5EEaaC28EC3F61d100daF4d40471f1852");
    let mut db = InMemoryDB::default();

    let mut bytecode = vec![0xff];
    bytecode.extend_from_slice(&rv_bytecode);
    let bytecode = Bytes::from(bytecode);
    add_contract_to_db(&mut db, CONTRACT_ADDR, bytecode);

    let pause_state = true;
    let selector_pause = &keccak256("set_paused")[0..4];
    let mut calldata_pause = pause_state.abi_encode();
    let mut complete_calldata_pause = selector_pause.to_vec();
    complete_calldata_pause.append(&mut calldata_pause);
    
    let pause_result = run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_pause);
    let log = &pause_result.logs[0];

    println!("\nEmitted Events:");
    println!("    PauseChanged(bool)");
    println!("        paused: {} (input: {})", parse_bool_result(&log.data.data), pause_state);

    assert_eq!(parse_bool_result(&log.data.data), pause_state, "Wrong pause state");
}

fn test_metadata_event() {
    let rv_bytecode = compile_runtime("erc20").unwrap();
    const CONTRACT_ADDR: Address = address!("0d4a11d5EEaaC28EC3F61d100daF4d40471f1852");
    let mut db = InMemoryDB::default();

    let mut bytecode = vec![0xff];
    bytecode.extend_from_slice(&rv_bytecode);
    let bytecode = Bytes::from(bytecode);
    add_contract_to_db(&mut db, CONTRACT_ADDR, bytecode);

    let metadata: [u8; 32] = [1u8; 32];
    let selector_metadata = &keccak256("update_metadata")[0..4];
    let mut calldata_metadata = metadata.abi_encode();
    let mut complete_calldata_metadata = selector_metadata.to_vec();
    complete_calldata_metadata.append(&mut calldata_metadata);
    
    let metadata_result = run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_metadata);
    let log = &metadata_result.logs[0];

    println!("\nEmitted Events:");
    println!("    MetadataUpdated(bytes32)");
    println!("        data: 0x{} (input: 0x{})", hex::encode(&log.data.data), hex::encode(&metadata));

    assert_eq!(log.data.data.as_ref(), &metadata[..], "Wrong metadata");
}

fn test_transfer_event() {
    let rv_bytecode = compile_runtime("erc20").unwrap();
    const CONTRACT_ADDR: Address = address!("0d4a11d5EEaaC28EC3F61d100daF4d40471f1852");
    let mut db = InMemoryDB::default();

    let mut bytecode = vec![0xff];
    bytecode.extend_from_slice(&rv_bytecode);
    let bytecode = Bytes::from(bytecode);
    add_contract_to_db(&mut db, CONTRACT_ADDR, bytecode);

    let from = address!("0000000000000000000000000000000000000001");
    let to = address!("0000000000000000000000000000000000000002");
    let amount = 1000u64;

    // Mint
    let selector_mint = &keccak256("mint")[0..4];
    let mut calldata_mint = (from, amount * 2).abi_encode();
    let mut complete_calldata_mint = selector_mint.to_vec();
    complete_calldata_mint.append(&mut calldata_mint);
    run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_mint);

    // Transfer
    let selector_transfer = &keccak256("transfer")[0..4];
    let mut calldata_transfer = (from, to, amount).abi_encode();
    let mut complete_calldata_transfer = selector_transfer.to_vec();
    complete_calldata_transfer.append(&mut calldata_transfer);
    
    let transfer_result = run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_transfer);
    let log = &transfer_result.logs[0];

    println!("\nEmitted Events:");
    println!("    Transfer(address,address,uint64)");
    println!("        from : {} (input: {})", hex::encode(&log.topics()[1]), hex::encode(from));
    println!("        to   : {} (input: {})", hex::encode(&log.topics()[2]), hex::encode(to));
    println!("        value: {} (input: {})", parse_hex_result(&log.data.data), amount);

    assert_eq!(Address::from_slice(&log.topics()[1][12..]), from, "Wrong from address");
    assert_eq!(Address::from_slice(&log.topics()[2][12..]), to, "Wrong to address");
    assert_eq!(parse_hex_result(&log.data.data), amount, "Wrong transfer amount");
}
fn main() {
    test_runtime_from_binary();
    test_deploy();
    test_erc20_mint_and_transfer();
    test_transfer_event_values();
    test_burn_event();
    test_pause_event();
    test_metadata_event();
    test_transfer_event();
}
