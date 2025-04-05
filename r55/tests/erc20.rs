use alloy_primitives::{Address, B256, U256};
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

struct ERC20Setup {
    db: InMemoryDB,
    token: Address,
    owner: Address,
}

fn erc20_setup(owner: Address) -> ERC20Setup {
    initialize_logger();
    let mut db = InMemoryDB::default();

    // Fund user accounts with some ETH
    for user in [ALICE, BOB, CAROL] {
        add_balance_to_db(&mut db, user, 1e18 as u64);
    }

    // Deploy contract
    let constructor = owner.abi_encode();
    let bytecode = get_bytecode("erc20");
    let token = deploy_contract(&mut db, bytecode, Some(constructor)).unwrap();

    ERC20Setup { db, token, owner }
}

#[test]
fn test_erc20_deployment() {
    let ERC20Setup {
        mut db,
        token,
        owner,
    } = erc20_setup(ALICE);

    let selector_owner = get_selector_from_sig("owner()");
    let owner_result = run_tx(&mut db, &token, selector_owner.to_vec(), &ALICE)
        .expect("Error executing tx")
        .output;

    assert_eq!(
        Address::from_word(B256::from_slice(owner_result.as_slice())),
        owner,
        "Incorrect owner"
    );
}

#[test]
fn test_erc20_mint() {
    let ERC20Setup {
        mut db,
        token,
        owner,
    } = erc20_setup(ALICE);
    let recipient = BOB;

    let mint_amount = U256::from(100e18);
    let selector_mint = get_selector_from_sig("mint(address,uint256)");
    let calldata_mint = get_calldata(selector_mint, (recipient, mint_amount).abi_encode());

    let mint_result = run_tx(&mut db, &token, calldata_mint, &owner).expect("Error executing tx");
    assert!(mint_result.status, "Mint transaction failed");

    // Verify balance
    let selector_balance = get_selector_from_sig("balance_of(address)");
    let calldata_balance = get_calldata(selector_balance, recipient.abi_encode());

    let balance_result = run_tx(&mut db, &token, calldata_balance, &owner)
        .expect("Error executing tx")
        .output;

    assert_eq!(
        U256::from_be_bytes::<32>(balance_result.as_slice().try_into().unwrap()),
        mint_amount,
        "Incorrect balance"
    );
}

#[test]
fn test_erc20_transfer() {
    let ERC20Setup {
        mut db,
        token,
        owner,
    } = erc20_setup(ALICE);
    let recipient = BOB;

    // Mint initial tokens to owner
    let mint_amount = U256::from(100e18);
    let selector_mint = get_selector_from_sig("mint(address,uint256)");
    let calldata_mint = get_calldata(selector_mint, (owner, mint_amount).abi_encode());

    let mint_result = run_tx(&mut db, &token, calldata_mint, &owner).expect("Error executing tx");
    assert!(mint_result.status, "Mint transaction failed");

    // Transfer
    let transfer_amount = U256::from(50e18);
    let selector_transfer = get_selector_from_sig("transfer(address,uint256)");
    let calldata_transfer =
        get_calldata(selector_transfer, (recipient, transfer_amount).abi_encode());

    let transfer_result =
        run_tx(&mut db, &token, calldata_transfer, &owner).expect("Error executing tx");
    assert!(transfer_result.status, "Transfer transaction failed");

    // Verify balances
    let selector_balance = get_selector_from_sig("balance_of(address)");

    // Check recipient balance
    let calldata_recipient_balance = get_calldata(selector_balance, recipient.abi_encode());

    let recipient_balance_result = run_tx(&mut db, &token, calldata_recipient_balance, &owner)
        .expect("Error executing tx")
        .output;

    assert_eq!(
        U256::from_be_bytes::<32>(recipient_balance_result.as_slice().try_into().unwrap()),
        transfer_amount,
        "Incorrect recipient balance"
    );

    // Check owner remaining balance
    let calldata_owner_balance = get_calldata(selector_balance, owner.abi_encode());

    let owner_balance_result = run_tx(&mut db, &token, calldata_owner_balance, &owner)
        .expect("Error executing tx")
        .output;

    assert_eq!(
        U256::from_be_bytes::<32>(owner_balance_result.as_slice().try_into().unwrap()),
        mint_amount - transfer_amount,
        "Incorrect owner balance"
    );
}

