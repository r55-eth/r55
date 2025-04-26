use alloy_primitives::{address, Address, Bytes, B256, U256};
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
use tracing::{error, warn};

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

#[cfg(test)]
mod uniswap_v2_pair_tests {
    use super::*;

    struct UniswapV2PairSetup {
        db: InMemoryDB,
        factory: Address,
        token0: Address,
        token1: Address,
        pair: Address,
        fee_to_setter: Address,
    }

    const MINIMUM_LIQUIDITY: U256 = U256::from_limbs([1000, 0, 0, 0]);

    fn pair_setup() -> UniswapV2PairSetup {
        initialize_logger();
        let mut db = InMemoryDB::default();

        // Fund user accounts with some ETH
        for user in [ALICE, BOB, CAROL] {
            add_balance_to_db(&mut db, user, 1e18 as u64);
        }

        // Deploy ERC20 test tokens
        let token_a =
            deploy_contract(&mut db, get_bytecode("erc20"), Some(ALICE.abi_encode())).unwrap();
        let token_b =
            deploy_contract(&mut db, get_bytecode("erc20"), Some(ALICE.abi_encode())).unwrap();

        // Sort tokens by address like uniswap-v2
        let (token0, token1) = if token_a < token_b {
            (token_a, token_b)
        } else {
            (token_b, token_a)
        };

        // Deploy factory
        let factory_bytecode = get_bytecode("uniswap_v2_factory");
        let factory = deploy_contract(&mut db, factory_bytecode, Some(ALICE.abi_encode())).unwrap();

        // Create pair via factory
        let selector_create_pair = get_selector_from_sig("create_pair(address,address)");
        let calldata_create_pair =
            get_calldata(selector_create_pair, (token0, token1).abi_encode());
        let new_pair_result =
            run_tx(&mut db, &factory, calldata_create_pair, &ALICE).expect("Error executing tx");
        assert!(new_pair_result.status, "Failed to create new pair");
        let pair = Address::from_word(B256::from_slice(new_pair_result.output.as_slice()));

        UniswapV2PairSetup {
            db,
            factory,
            token0,
            token1,
            pair,
            fee_to_setter: ALICE,
        }
    }

    fn erc20_mint(db: &mut InMemoryDB, token: Address, to: Address, amount: U256) {
        let selector_mint = get_selector_from_sig("mint(address,uint256)");
        let calldata_mint = get_calldata(selector_mint, (to, amount).abi_encode());
        let result = run_tx(db, &token, calldata_mint, &ALICE).expect("Error executing mint tx");
        assert!(result.status, "Mint transaction failed");
    }

    fn erc20_transfer(db: &mut InMemoryDB, token: Address, to: Address, amount: U256) {
        let selector_transfer = get_selector_from_sig("transfer(address,uint256)");
        let calldata_transfer = get_calldata(selector_transfer, (to, amount).abi_encode());
        let result =
            run_tx(db, &token, calldata_transfer, &ALICE).expect("Error executing transfer tx");
        assert!(result.status, "Transfer transaction failed");
    }

    fn erc20_balance(db: &mut InMemoryDB, token: Address, owner: Address) -> U256 {
        let selector_balance_of = get_selector_from_sig("balance_of(address)");
        let calldata_balance_of = get_calldata(selector_balance_of, owner.abi_encode());
        let balance_result = run_tx(db, &token, calldata_balance_of, &ALICE)
            .expect("Error executing balance_of tx")
            .output;
        B256::from_slice(&balance_result).into()
    }

    fn get_reserves(db: &mut InMemoryDB, pair: Address) -> (U256, U256, U256) {
        let selector_get_reserves = get_selector_from_sig("get_reserves()");
        let reserves_result = run_tx(db, &pair, selector_get_reserves.to_vec(), &ALICE)
            .expect("Error executing get_reserves tx")
            .output;

        // Extract three U256 values from the output
        let reserve0: U256 = B256::from_slice(&reserves_result[0..32]).into();
        let reserve1: U256 = B256::from_slice(&reserves_result[32..64]).into();
        let last_block_at: U256 = B256::from_slice(&reserves_result[64..96]).into();

        (reserve0, reserve1, last_block_at)
    }

