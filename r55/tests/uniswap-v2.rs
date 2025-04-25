use alloy_primitives::{address, Address, B256};
use alloy_sol_types::SolValue;
use r55::{
    exec::{deploy_contract, run_tx},
    get_bytecode,
    test_utils::{
        add_balance_to_db, get_calldata, get_selector_from_sig, initialize_logger, ALICE, BOB,
        CAROL,
    },
};
use revm::InMemoryDB;

const WETH: Address = address!("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2");
const USDC: Address = address!("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48");

#[cfg(test)]
mod uniswap_v2_factory_tests {
    use super::*;

    struct UniswapV2FactorySetup {
        db: InMemoryDB,
        factory: Address,
        fee_to_setter: Address,
    }

    fn factory_setup(fee_to_setter: Address) -> UniswapV2FactorySetup {
        initialize_logger();
        let mut db = InMemoryDB::default();

        // Fund user accounts with some ETH
        for user in [ALICE, BOB, CAROL] {
            add_balance_to_db(&mut db, user, 1e18 as u64);
        }

        // Deploy factory contract
        let constructor = fee_to_setter.abi_encode();
        let bytecode = get_bytecode("uniswap_v2_factory");
        let factory = deploy_contract(&mut db, bytecode, Some(constructor)).unwrap();

        UniswapV2FactorySetup {
            db,
            factory,
            fee_to_setter,
        }
    }

    #[test]
    fn test_factory_initialization() {
        let UniswapV2FactorySetup {
            mut db,
            factory,
            fee_to_setter,
        } = factory_setup(ALICE);

        // Check `fee_to`
        let selector_fee_to_setter = get_selector_from_sig("fee_to_setter()");
        let fee_to_result = run_tx(&mut db, &factory, selector_fee_to_setter.to_vec(), &ALICE)
            .expect("Error executing tx")
            .output;

        assert_eq!(
            Address::from_word(B256::from_slice(fee_to_result.as_slice())),
            fee_to_setter,
            "Initial `fee_to_setter` should be ALICE"
        );

        // Check `fee_to_setter`
        let selector_fee_to_setter = get_selector_from_sig("fee_to_setter()");
        let fee_to_setter_result =
            run_tx(&mut db, &factory, selector_fee_to_setter.to_vec(), &ALICE)
                .expect("Error executing tx")
                .output;

        assert_eq!(
            Address::from_word(B256::from_slice(fee_to_setter_result.as_slice())),
            fee_to_setter,
            "Initial `fee_to_setter` should be ALICE"
        );
    }

    fn create_pair_and_validate(token0: Address, token1: Address, pass_flipped: bool) {
        let UniswapV2FactorySetup {
            mut db,
            factory,
            fee_to_setter: _,
        } = factory_setup(ALICE);

        // Call `create_pair`
        let selector_create_pair = get_selector_from_sig("create_pair(address,address)");
        let calldata_create_pair = get_calldata(
            selector_create_pair,
            if pass_flipped {
                (token1, token0).abi_encode()
            } else {
                (token0, token1).abi_encode()
            },
        );

        run_tx(&mut db, &factory, calldata_create_pair, &ALICE)
            .expect("Create pair transaction failed");

        // Verify pair was created by checking getPair
        let selector_get_pair = get_selector_from_sig("pair(address,address)");

        let calldata_get_pair = get_calldata(selector_get_pair, (token0, token1).abi_encode());
        let pair_result = run_tx(&mut db, &factory, calldata_get_pair, &ALICE)
            .expect("Error executing tx")
            .output;

        // The pair address should not be zero
        let pair = Address::from_word(B256::from_slice(pair_result.as_slice()));
        assert_ne!(pair, Address::ZERO, "Pair address should not be zero");

        // Call `token0` and `token1` on the pair contract
        let selector_token0 = get_selector_from_sig("token0()");
        let selector_token1 = get_selector_from_sig("token1()");
        let token0_result = run_tx(&mut db, &pair, selector_token0.to_vec(), &ALICE)
            .expect("Error executing tx")
            .output;
        let token1_result = run_tx(&mut db, &pair, selector_token1.to_vec(), &ALICE)
            .expect("Error executing tx")
            .output;

        // Validate tokens
        assert_eq!(
            Address::from_word(B256::from_slice(token0_result.as_slice())),
            token0,
            "Unexpected token0 address"
        );
        assert_eq!(
            Address::from_word(B256::from_slice(token1_result.as_slice())),
            token1,
            "Unexpected token1 address"
        );
    }