#[test]
fn test_erc20_approve_and_transfer_from() {
    let ERC20Setup {
        mut db,
        token,
        owner,
    } = erc20_setup(ALICE);
    let spender = BOB;
    let recipient = CAROL;

    // Mint initial tokens to owner
    let mint_amount = U256::from(100e18);
    let selector_mint = get_selector_from_sig("mint(address,uint256)");
    let calldata_mint = get_calldata(selector_mint, (owner, mint_amount).abi_encode());

    let mint_result = run_tx(&mut db, &token, calldata_mint, &owner).expect("Error executing tx");
    assert!(mint_result.status, "Mint transaction failed");

    // Approve
    let approve_amount = U256::from(50e18);
    let selector_approve = get_selector_from_sig("approve(address,uint256)");
    let calldata_approve = get_calldata(selector_approve, (spender, approve_amount).abi_encode());

    let approve_result =
        run_tx(&mut db, &token, calldata_approve, &owner).expect("Error executing tx");
    assert!(approve_result.status, "Approve transaction failed");

    // Transfer from owner
    let transfer_amount = U256::from(30e18);
    let selector_transfer_from = get_selector_from_sig("transfer_from(address,address,uint256)");
    let calldata_transfer_from = get_calldata(
        selector_transfer_from,
        (owner, recipient, transfer_amount).abi_encode(),
    );

    let transfer_from_result =
        run_tx(&mut db, &token, calldata_transfer_from, &spender).expect("Error executing tx");
    assert!(
        transfer_from_result.status,
        "TransferFrom transaction failed"
    );

    // Verify balances and allowance
    let selector_balance = get_selector_from_sig("balance_of(address)");
    let selector_allowance = get_selector_from_sig("allowance(address,address)");

    // Check recipient balance
    let calldata_recipient_balance = get_calldata(selector_balance, recipient.abi_encode());

    let recipient_balance_result = run_tx(&mut db, &token, calldata_recipient_balance, &owner)
        .expect("Error executing tx")
        .output;

    assert_eq!(
        U256::from_be_bytes::<32>(recipient_balance_result.as_slice().try_into().unwrap()),
        transfer_amount,
        "Incorrect recipient balance"
    );

    // Check remaining allowance
    let calldata_allowance = get_calldata(selector_allowance, (owner, spender).abi_encode());

    let allowance_result = run_tx(&mut db, &token, calldata_allowance, &owner)
        .expect("Error executing tx")
        .output;

    assert_eq!(
        U256::from_be_bytes::<32>(allowance_result.as_slice().try_into().unwrap()),
        approve_amount - transfer_amount,
        "Incorrect allowance"
    );
}

#[test]
fn test_erc20_transfer_insufficient_balance() {
    let ERC20Setup {
        mut db,
        token,
        owner,
    } = erc20_setup(ALICE);
    let recipient = BOB;

    // Mint tokens to owner
    let mint_amount = U256::from(1e18);
    let selector_mint = get_selector_from_sig("mint(address,uint256)");
    let calldata_mint = get_calldata(selector_mint, (owner, mint_amount).abi_encode());

    run_tx(&mut db, &token, calldata_mint, &owner).expect("Error executing mint tx");

    // Attempt to transfer more tokens than balance
    let transfer_amount = U256::from(5e18);
    let selector_transfer = get_selector_from_sig("transfer(address,uint256)");
    let calldata_transfer =
        get_calldata(selector_transfer, (recipient, transfer_amount).abi_encode());

    let result = run_tx(&mut db, &token, calldata_transfer, &owner)
        .expect_err("Transfer transaction succeeded when it should fail");

    assert!(
        result.matches_custom_error_with_args(
            "ERC20Error::InsufficientBalance(uint256)",
            mint_amount.abi_encode()
        ),
        "Incorrect error signature"
    );
}

