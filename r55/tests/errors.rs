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
const ERC20X_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/erc20x");

#[test]
fn erc20() {
    initialize_logger();

    let mut db = InMemoryDB::default();

    let alice: Address = address!("000000000000000000000000000000000000000A");
    let bob: Address = address!("000000000000000000000000000000000000000B");
    add_balance_to_db(&mut db, alice, 1e18 as u64);

    let bytecode_x = compile_with_prefix(compile_deploy, ERC20X_PATH).unwrap();
    let erc20x = deploy_contract(&mut db, bytecode_x, None).unwrap();

    let constructor = bob.abi_encode();
    let bytecode = compile_with_prefix(compile_deploy, ERC20_PATH).unwrap();
    let erc20 = deploy_contract(&mut db, bytecode, Some(constructor)).unwrap();

    let alice: Address = address!("000000000000000000000000000000000000000A");
    let value_mint = U256::from(42e18);

    let selector_x_mint = get_selector_from_sig("x_mint");
    let mut complete_calldata_x_mint = selector_x_mint.to_vec();
    let mut calldata_x_mint = (alice, value_mint, erc20).abi_encode();
    complete_calldata_x_mint.append(&mut calldata_x_mint);

    info!("----------------------------------------------------------");
    info!("-- X-MINT TX (ERRORS) ------------------------------------");
    info!("----------------------------------------------------------");
    debug!(
        "Tx Calldata:\n> {:#?}",
        Bytes::from(complete_calldata_x_mint.clone())
    );
    match run_tx(&mut db, &erc20x, complete_calldata_x_mint.clone()) {
        Ok(res) => info!("{}", res),
        Err(e) => {
            error!("Error when executing tx! {:#?}", e);
            panic!()
        }
    };
}