    fn get_total_supply(db: &mut InMemoryDB, pair: Address) -> U256 {
        let selector_total_supply = get_selector_from_sig("total_supply()");
        let total_supply_result = run_tx(db, &pair, selector_total_supply.to_vec(), &ALICE)
            .expect("Error executing mint tx");
        B256::from_slice(&total_supply_result.output).into()
    }

    fn add_liquidity(
        db: &mut InMemoryDB,
        pair: Address,
        token0: Address,
        token1: Address,
        token0_amount: U256,
        token1_amount: U256,
    ) -> U256 {
        // Transfer tokens to the pair contract
        erc20_transfer(db, token0, pair, token0_amount);
        erc20_transfer(db, token1, pair, token1_amount);

        // Call mint
        let selector_mint = get_selector_from_sig("mint(address)");
        let calldata_mint = get_calldata(selector_mint, ALICE.abi_encode());
        let mint_result =
            run_tx(db, &pair, calldata_mint, &ALICE).expect("Error executing mint tx");
        assert!(mint_result.status, "Mint transaction failed");

        B256::from_slice(&mint_result.output).into()
    }

    #[test]
    fn test_mint() {
        let UniswapV2PairSetup {
            mut db,
            factory: _,
            token0,
            token1,
            pair,
            fee_to_setter: _,
        } = pair_setup();

        // Mint tokens to ALICE
        let token0_amount = U256::from(1e18);
        let token1_amount = U256::from(4e18);
        erc20_mint(&mut db, token0, ALICE, token0_amount);
        erc20_mint(&mut db, token1, ALICE, token1_amount);

        // Transfer tokens to pair
        warn!("TRANSFER");
        erc20_transfer(&mut db, token0, pair, token0_amount);
        erc20_transfer(&mut db, token1, pair, token1_amount);

        // Call mint function
        let expected_liquidity = U256::from(2e18);
        let selector_mint = get_selector_from_sig("mint(address)");
        let calldata_mint = get_calldata(selector_mint, ALICE.abi_encode());

        let mint_result =
            run_tx(&mut db, &pair, calldata_mint, &ALICE).expect("Error executing mint tx");
        assert!(mint_result.status, "Mint transaction failed");

        // Verify outputs
        let total_supply = get_total_supply(&mut db, pair);
        let alice_balance = erc20_balance(&mut db, pair, ALICE);
        let (reserve0, reserve1, _) = get_reserves(&mut db, pair);

        assert_eq!(total_supply, expected_liquidity, "Incorrect total supply");
        assert_eq!(
            alice_balance,
            expected_liquidity - MINIMUM_LIQUIDITY,
            "Incorrect ALICE balance"
        );
        assert_eq!(reserve0, token0_amount, "Incorrect reserve0");
        assert_eq!(reserve1, token1_amount, "Incorrect reserve1");
        assert_eq!(
            erc20_balance(&mut db, token0, pair),
            token0_amount,
            "Incorrect token0 balance"
        );
        assert_eq!(
            erc20_balance(&mut db, token1, pair),
            token1_amount,
            "Incorrect token1 balance"
        );
    }

