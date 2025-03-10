use alloy_primitives::{address, Address, Bytes, U256};
use alloy_sol_types::SolValue;
use r55::{
    get_bytecode,
    exec::{deploy_contract, run_tx},
    test_utils::{add_balance_to_db, get_selector_from_sig, initialize_logger}, 
};
use revm::InMemoryDB;
use tracing::{debug, error, info};

#[test]
fn erc20() {
    initialize_logger();

    let mut db = InMemoryDB::default();

    let alice: Address = address!("000000000000000000000000000000000000000A");
    add_balance_to_db(&mut db, alice, 1e18 as u64);

    let constructor = alice.abi_encode();
    // let bytecode = compile_with_prefix(compile_deploy, ERC20_PATH).unwrap();
    let bytecode = get_bytecode("erc20");
    let erc20 = deploy_contract(&mut db, bytecode, Some(constructor)).unwrap();

    let total_supply = get_selector_from_sig("total_supply()");
    let selector_balance = get_selector_from_sig("balance_of(address)");
    let selector_mint = get_selector_from_sig("mint(address,uint256)");

    info!("----------------------------------------------------------");
    info!("-- MINT TX -----------------------------------------------");
    info!("----------------------------------------------------------");
    let value_mint = U256::from(42e18);
    let mut calldata_mint = (alice, value_mint).abi_encode();
    let mut complete_calldata_mint = selector_mint.to_vec();
    complete_calldata_mint.append(&mut calldata_mint);

    debug!(
        "Tx Calldata:\n> {:#?}",
        Bytes::from(complete_calldata_mint.clone())
    );
    match run_tx(&mut db, &erc20, complete_calldata_mint.clone(), &alice) {
        Ok(res) => info!("{}", res),
        Err(e) => {
            error!("Error when executing tx! {}", e);
            panic!()
        }
    };

    info!("----------------------------------------------------------");
    info!("-- TOTAL SUPPLY ------------------------------------------");
    info!("----------------------------------------------------------");
    debug!("Tx Calldata:\n> {:#?}", Bytes::from(total_supply.to_vec()));
    match run_tx(&mut db, &erc20, total_supply.to_vec(), &alice) {
        Ok(res) => info!("Success! {}", res),
        Err(e) => {
            error!("Error when executing tx! {}", e);
            panic!()
        }
    };

    info!("----------------------------------------------------------");
    info!("-- BALANCE OF TX -----------------------------------------");
    info!("----------------------------------------------------------");
    let mut calldata_balance = alice.abi_encode();
    let mut complete_calldata_balance = selector_balance.to_vec();
    complete_calldata_balance.append(&mut calldata_balance);

    debug!(
        "Tx Calldata:\n> {:#?}",
        Bytes::from(complete_calldata_balance.clone())
    );
    match run_tx(&mut db, &erc20, complete_calldata_balance.clone(), &alice) {
        Ok(res) => info!("{}", res),
        Err(e) => {
            error!("Error when executing tx! {}", e);
            panic!()
        }
    };
}

#[test]
fn erc20x() {
    initialize_logger();

    let mut db = InMemoryDB::default();

    let alice: Address = address!("000000000000000000000000000000000000000A");
    add_balance_to_db(&mut db, alice, 1e18 as u64);

    let bytecode_x = get_bytecode("erc20x");
    let erc20x = deploy_contract(&mut db, bytecode_x, None).unwrap();

    let constructor = erc20x.abi_encode();
    let bytecode = get_bytecode("erc20");
    let erc20 = deploy_contract(&mut db, bytecode, Some(constructor)).unwrap();

    let erc20_bytecode = get_selector_from_sig("erc20_bytecode()");
    let total_supply = get_selector_from_sig("total_supply()");
    let selector_x_balance = get_selector_from_sig("x_balance_of(address,address)");
    let selector_x_mint = get_selector_from_sig("x_mint(address,uint256,address)");

    info!("----------------------------------------------------------");
    info!("-- GET ERC20 BYTECODE ------------------------------------");
    info!("----------------------------------------------------------");
    match run_tx(&mut db, &erc20x, erc20_bytecode.to_vec(), &alice) {
        Ok(res) => info!("Success! {}", res),
        Err(e) => {
            error!("Error when executing tx! {}", e);
            panic!()
        }
    };

    info!("----------------------------------------------------------");
    info!("-- X-MINT TX -----------------------------------------------");
    info!("----------------------------------------------------------");
    let value_x_mint = U256::from(42e18);
    let mut complete_calldata_x_mint = selector_x_mint.to_vec();
    complete_calldata_x_mint.append(&mut (alice, value_x_mint, erc20).abi_encode());

    debug!(
        "Tx Calldata:\n> {:#?}",
        Bytes::from(complete_calldata_x_mint.clone())
    );
    match run_tx(&mut db, &erc20x, complete_calldata_x_mint.clone(), &alice) {
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
    match run_tx(&mut db, &erc20, total_supply.to_vec(), &alice) {
        Ok(res) => info!("Success! {}", res),
        Err(e) => {
            error!("Error when executing tx! {:#?}", e);
            panic!()
        }
    };

    info!("----------------------------------------------------------");
    info!("-- X-CONTRACT BALANCE OF TX ------------------------------");
    info!("----------------------------------------------------------");
    let mut calldata_x_balance = (alice, erc20).abi_encode();
    let mut complete_calldata_x_balance = selector_x_balance.to_vec();
    complete_calldata_x_balance.append(&mut calldata_x_balance);

    debug!(
        "Tx calldata:\n> {:#?}",
        Bytes::from(complete_calldata_x_balance.clone())
    );
    match run_tx(
        &mut db,
        &erc20x,
        complete_calldata_x_balance.clone(),
        &alice,
    ) {
        Ok(res) => info!("{}", res),
        Err(e) => {
            error!("Error when executing tx! {}", e);
            panic!();
        }
    }
}
