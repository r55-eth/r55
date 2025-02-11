use alloy_primitives::{address, Address, Bytes, U256};
use alloy_sol_types::SolValue;
use r55::{
    compile_deploy, compile_with_prefix,
    exec::{deploy_contract, run_tx},
    test_utils::{add_balance_to_db, get_selector_from_sig, initialize_logger},
};
use revm::InMemoryDB;
use tracing::{debug, error, info};

const ERC20_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/erc20");

#[test]
fn erc20() {
    initialize_logger();

    let mut db = InMemoryDB::default();

    let alice: Address = address!("000000000000000000000000000000000000000A");
    let bob: Address = address!("000000000000000000000000000000000000000B");
    add_balance_to_db(&mut db, alice, 1e18 as u64);

    let constructor = bob.abi_encode();

    let bytecode = compile_with_prefix(compile_deploy, ERC20_PATH).unwrap();
    let addr1 = deploy_contract(&mut db, bytecode, Some(constructor)).unwrap();

    let selector_mint = get_selector_from_sig("r55_mint");
    let alice: Address = address!("000000000000000000000000000000000000000A");
    let value_mint = U256::from(42e18);
    let mut calldata_mint = (alice, value_mint).abi_encode();

    let mut complete_calldata_mint = selector_mint.to_vec();
    complete_calldata_mint.append(&mut calldata_mint);

    info!("----------------------------------------------------------");
    info!("-- MINT TX -----------------------------------------------");
    info!("----------------------------------------------------------");
    debug!(
        "Tx Calldata:\n> {:#?}",
        Bytes::from(complete_calldata_mint.clone())
    );
    match run_tx(&mut db, &addr1, complete_calldata_mint.clone()) {
        Ok(res) => info!("{}", res),
        Err(e) => {
            error!("Error when executing tx! {:#?}", e);
            panic!()
        }
    };
}