    #[test]
    fn test_swap_token0() {
        let UniswapV2PairSetup {
            mut db,
            factory: _,
            token0,
            token1,
            pair,
            fee_to_setter: _,
        } = pair_setup();

        // Setup initial liquidity
        let token0_amount = U256::from(5e18);
        let token1_amount = U256::from(10e18);

        erc20_mint(&mut db, token0, ALICE, token0_amount);
        erc20_mint(&mut db, token1, ALICE, token1_amount);
        add_liquidity(&mut db, pair, token0, token1, token0_amount, token1_amount);

        // Prepare swap
        let swap_amount = U256::from(1e18);
        let expected_output_amount = U256::from(1662497915624478906_u128);

        erc20_mint(&mut db, token0, ALICE, swap_amount);
        erc20_transfer(&mut db, token0, pair, swap_amount);

        // Execute swap
        let selector_swap = get_selector_from_sig("swap(uint256,uint256,address,bytes)");
        let calldata_swap = get_calldata(
            selector_swap,
            (U256::ZERO, expected_output_amount, ALICE, Bytes::default()).abi_encode(),
        );

        let swap_result =
            run_tx(&mut db, &pair, calldata_swap, &ALICE).expect("Error executing swap tx");
        assert!(swap_result.status, "Swap transaction failed");

        // Verify state after swap
        let (reserve0, reserve1, _) = get_reserves(&mut db, pair);

        assert_eq!(
            reserve0,
            token0_amount + swap_amount,
            "Incorrect reserve0 after swap"
        );
        assert_eq!(
            reserve1,
            token1_amount - expected_output_amount,
            "Incorrect reserve1 after swap"
        );

        // Check token balances
        assert_eq!(
            erc20_balance(&mut db, token0, pair),
            token0_amount + swap_amount,
            "Incorrect token0 balance after swap"
        );
        assert_eq!(
            erc20_balance(&mut db, token1, pair),
            token1_amount - expected_output_amount,
            "Incorrect token1 balance after swap"
        );
    }

    #[test]
    fn test_swap_token1() {
        let UniswapV2PairSetup {
            mut db,
            factory: _,
            token0,
            token1,
            pair,
            fee_to_setter: _,
        } = pair_setup();

        // Setup initial liquidity
        let token0_amount = U256::from(5e18);
        let token1_amount = U256::from(10e18);

        erc20_mint(&mut db, token0, ALICE, token0_amount);
        erc20_mint(&mut db, token1, ALICE, token1_amount);
        add_liquidity(&mut db, pair, token0, token1, token0_amount, token1_amount);

        // Prepare swap
        let swap_amount = U256::from(1e18);
        let expected_output_amount = U256::from(453305446940074565_u128);

        erc20_mint(&mut db, token1, ALICE, swap_amount);
        erc20_transfer(&mut db, token1, pair, swap_amount);

        // Execute swap
        let selector_swap = get_selector_from_sig("swap(uint256,uint256,address,bytes)");
        let calldata_swap = get_calldata(
            selector_swap,
            (expected_output_amount, U256::ZERO, ALICE, Bytes::new()).abi_encode(),
        );

        let swap_result =
            run_tx(&mut db, &pair, calldata_swap, &ALICE).expect("Error executing swap tx");
        assert!(swap_result.status, "Swap transaction failed");

        // Verify state after swap
        let (reserve0, reserve1, _) = get_reserves(&mut db, pair);

        assert_eq!(
            reserve0,
            token0_amount - expected_output_amount,
            "Incorrect reserve0 after swap"
        );
        assert_eq!(
            reserve1,
            token1_amount + swap_amount,
            "Incorrect reserve1 after swap"
        );

        // Check token balances
        assert_eq!(
            erc20_balance(&mut db, token0, pair),
            token0_amount - expected_output_amount,
            "Incorrect token0 balance after swap"
        );
        assert_eq!(
            erc20_balance(&mut db, token1, pair),
            token1_amount + swap_amount,
            "Incorrect token1 balance after swap"
        );
    }