    #[test]
    fn test_create_pair_works() {
        let token0 = USDC;
        let token1 = WETH;

        // test in correct order (token0 < token1)
        create_pair_and_validate(token0, token1, false);

        // test in wrong order
        create_pair_and_validate(token0, token1, true);
    }

    #[test]
    fn test_create_pair_identical_addresses() {
        let UniswapV2FactorySetup {
            mut db,
            factory,
            fee_to_setter: _,
        } = factory_setup(ALICE);

        let token = WETH;

        // Try to create pair with identical addresses
        let selector_create_pair = get_selector_from_sig("create_pair(address,address)");
        let calldata_create_pair = get_calldata(selector_create_pair, (token, token).abi_encode());

        let result = run_tx(&mut db, &factory, calldata_create_pair, &ALICE)
            .expect_err("Create pair with identical addresses should fail");

        assert!(
            result.matches_custom_error("UniswapV2FactoryError::SameToken"),
            "Incorrect error signature for identical addresses"
        );
    }

    #[test]
    fn test_create_pair_zero_address() {
        let UniswapV2FactorySetup {
            mut db,
            factory,
            fee_to_setter: _,
        } = factory_setup(ALICE);

        let token = WETH;

        // Try to create pair with zero address
        let selector_create_pair = get_selector_from_sig("create_pair(address,address)");
        let calldata_create_pair =
            get_calldata(selector_create_pair, (token, Address::ZERO).abi_encode());

        let result = run_tx(&mut db, &factory, calldata_create_pair, &ALICE)
            .expect_err("Create pair with zero address should fail");

        assert!(
            result.matches_custom_error("UniswapV2FactoryError::ZeroAddress"),
            "Incorrect error signature for zero address"
        );
    }

    #[test]
    fn test_create_pair_pair_exists() {
        let UniswapV2FactorySetup {
            mut db,
            factory,
            fee_to_setter: _,
        } = factory_setup(ALICE);

        let token0 = USDC;
        let token1 = WETH;

        // Create pair first time
        let selector_create_pair = get_selector_from_sig("create_pair(address,address)");
        let calldata_create_pair =
            get_calldata(selector_create_pair, (token0, token1).abi_encode());

        run_tx(&mut db, &factory, calldata_create_pair.clone(), &ALICE)
            .expect("Error executing first create_pair tx");

        // Try to create the same pair again
        let result = run_tx(&mut db, &factory, calldata_create_pair, &ALICE)
            .expect_err("Create pair should fail when pair already exists");

        assert!(
            result.matches_custom_error("UniswapV2FactoryError::PairExists"),
            "Incorrect error signature for pair exists"
        );
    }

    #[test]
    fn test_set_fee_to() {
        let UniswapV2FactorySetup {
            mut db,
            factory,
            fee_to_setter,
        } = factory_setup(ALICE);

        let new_fee_to = BOB;

        // Set feeTo
        let selector_set_fee_to = get_selector_from_sig("set_fee_to(address)");
        let calldata_set_fee_to = get_calldata(selector_set_fee_to, new_fee_to.abi_encode());

        let set_fee_to_result = run_tx(&mut db, &factory, calldata_set_fee_to, &fee_to_setter)
            .expect("Error executing set_fee_to tx");

        assert!(set_fee_to_result.status, "Set `fee_to` transaction failed");

        // Verify feeTo was updated
        let selector_fee_to = get_selector_from_sig("fee_to()");
        let fee_to_result = run_tx(&mut db, &factory, selector_fee_to.to_vec(), &fee_to_setter)
            .expect("Error executing tx")
            .output;

        assert_eq!(
            Address::from_word(B256::from_slice(fee_to_result.as_slice())),
            new_fee_to,
            "feeTo was not updated correctly"
        );
    }

