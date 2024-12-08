use alloy_sol_types::SolValue;
use r55::{
    compile_deploy, compile_with_prefix,
    exec::{deploy_contract, run_tx},
    test_utils::{add_balance_to_db, get_selector_from_sig, initialize_logger},
};
use revm::{
    primitives::{address, Address},
    InMemoryDB,
};

const ERC20_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../erc20");

#[test]
fn erc20() {
    initialize_logger();

    let mut db = InMemoryDB::default();

    let bytecode = compile_with_prefix(compile_deploy, ERC20_PATH).unwrap();
    let addr = deploy_contract(&mut db, bytecode).unwrap();

    let selector_balance = get_selector_from_sig("balance_of");
    let selector_mint = get_selector_from_sig("mint");
    let alice: Address = address!("0000000000000000000000000000000000000001");
    let value_mint: u64 = 42;
    let mut calldata_balance = alice.abi_encode();
    let mut calldata_mint = (alice, value_mint).abi_encode();

    add_balance_to_db(&mut db, alice, 1e18 as u64);

    let mut complete_calldata_balance = selector_balance.to_vec();
    complete_calldata_balance.append(&mut calldata_balance);

    let mut complete_calldata_mint = selector_mint.to_vec();
    complete_calldata_mint.append(&mut calldata_mint);

    run_tx(&mut db, &addr, complete_calldata_mint.clone()).unwrap();
    run_tx(&mut db, &addr, complete_calldata_balance.clone()).unwrap();
}