    #[test]
    fn test_burn() {
        let UniswapV2PairSetup {
            mut db,
            factory: _,
            token0,
            token1,
            pair,
            fee_to_setter: _,
        } = pair_setup();

        // Setup initial liquidity
        let token0_amount = U256::from(3e18);
        let token1_amount = U256::from(3e18);

        erc20_mint(&mut db, token0, ALICE, token0_amount);
        erc20_mint(&mut db, token1, ALICE, token1_amount);
        let alice_liquidity =
            add_liquidity(&mut db, pair, token0, token1, token0_amount, token1_amount);
        let total_supply = get_total_supply(&mut db, pair);

        assert_eq!(
            alice_liquidity,
            total_supply - MINIMUM_LIQUIDITY,
            "Incorrect liquidity"
        );

        // Record initial token balances of ALICE
        let alice_token0_balance_before = erc20_balance(&mut db, token0, ALICE);
        let alice_token1_balance_before = erc20_balance(&mut db, token1, ALICE);

        // Transfer LP tokens to pair for burning
        erc20_transfer(&mut db, pair, pair, alice_liquidity);

        // Execute burn
        let selector_burn = get_selector_from_sig("burn(address)");
        let calldata_burn = get_calldata(selector_burn, ALICE.abi_encode());

        let burn_result =
            run_tx(&mut db, &pair, calldata_burn, &ALICE).expect("Error executing burn tx");
        assert!(burn_result.status, "Burn transaction failed");

        // Verify state after burn
        let total_supply = get_total_supply(&mut db, pair);
        let (reserve0, reserve1, _) = get_reserves(&mut db, pair);

        // Validate state
        assert_eq!(
            total_supply, MINIMUM_LIQUIDITY,
            "Total supply should equal MINIMUM_LIQUIDITY"
        );
        assert_eq!(reserve0, U256::from(1000), "Incorrect reserve0 after burn");
        assert_eq!(reserve1, U256::from(1000), "Incorrect reserve1 after burn");

        // Check token balances of pair and ALICE
        assert_eq!(
            erc20_balance(&mut db, token0, pair),
            U256::from(1000),
            "Incorrect token0 balance in pair"
        );
        assert_eq!(
            erc20_balance(&mut db, token1, pair),
            U256::from(1000),
            "Incorrect token1 balance in pair"
        );

        // Alice's balance should have increased by almost the full amount (minus MINIMUM_LIQUIDITY)
        let alice_token0_balance_after = erc20_balance(&mut db, token0, ALICE);
        let alice_token1_balance_after = erc20_balance(&mut db, token1, ALICE);

        assert_eq!(
            alice_token0_balance_after - alice_token0_balance_before,
            token0_amount - U256::from(1000),
            "ALICE didn't receive expected token0 amount"
        );
        assert_eq!(
            alice_token1_balance_after - alice_token1_balance_before,
            token1_amount - U256::from(1000),
            "ALICE didn't receive expected token1 amount"
        );
    }

    #[test]
    fn test_swap_insufficient_output_amount() {
        let UniswapV2PairSetup {
            mut db,
            factory: _,
            token0,
            token1,
            pair,
            fee_to_setter: _,
        } = pair_setup();

        // Setup initial liquidity
        let token0_amount = U256::from(5e18);
        let token1_amount = U256::from(10e18);

        erc20_mint(&mut db, token0, ALICE, token0_amount);
        erc20_mint(&mut db, token1, ALICE, token1_amount);
        add_liquidity(&mut db, pair, token0, token1, token0_amount, token1_amount);

        // Execute swap with both amounts being zero
        let selector_swap = get_selector_from_sig("swap(uint256,uint256,address,bytes)");
        let calldata_swap = get_calldata(
            selector_swap,
            (U256::ZERO, U256::ZERO, ALICE, Bytes::new()).abi_encode(),
        );

        let result = run_tx(&mut db, &pair, calldata_swap, &ALICE)
            .expect_err("Swap with zero outputs should fail");

        assert!(
            result.matches_custom_error("UniswapV2PairError::InsufficientOutputAmount"),
            "Incorrect error for insufficient output amount"
        );
    }