    #[test]
    fn test_set_fee_to_unauthorized() {
        let UniswapV2FactorySetup {
            mut db,
            factory,
            fee_to_setter: _,
        } = factory_setup(ALICE);

        let unauthorized = BOB;
        let new_fee_to = CAROL;

        // Try to set feeTo with unauthorized account
        let selector_set_fee_to = get_selector_from_sig("set_fee_to(address)");
        let calldata_set_fee_to = get_calldata(selector_set_fee_to, new_fee_to.abi_encode());

        let result = run_tx(&mut db, &factory, calldata_set_fee_to, &unauthorized)
            .expect_err("Unauthorized set_fee_to should fail");

        assert!(
            result.matches_custom_error("UniswapV2FactoryError::Unauthorized"),
            "Incorrect error signature for unauthorized set_fee_to"
        );
    }

    #[test]
    fn test_set_fee_to_setter() {
        let UniswapV2FactorySetup {
            mut db,
            factory,
            fee_to_setter,
        } = factory_setup(ALICE);

        let new_fee_to_setter = BOB;

        // Set feeToSetter
        let selector_set_fee_to_setter = get_selector_from_sig("set_fee_to_setter(address)");
        let calldata_set_fee_to_setter =
            get_calldata(selector_set_fee_to_setter, new_fee_to_setter.abi_encode());

        let set_fee_to_setter_result = run_tx(
            &mut db,
            &factory,
            calldata_set_fee_to_setter,
            &fee_to_setter,
        )
        .expect("Error executing set_fee_to_setter tx");

        assert!(
            set_fee_to_setter_result.status,
            "Set feeToSetter transaction failed"
        );

        // Verify feeToSetter was updated
        let selector_fee_to_setter = get_selector_from_sig("fee_to_setter()");
        let fee_to_setter_result =
            run_tx(&mut db, &factory, selector_fee_to_setter.to_vec(), &ALICE)
                .expect("Error executing tx")
                .output;

        assert_eq!(
            Address::from_slice(&fee_to_setter_result[12..32]),
            new_fee_to_setter,
            "feeToSetter was not updated correctly"
        );
    }

    #[test]
    fn test_set_fee_to_setter_unauthorized() {
        let UniswapV2FactorySetup {
            mut db,
            factory,
            fee_to_setter: _,
        } = factory_setup(ALICE);

        let unauthorized = BOB;
        let new_fee_to_setter = CAROL;

        // Try to set feeToSetter with unauthorized account
        let selector_set_fee_to_setter = get_selector_from_sig("set_fee_to_setter(address)");
        let calldata_set_fee_to_setter =
            get_calldata(selector_set_fee_to_setter, new_fee_to_setter.abi_encode());

        let result = run_tx(&mut db, &factory, calldata_set_fee_to_setter, &unauthorized)
            .expect_err("Unauthorized set_fee_to_setter should fail");

        assert!(
            result.matches_custom_error("UniswapV2FactoryError::Unauthorized"),
            "Incorrect error signature for unauthorized set_fee_to_setter"
        );
    }

    #[test]
    fn test_set_fee_to_setter_to_other_then_try_again() {
        let UniswapV2FactorySetup {
            mut db,
            factory,
            fee_to_setter,
        } = factory_setup(ALICE);

        let new_fee_to = BOB;
        let new_fee_to_setter = CAROL;

        // First set feeTo to a new address
        let selector_set_fee_to = get_selector_from_sig("set_fee_to(address)");
        let calldata_set_fee_to = get_calldata(selector_set_fee_to, new_fee_to.abi_encode());

        run_tx(&mut db, &factory, calldata_set_fee_to, &fee_to_setter)
            .expect("Error executing set_fee_to tx");

        // Then set feeToSetter to a new address
        let selector_set_fee_to_setter = get_selector_from_sig("set_fee_to_setter(address)");
        let calldata_set_fee_to_setter =
            get_calldata(selector_set_fee_to_setter, new_fee_to_setter.abi_encode());

        run_tx(
            &mut db,
            &factory,
            calldata_set_fee_to_setter,
            &fee_to_setter,
        )
        .expect("Error executing set_fee_to_setter tx");

        // Try to set feeToSetter again with the original feeToSetter (should fail)
        let calldata_set_fee_to_setter_again =
            get_calldata(selector_set_fee_to_setter, BOB.abi_encode());

        let result = run_tx(
            &mut db,
            &factory,
            calldata_set_fee_to_setter_again,
            &fee_to_setter,
        )
        .expect_err("set_fee_to_setter from original account should fail after transfer");

        assert!(
            result.matches_custom_error("UniswapV2FactoryError::Unauthorized"),
            "Incorrect error signature for unauthorized set_fee_to_setter after transfer"
        );
    }
}