#[test]
fn test_erc20_transfer_from_insufficient_allowance() {
    let ERC20Setup {
        mut db,
        token,
        owner,
    } = erc20_setup(ALICE);
    let spender = BOB;
    let recipient = CAROL;

    // Mint tokens to owner
    let mint_amount = U256::from(1e18);
    let selector_mint = get_selector_from_sig("mint(address,uint256)");
    let calldata_mint = get_calldata(selector_mint, (owner, mint_amount).abi_encode());

    run_tx(&mut db, &token, calldata_mint, &owner).expect("Error executing mint tx");

    // Approve some tokens
    let approve_amount = U256::from(1e18);
    let selector_approve = get_selector_from_sig("approve(address,uint256)");
    let calldata_approve = get_calldata(selector_approve, (spender, approve_amount).abi_encode());

    run_tx(&mut db, &token, calldata_approve, &owner).expect("Error executing approve tx");

    // Attempt to transfer more than allowance
    let transfer_amount = U256::from(5e18);
    let selector_transfer_from = get_selector_from_sig("transfer_from(address,address,uint256)");
    let calldata_transfer_from = get_calldata(
        selector_transfer_from,
        (owner, recipient, transfer_amount).abi_encode(),
    );

    let result = run_tx(&mut db, &token, calldata_transfer_from, &spender)
        .expect_err("TransferFrom transaction succeeded when it should fail");

    assert!(transfer_amount > approve_amount);
    assert!(
        result.matches_custom_error_with_args(
            "ERC20Error::InsufficientAllowance(uint256)",
            approve_amount.abi_encode()
        ),
        "Incorrect error signature"
    );
}

#[test]
fn test_erc20_transfer_from_insufficient_balance() {
    let ERC20Setup {
        mut db,
        token,
        owner,
    } = erc20_setup(ALICE);
    let spender = BOB;
    let recipient = CAROL;

    // Mint tokens to owner
    let mint_amount = U256::from(1e18);
    let selector_mint = get_selector_from_sig("mint(address,uint256)");
    let calldata_mint = get_calldata(selector_mint, (owner, mint_amount).abi_encode());

    run_tx(&mut db, &token, calldata_mint, &owner).expect("Error executing mint tx");

    // Approve transfer amount
    let transfer_amount = U256::from(2e18);
    let selector_approve = get_selector_from_sig("approve(address,uint256)");
    let calldata_approve = get_calldata(selector_approve, (spender, transfer_amount).abi_encode());

    run_tx(&mut db, &token, calldata_approve, &owner).expect("Error executing approve tx");

    // Attempt to transfer more than balance
    let selector_transfer_from = get_selector_from_sig("transfer_from(address,address,uint256)");
    let calldata_transfer_from = get_calldata(
        selector_transfer_from,
        (owner, recipient, transfer_amount).abi_encode(),
    );

    let result = run_tx(&mut db, &token, calldata_transfer_from, &spender)
        .expect_err("TransferFrom transaction succeeded when it should fail");

    assert!(transfer_amount > mint_amount);
    assert!(
        result.matches_custom_error_with_args(
            "ERC20Error::InsufficientBalance(uint256)",
            mint_amount.abi_encode()
        ),
        "Incorrect error signature"
    );
}

#[test]
fn test_erc20_mint_unauthorized() {
    let ERC20Setup {
        mut db,
        token,
        owner: _,
    } = erc20_setup(ALICE);
    let unauthorized = BOB;
    let recipient = CAROL;

    let mint_amount = U256::from(1e18);
    let selector_mint = get_selector_from_sig("mint(address,uint256)");
    let calldata_mint = get_calldata(selector_mint, (recipient, mint_amount).abi_encode());

    let result = run_tx(&mut db, &token, calldata_mint, &unauthorized)
        .expect_err("Mint transaction succeeded when it should fail");

    assert!(
        result.matches_custom_error("ERC20Error::OnlyOwner"),
        "Incorrect error signature"
    );
}

#[test]
fn test_erc20_zero_address_checks() {
    let ERC20Setup {
        mut db,
        token,
        owner,
    } = erc20_setup(ALICE);
    let zero_address = Address::ZERO;

    // Test mint to zero address
    let mint_amount = U256::from(1e18);
    let selector_mint = get_selector_from_sig("mint(address,uint256)");
    let calldata_mint = get_calldata(selector_mint, (zero_address, mint_amount).abi_encode());

    let result = run_tx(&mut db, &token, calldata_mint, &owner)
        .expect_err("Mint transaction succeeded when it should fail");

    assert!(
        result.matches_custom_error("ERC20Error::ZeroAddress"),
        "Incorrect error signature"
    );

    // Test transfer to zero address
    let selector_transfer = get_selector_from_sig("transfer(address,uint256)");
    let calldata_transfer =
        get_calldata(selector_transfer, (zero_address, mint_amount).abi_encode());

    let result = run_tx(&mut db, &token, calldata_transfer, &owner)
        .expect_err("Transfer transaction succeeded when it should fail");

    assert!(
        result.matches_custom_error("ERC20Error::ZeroAddress"),
        "Incorrect error signature"
    );
}