    #[test]
    fn test_swap_exceeds_reserves() {
        let UniswapV2PairSetup {
            mut db,
            factory: _,
            token0,
            token1,
            pair,
            fee_to_setter: _,
        } = pair_setup();

        // Setup initial liquidity
        let token0_amount = U256::from(5e18);
        let token1_amount = U256::from(10e18);

        erc20_mint(&mut db, token0, ALICE, token0_amount);
        erc20_mint(&mut db, token1, ALICE, token1_amount);
        add_liquidity(&mut db, pair, token0, token1, token0_amount, token1_amount);

        // Try to swap for more than reserves
        let selector_swap = get_selector_from_sig("swap(uint256,uint256,address,bytes)");
        let calldata_swap = get_calldata(
            selector_swap,
            (
                token0_amount + U256::from(1),
                U256::ZERO,
                ALICE,
                Bytes::new(),
            )
                .abi_encode(),
        );

        let result = run_tx(&mut db, &pair, calldata_swap, &ALICE)
            .expect_err("Swap exceeding reserves should fail");

        assert!(
            result.matches_custom_error("UniswapV2PairError::InsufficientLiquidity"),
            "Incorrect error for insufficient liquidity"
        );
    }

    #[test]
    fn test_swap_invalid_to() {
        let UniswapV2PairSetup {
            mut db,
            factory: _,
            token0,
            token1,
            pair,
            fee_to_setter: _,
        } = pair_setup();

        // Setup initial liquidity
        let token0_amount = U256::from(5e18);
        let token1_amount = U256::from(10e18);

        erc20_mint(&mut db, token0, ALICE, token0_amount);
        erc20_mint(&mut db, token1, ALICE, token1_amount);
        add_liquidity(&mut db, pair, token0, token1, token0_amount, token1_amount);

        // Prepare swap with invalid destination (token address)
        let swap_amount = U256::from(1e18);
        let expected_output_amount = U256::from(1662497915624478906_u128);

        erc20_mint(&mut db, token0, ALICE, swap_amount);
        erc20_transfer(&mut db, token0, pair, swap_amount);

        // Try swap with token0 as destination
        let selector_swap = get_selector_from_sig("swap(uint256,uint256,address,bytes)");
        let calldata_swap = get_calldata(
            selector_swap,
            (U256::ZERO, expected_output_amount, token0, Bytes::new()).abi_encode(),
        );

        let result = run_tx(&mut db, &pair, calldata_swap, &ALICE)
            .expect_err("Swap to token address should fail");

        assert!(
            result.matches_custom_error("UniswapV2PairError::InvalidTo"),
            "Incorrect error for invalid to address"
        );
    }

    #[test]
    fn test_swap_k_invariant() {
        let UniswapV2PairSetup {
            mut db,
            factory: _,
            token0,
            token1,
            pair,
            fee_to_setter: _,
        } = pair_setup();

        // Setup initial liquidity
        let token0_amount = U256::from(5e18);
        let token1_amount = U256::from(10e18);

        erc20_mint(&mut db, token0, ALICE, token0_amount);
        erc20_mint(&mut db, token1, ALICE, token1_amount);
        add_liquidity(&mut db, pair, token0, token1, token0_amount, token1_amount);

        // Prepare swap
        let swap_amount = U256::from(1e18);
        // Try an output amount that's too high (breaking K invariant)
        let too_high_output = U256::from(1662497915624478906_u128) + U256::from(1);

        erc20_mint(&mut db, token0, ALICE, swap_amount);
        erc20_transfer(&mut db, token0, pair, swap_amount);

        // Execute swap with too high output
        let selector_swap = get_selector_from_sig("swap(uint256,uint256,address,bytes)");
        let calldata_swap = get_calldata(
            selector_swap,
            (U256::ZERO, too_high_output, ALICE, Bytes::new()).abi_encode(),
        );

        let result = run_tx(&mut db, &pair, calldata_swap, &ALICE)
            .expect_err("Swap breaking K invariant should fail");

        assert!(
            result.matches_custom_error("UniswapV2PairError::K"),
            "Incorrect error for K invariant violation"
        );
    }

