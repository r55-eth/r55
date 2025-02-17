use alloy_primitives::{Bytes, U256};
use alloy_sol_types::SolValue;
use r55::{
    compile_deploy, compile_with_prefix,
    exec::{deploy_contract, run_tx},
    test_utils::{
        add_balance_to_db, get_selector_from_sig, initialize_logger, load_bytecode_from_file,
    },
};
use revm::{
    primitives::{address, Address},
    InMemoryDB,
};
use tracing::{debug, error, info};

const EVM_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/simple-evm-contract.txt");
const RISCV_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/evm-caller");

// ------------------------------------------------------------------------------------------------
//    SIMPLE EVM CONTRACT
// ------------------------------------------------------------------------------------------------
//    SPDX-License-Identifier: UNLICENSED
//    pragma solidity ^0.8.13;
//
//    struct callParams { address target; bytes data; }
//
//    contract SimpleStorage {
//        uint256 number;
//
//        function set(uint256 num) public { number = num; }
//        function get() public view returns (uint256) { return number; }
//        function rawCall(callParams calldata params) public returns (bool success) {
//            (success, ) = params.target.call(params.data);
//        }
//    }
// ------------------------------------------------------------------------------------------------

#[test]
fn evm_call() {
    initialize_logger();

    let mut db = InMemoryDB::default();

    let bytecode_evm = load_bytecode_from_file(EVM_PATH);
    let bytecode_r55 = compile_with_prefix(compile_deploy, RISCV_PATH).unwrap();
    let evm = deploy_contract(&mut db, bytecode_evm, None).unwrap();
    let r55 = deploy_contract(&mut db, bytecode_r55, None).unwrap();

    let selector_get = get_selector_from_sig("get()");
    let selector_set = get_selector_from_sig("set(uint256)");
    let selector_raw_call = get_selector_from_sig("rawCall((address,bytes))");
    let selector_x_get = get_selector_from_sig("x_get(address)");
    let selector_x_set = get_selector_from_sig("x_set(address,uint256)");

    let alice: Address = address!("000000000000000000000000000000000000000A");
    add_balance_to_db(&mut db, alice, 1e18 as u64);

    info!("----------------------------------------------------------");
    info!("-- SET VALUE TX (EVM CONTRACT) ---------------------------");
    info!("----------------------------------------------------------");
    let value_set = U256::from(1e18);
    let mut calldata_set = value_set.abi_encode();
    let mut complete_calldata_set = selector_set.to_vec();
    complete_calldata_set.append(&mut calldata_set);

    debug!(
        "Tx Calldata:\n> {:#?}",
        Bytes::from(complete_calldata_set.clone())
    );
    match run_tx(&mut db, &evm, complete_calldata_set.clone(), &alice) {
        Ok(res) => info!("{}", res),
        Err(e) => {
            error!("Error when executing tx! {:#?}", e);
            panic!()
        }
    };

    info!("----------------------------------------------------------");
    info!("-- X-GET VALUE TX (R55 CONTRACT -> EVM CONTRACT) ----------");
    info!("----------------------------------------------------------");
    // call traces: r55.x_get() -> evm.get()
    let mut calldata_x_get = evm.abi_encode();
    let mut complete_calldata_x_get = selector_x_get.to_vec();
    complete_calldata_x_get.append(&mut calldata_x_get);

    debug!(
        "Tx calldata:\n> {:#?}",
        Bytes::from(complete_calldata_x_get.clone())
    );
    match run_tx(&mut db, &r55, complete_calldata_x_get.clone(), &alice) {
        Ok(res) => {
            assert_eq!(
                U256::from_be_bytes::<32>(res.output.as_slice().try_into().unwrap()),
                value_set
            );
            info!("{}", res)
        }
        Err(e) => {
            error!("Error when executing tx! {:#?}", e);
            panic!();
        }
    }

    info!("----------------------------------------------------------");
    info!("-- X-SET VALUE TX (R55 CONTRACT -> EVM CONTRACT) ----------");
    info!("----------------------------------------------------------");
    // call traces: r55.x_set() -> evm.set()
    let value_x_set = U256::from(3e18);
    let mut calldata_x_set = (evm, value_x_set).abi_encode();
    let mut complete_calldata_x_set = selector_x_set.to_vec();
    complete_calldata_x_set.append(&mut calldata_x_set);

    debug!(
        "Tx calldata:\n> {:#?}",
        Bytes::from(complete_calldata_x_set.clone())
    );
    match run_tx(&mut db, &r55, complete_calldata_x_set.clone(), &alice) {
        Ok(res) => info!("{}", res),
        Err(e) => {
            error!("Error when executing tx! {:#?}", e);
            panic!();
        }
    }

    info!("----------------------------------------------------------");
    info!("-- GET VALUE TX (EVM CONTRACT) ---------------------------");
    info!("----------------------------------------------------------");
    debug!("Tx Calldata:\n> {:#?}", Bytes::from(selector_get.to_vec()));
    match run_tx(&mut db, &evm, selector_get.to_vec(), &alice) {
        Ok(res) => {
            assert_eq!(
                U256::from_be_bytes::<32>(res.output.as_slice().try_into().unwrap()),
                value_x_set
            );
            info!("{}", res)
        }
        Err(e) => {
            error!("Error when executing tx! {:#?}", e);
            panic!()
        }
    };

    info!("----------------------------------------------------------");
    info!("-- RAW-CALL TX (EVM -> R55 -> EVM ) ----------------------");
    info!("----------------------------------------------------------");
    // call traces: evm.rawCall() -> r55.x_set() -> evm.set()
    let value_raw_call_x_set = U256::from(5e18);
    let mut calldata_x_set = (evm, value_raw_call_x_set).abi_encode();
    let mut complete_calldata_x_set = selector_x_set.to_vec();
    complete_calldata_x_set.append(&mut calldata_x_set);
    debug!(
        "Tx sub-calldata:\n> {:#?}",
        Bytes::from(complete_calldata_x_set.clone())
    );

    let mut calldata_raw_call = (r55, Bytes::from(complete_calldata_x_set)).abi_encode();
    let mut complete_calldata_raw_call = selector_raw_call.to_vec();
    complete_calldata_raw_call.append(&mut calldata_raw_call);
    debug!(
        "Tx calldata:\n> {:#?}",
        Bytes::from(complete_calldata_raw_call.clone())
    );
    match run_tx(&mut db, &evm, complete_calldata_raw_call.clone(), &alice) {
        Ok(res) => {
            assert_eq!(
                U256::from_be_bytes::<32>(res.output.as_slice()[..32].try_into().unwrap()),
                U256::from(1)
            );
            info!("{}", res)
        }
        Err(e) => {
            error!("Error when executing tx! {:#?}", e);
            panic!();
        }
    }

    info!("----------------------------------------------------------");
    info!("-- GET VALUE TX (EVM CONTRACT) ---------------------------");
    info!("----------------------------------------------------------");
    debug!("Tx Calldata:\n> {:#?}", Bytes::from(selector_get.to_vec()));
    match run_tx(&mut db, &evm, selector_get.to_vec(), &alice) {
        Ok(res) => {
            assert_eq!(
                U256::from_be_bytes::<32>(res.output.as_slice().try_into().unwrap()),
                value_raw_call_x_set
            );
            info!("{}", res)
        }
        Err(e) => {
            error!("Error when executing tx! {:#?}", e);
            panic!()
        }
    };
}
