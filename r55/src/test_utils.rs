pub use revm::{
    primitives::{keccak256, ruint::Uint, AccountInfo, Address, Bytecode, Bytes, U256},
    InMemoryDB,
};
use std::sync::Once;

static INIT: Once = Once::new();

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