    #[test]
    fn test_fee_to_off() {
        let UniswapV2PairSetup {
            mut db,
            factory: _,
            token0,
            token1,
            pair,
            fee_to_setter: _,
        } = pair_setup();

        // Setup initial liquidity
        let token0_amount = U256::from(1000e18);
        let token1_amount = U256::from(1000e18);

        erc20_mint(&mut db, token0, ALICE, token0_amount);
        erc20_mint(&mut db, token1, ALICE, token1_amount);
        add_liquidity(&mut db, pair, token0, token1, token0_amount, token1_amount);

        // Perform a swap
        let swap_amount = U256::from(1e18);
        let expected_output_amount = U256::from(996006981039903216_u128);

        erc20_mint(&mut db, token1, ALICE, swap_amount);
        erc20_transfer(&mut db, token1, pair, swap_amount);

        // Execute swap
        let selector_swap = get_selector_from_sig("swap(uint256,uint256,address,bytes)");
        let calldata_swap = get_calldata(
            selector_swap,
            (expected_output_amount, U256::ZERO, ALICE, Bytes::new()).abi_encode(),
        );

        run_tx(&mut db, &pair, calldata_swap, &ALICE).expect("Error executing swap tx");

        // Transfer liquidity to pair and burn
        let liquidity = erc20_balance(&mut db, pair, ALICE);
        let selector_transfer = get_selector_from_sig("transfer(address,uint256)");
        let calldata_transfer = get_calldata(selector_transfer, (pair, liquidity).abi_encode());

        run_tx(&mut db, &pair, calldata_transfer, &ALICE).expect("Error executing transfer tx");

        // Execute burn
        let selector_burn = get_selector_from_sig("burn(address)");
        let calldata_burn = get_calldata(selector_burn, ALICE.abi_encode());

        run_tx(&mut db, &pair, calldata_burn, &ALICE).expect("Error executing burn tx");

        // Verify total supply is just MINIMUM_LIQUIDITY since `fee_to` is not set
        assert_eq!(
            get_total_supply(&mut db, pair),
            MINIMUM_LIQUIDITY,
            "Total supply should equal MINIMUM_LIQUIDITY"
        );
    }

    #[test]
    fn test_fee_to_on() {
        let UniswapV2PairSetup {
            mut db,
            factory,
            token0,
            token1,
            pair,
            fee_to_setter,
        } = pair_setup();

        // Set feeTo to BOB
        let selector_set_fee_to = get_selector_from_sig("set_fee_to(address)");
        let calldata_set_fee_to = get_calldata(selector_set_fee_to, BOB.abi_encode());

        run_tx(&mut db, &factory, calldata_set_fee_to, &fee_to_setter)
            .expect("Error executing set_fee_to tx");

        // Setup initial liquidity
        let token0_amount = U256::from(1000e18);
        let token1_amount = U256::from(1000e18);

        erc20_mint(&mut db, token0, ALICE, token0_amount);
        erc20_mint(&mut db, token1, ALICE, token1_amount);
        add_liquidity(&mut db, pair, token0, token1, token0_amount, token1_amount);

        // Perform a swap
        let swap_amount = U256::from(1e18);
        let expected_output_amount = U256::from(996006981039903216_u128);

        erc20_mint(&mut db, token1, ALICE, swap_amount);
        erc20_transfer(&mut db, token1, pair, swap_amount);

        // Execute swap
        let selector_swap = get_selector_from_sig("swap(uint256,uint256,address,bytes)");
        let calldata_swap = get_calldata(
            selector_swap,
            (expected_output_amount, U256::ZERO, ALICE, Bytes::new()).abi_encode(),
        );

        run_tx(&mut db, &pair, calldata_swap, &ALICE).expect("Error executing swap tx");

        // Transfer liquidity to pair and burn
        let liquidity = erc20_balance(&mut db, pair, ALICE);
        let selector_transfer = get_selector_from_sig("transfer(address,uint256)");
        let calldata_transfer = get_calldata(selector_transfer, (pair, liquidity).abi_encode());

        run_tx(&mut db, &pair, calldata_transfer, &ALICE).expect("Error executing transfer tx");

        // Execute burn
        let selector_burn = get_selector_from_sig("burn(address)");
        let calldata_burn = get_calldata(selector_burn, ALICE.abi_encode());

        run_tx(&mut db, &pair, calldata_burn, &ALICE).expect("Error executing burn tx");

        // Verify total supply is more than MINIMUM_LIQUIDITY due to fees
        assert!(
            get_total_supply(&mut db, pair) > MINIMUM_LIQUIDITY,
            "Total supply should be greater than MINIMUM_LIQUIDITY"
        );

        // Check BOB's LP token balance
        assert!(
            erc20_balance(&mut db, pair, BOB) > U256::ZERO,
            "BOB should have LP tokens from fees"
        );
    }