#[test]
fn test_erc20_zero_amount_checks() {
    let ERC20Setup {
        mut db,
        token,
        owner,
    } = erc20_setup(ALICE);
    let recipient = BOB;

    // Test mint zero amount
    let zero_amount = U256::ZERO;
    let selector_mint = get_selector_from_sig("mint(address,uint256)");
    let calldata_mint = get_calldata(selector_mint, (recipient, zero_amount).abi_encode());

    let result = run_tx(&mut db, &token, calldata_mint, &owner)
        .expect_err("Mint transaction succeeded when it should fail");

    assert!(
        result.matches_custom_error("ERC20Error::ZeroAmount"),
        "Incorrect error signature"
    );

    // Test transfer zero amount
    let selector_transfer = get_selector_from_sig("transfer(address,uint256)");
    let calldata_transfer = get_calldata(selector_transfer, (recipient, zero_amount).abi_encode());

    let result = run_tx(&mut db, &token, calldata_transfer, &owner)
        .expect_err("Transfer transaction succeeded when it should fail");

    assert!(
        result.matches_custom_error("ERC20Error::ZeroAmount"),
        "Incorrect error signature"
    );
}

#[test]
fn test_erc20_self_approval() {
    let ERC20Setup {
        mut db,
        token,
        owner,
    } = erc20_setup(ALICE);

    // Attempt to approve self
    let approve_amount = U256::from(1e18);
    let selector_approve = get_selector_from_sig("approve(address,uint256)");
    let calldata_approve = get_calldata(selector_approve, (owner, approve_amount).abi_encode());

    let result = run_tx(&mut db, &token, calldata_approve, &owner)
        .expect_err("Approve transaction succeeded when it should fail");

    assert!(
        result.matches_custom_error("ERC20Error::SelfApproval"),
        "Incorrect error signature"
    );
}

#[test]
fn test_erc20_self_transfer() {
    let ERC20Setup {
        mut db,
        token,
        owner,
    } = erc20_setup(ALICE);

    // First mint some tokens
    let mint_amount = U256::from(1e18);
    let selector_mint = get_selector_from_sig("mint(address,uint256)");
    let calldata_mint = get_calldata(selector_mint, (owner, mint_amount).abi_encode());

    run_tx(&mut db, &token, calldata_mint, &owner).expect("Error executing mint tx");

    // Attempt direct self-transfer
    let selector_transfer = get_selector_from_sig("transfer(address,uint256)");
    let calldata_transfer = get_calldata(selector_transfer, (owner, mint_amount).abi_encode());

    let result = run_tx(&mut db, &token, calldata_transfer, &owner)
        .expect_err("Transfer transaction succeeded when it should fail");

    assert!(
        result.matches_custom_error("ERC20Error::SelfTransfer"),
        "Incorrect error signature"
    );

    // Attempt self-transfer through transferFrom
    let spender = BOB;

    // First approve spender
    let selector_approve = get_selector_from_sig("approve(address,uint256)");
    let calldata_approve = get_calldata(selector_approve, (spender, mint_amount).abi_encode());

    run_tx(&mut db, &token, calldata_approve, &owner).expect("Error executing approve tx");

    // Attempt transferFrom to same address
    let selector_transfer_from = get_selector_from_sig("transfer_from(address,address,uint256)");
    let calldata_transfer_from = get_calldata(
        selector_transfer_from,
        (owner, owner, mint_amount).abi_encode(),
    );

    let result = run_tx(&mut db, &token, calldata_transfer_from, &spender)
        .expect_err("TransferFrom transaction succeeded when it should fail");

    assert!(
        result.matches_custom_error("ERC20Error::SelfTransfer"),
        "Incorrect error signature"
    );
}
