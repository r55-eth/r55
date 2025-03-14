use alloy_core::hex::FromHex;
use alloy_primitives::address;
use revm::Database;
pub use revm::{
    primitives::{keccak256, ruint::Uint, AccountInfo, Address, Bytecode, Bytes, U256},
    InMemoryDB,
};
use std::{fs, path::Path, sync::Once};

static INIT: Once = Once::new();

pub const ALICE: Address = address!("000000000000000000000000000000000000000A");
pub const BOB: Address = address!("000000000000000000000000000000000000000B");
pub const CAROL: Address = address!("000000000000000000000000000000000000000C");

pub fn initialize_logger() {
    INIT.call_once(|| {
        let log_level = std::env::var("RUST_LOG").unwrap_or("INFO".to_owned());
        let tracing_sub = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_env_filter(tracing_subscriber::EnvFilter::new(log_level))
            .with_target(false)
            .finish();
        tracing::subscriber::set_global_default(tracing_sub)
            .expect("Setting tracing subscriber failed");
    });
}

pub fn add_balance_to_db(db: &mut InMemoryDB, addr: Address, value: u64) {
    db.insert_account_info(addr, AccountInfo::from_balance(U256::from(value)));
}

pub fn add_contract_to_db(db: &mut InMemoryDB, addr: Address, bytecode: Bytes) {
    let account = AccountInfo::new(
        Uint::from(0),
        0,
        keccak256(&bytecode),
        Bytecode::new_raw(bytecode),
    );
    db.insert_account_info(addr, account);
}

pub fn get_selector_from_sig(sig: &str) -> [u8; 4] {
    keccak256(sig)[0..4]
        .try_into()
        .expect("Selector should have exactly 4 bytes")
}

pub fn get_calldata(selector: [u8; 4], mut args: Vec<u8>) -> Vec<u8> {
    let mut calldata = selector.to_vec();
    calldata.append(&mut args);

    calldata
}

pub fn get_mapping_slot(key_bytes: Vec<u8>, id: U256) -> U256 {
    let mut data_bytes = Vec::with_capacity(64);
    data_bytes.extend_from_slice(&key_bytes);
    data_bytes.extend_from_slice(&id.to_be_bytes::<32>());

    keccak256(data_bytes).into()
}

pub fn read_db_slot(db: &mut InMemoryDB, contract: Address, slot: U256) -> U256 {
    db.storage(contract, slot)
        .expect("Unable to read storge slot")
}

pub fn load_bytecode_from_file<P: AsRef<Path>>(path: P) -> Bytes {
    let content = fs::read_to_string(path).expect("Unable to load bytecode from path");
    let trimmed = content.trim().trim_start_matches("0x");
    Bytes::from_hex(trimmed).expect("Unable to parse file content as bytes")
}