    #[test]
    fn test_price_update_after_swap() {
        let UniswapV2PairSetup {
            mut db,
            factory: _,
            token0,
            token1,
            pair,
            fee_to_setter: _,
        } = pair_setup();

        // Setup initial liquidity
        let token0_amount = U256::from(3e18);
        let token1_amount = U256::from(3e18);

        erc20_mint(&mut db, token0, ALICE, token0_amount * U256::from(2)); // Extra for swap
        erc20_mint(&mut db, token1, ALICE, token1_amount);

        add_liquidity(&mut db, pair, token0, token1, token0_amount, token1_amount);

        // Sync the pair to set initial price accumulators
        let selector_sync = get_selector_from_sig("sync()");
        run_tx(&mut db, &pair, selector_sync.to_vec(), &ALICE).expect("Error executing sync tx");

        // Get initial price accumulators
        let selector_price0 = get_selector_from_sig("price0_cumulative_last()");
        let selector_price1 = get_selector_from_sig("price1_cumulative_last()");

        let price0_before_result = run_tx(&mut db, &pair, selector_price0.to_vec(), &ALICE)
            .expect("Error executing price0_cumulative_last tx")
            .output;
        let price1_before_result = run_tx(&mut db, &pair, selector_price1.to_vec(), &ALICE)
            .expect("Error executing price1_cumulative_last tx")
            .output;

        let price0_before: U256 = B256::from_slice(&price0_before_result).into();
        let price1_before: U256 = B256::from_slice(&price1_before_result).into();

        // Perform a swap to change the price ratio
        let swap_amount = U256::from(3e18);
        erc20_transfer(&mut db, token0, pair, swap_amount);

        // Execute swap with 1 token output to make price change significant
        let selector_swap = get_selector_from_sig("swap(uint256,uint256,address,bytes)");
        let calldata_swap = get_calldata(
            selector_swap,
            (U256::ZERO, U256::from(1e18), ALICE, Bytes::new()).abi_encode(),
        );

        run_tx(&mut db, &pair, calldata_swap, &ALICE).expect("Error executing swap tx");

        // Get updated price accumulators
        let price0_after_result = run_tx(&mut db, &pair, selector_price0.to_vec(), &ALICE)
            .expect("Error executing price0_cumulative_last tx")
            .output;
        let price1_after_result = run_tx(&mut db, &pair, selector_price1.to_vec(), &ALICE)
            .expect("Error executing price1_cumulative_last tx")
            .output;

        let price0_after: U256 = B256::from_slice(&price0_after_result).into();
        let price1_after: U256 = B256::from_slice(&price1_after_result).into();

        // Verify price accumulators have updated
        assert!(
            price0_after >= price0_before,
            "Price0 accumulator should increase or stay the same"
        );
        assert!(
            price1_after >= price1_before,
            "Price1 accumulator should increase or stay the same"
        );
    }
}
