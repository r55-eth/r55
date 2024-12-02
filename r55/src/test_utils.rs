pub use revm::{
    primitives::{keccak256, ruint::Uint, AccountInfo, Address, Bytecode, Bytes, U256},
    InMemoryDB,
};
use std::sync::Once;

static INIT: Once = Once::new();

pub fn initialize_logger() {
    INIT.call_once(|| {
        env_logger::builder().is_test(true).try_init().unwrap();
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
