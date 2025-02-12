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
    add_balance_to_db(&mut db, alice, 1e18 as u64);

    let constructor = alice.abi_encode();

    let bytecode = compile_with_prefix(compile_deploy, ERC20_PATH).unwrap();
    let bytecode_x = compile_with_prefix(compile_deploy, ERC20X_PATH).unwrap();
    let erc20 = deploy_contract(&mut db, bytecode, Some(constructor)).unwrap();
    let erc20_x = deploy_contract(&mut db, bytecode_x, None).unwrap();

    let total_supply = get_selector_from_sig("total_supply");
    let selector_balance = get_selector_from_sig("balance_of");
    let selector_x_balance = get_selector_from_sig("x_balance_of");
    let selector_mint = get_selector_from_sig("mint");
    let selector_x_mint = get_selector_from_sig("x_mint");
    let alice: Address = address!("000000000000000000000000000000000000000A");
    let value_mint = U256::from(42e18);
    let mut calldata_balance = alice.abi_encode();
    let mut calldata_mint = (alice, value_mint).abi_encode();
    let mut calldata_x_balance = (alice, erc20).abi_encode();

    let mut complete_calldata_balance = selector_balance.to_vec();
    complete_calldata_balance.append(&mut calldata_balance);

    let mut complete_calldata_mint = selector_mint.to_vec();
    complete_calldata_mint.append(&mut calldata_mint);

    let mut complete_calldata_x_balance = selector_x_balance.to_vec();
    complete_calldata_x_balance.append(&mut calldata_x_balance);

    info!("----------------------------------------------------------");
    info!("-- X-MINT TX -----------------------------------------------");
    info!("----------------------------------------------------------");

    let mut complete_calldata_x_mint = selector_x_mint.to_vec();
    complete_calldata_x_mint.append(&mut (alice, U256::from(1e18), erc20).abi_encode());

    debug!(
        "Tx Calldata:\n> {:#?}",
        Bytes::from(complete_calldata_x_mint.clone())
    );
    match run_tx(&mut db, &erc20_x, complete_calldata_x_mint.clone()) {
        Ok(res) => info!("{}", res),
        Err(e) => {
            error!("Error when executing tx! {:#?}", e);
            panic!()
        }
    };

    info!("----------------------------------------------------------");
    info!("-- MINT TX -----------------------------------------------");
    info!("----------------------------------------------------------");
    debug!(
        "Tx Calldata:\n> {:#?}",
        Bytes::from(complete_calldata_mint.clone())
    );
    match run_tx(&mut db, &erc20, complete_calldata_mint.clone()) {
        Ok(res) => info!("{}", res),
        Err(e) => {
            error!("Error when executing tx! {:#?}", e);
            panic!()
        }
    };
    info!("----------------------------------------------------------");
    info!("-- TOTAL SUPPLY ------------------------------------------");
    info!("----------------------------------------------------------");
    debug!("Tx Calldata:\n> {:#?}", Bytes::from(total_supply.to_vec()));
    match run_tx(&mut db, &erc20, total_supply.to_vec()) {
        Ok(res) => info!("Success! {}", res),
        Err(e) => {
            error!("Error when executing tx! {:#?}", e);
            panic!()
        }
    };

    info!("----------------------------------------------------------");
    info!("-- BALANCE OF TX -----------------------------------------");
    info!("----------------------------------------------------------");
    debug!(
        "Tx Calldata:\n> {:#?}",
        Bytes::from(complete_calldata_balance.clone())
    );
    match run_tx(&mut db, &erc20, complete_calldata_balance.clone()) {
        Ok(res) => info!("{}", res),
        Err(e) => {
            error!("Error when executing tx! {:#?}", e);
            panic!()
        }
    };

    info!("----------------------------------------------------------");
    info!("-- X-CONTRACT BALANCE OF TX ------------------------------");
    info!("----------------------------------------------------------");
    debug!(
        "Tx calldata:\n> {:#?}",
        Bytes::from(complete_calldata_x_balance.clone())
    );
    match run_tx(&mut db, &erc20_x, complete_calldata_x_balance.clone()) {
        Ok(res) => info!("{}", res),
        Err(e) => {
            error!("Error when executing tx! {:#?}", e);
            panic!();
        }
    }
}
