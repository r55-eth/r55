use alloy_primitives::{Address, B256, U256};
use alloy_sol_types::SolValue;
use r55::{
    compile_deploy, compile_with_prefix,
    exec::{deploy_contract, run_tx},
    test_utils::{
        add_balance_to_db, get_calldata, get_selector_from_sig, initialize_logger, ALICE, BOB,
        CAROL,
    },
};
use revm::InMemoryDB;

const ERC721_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/erc721");

struct ERC721Setup {
    db: InMemoryDB,
    token: Address,
    owner: Address,
}

fn erc721_setup(owner: Address) -> ERC721Setup {
    initialize_logger();
    let mut db = InMemoryDB::default();

    // Fund user accounts with some ETH
    for user in [ALICE, BOB, CAROL] {
        add_balance_to_db(&mut db, user, 1e18 as u64);
    }

    // Deploy contract
    let constructor = owner.abi_encode();
    let bytecode = compile_with_prefix(compile_deploy, ERC721_PATH).unwrap();
    let token = deploy_contract(&mut db, bytecode, Some(constructor)).unwrap();

    ERC721Setup { db, token, owner }
}

#[test]
fn test_erc721_deployment() {
    let ERC721Setup {
        mut db,
        token,
        owner,
    } = erc721_setup(ALICE);

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
fn test_erc721_mint() {
    let ERC721Setup {
        mut db,
        token,
        owner,
    } = erc721_setup(ALICE);
    let recipient = BOB;
    let token_id = U256::from(1);

    // Mint token
    let selector_mint = get_selector_from_sig("mint(address,uint256)");
    let calldata_mint = get_calldata(selector_mint, (recipient, token_id).abi_encode());

    let mint_result = run_tx(&mut db, &token, calldata_mint, &owner).expect("Error executing tx");
    assert!(mint_result.status, "Mint transaction failed");

    // Verify ownership
    let selector_owner_of = get_selector_from_sig("owner_of(uint256)");
    let calldata_owner = get_calldata(selector_owner_of, token_id.abi_encode());

    let owner_result = run_tx(&mut db, &token, calldata_owner, &owner)
        .expect("Error executing tx")
        .output;

    assert_eq!(
        Address::from_word(B256::from_slice(owner_result.as_slice())),
        recipient,
        "Incorrect token owner"
    );

    // Verify balance
    let selector_balance = get_selector_from_sig("balance_of(address)");
    let calldata_balance = get_calldata(selector_balance, recipient.abi_encode());

    let balance_result = run_tx(&mut db, &token, calldata_balance, &owner)
        .expect("Error executing tx")
        .output;

    assert_eq!(
        U256::from_be_bytes::<32>(balance_result.as_slice().try_into().unwrap()),
        U256::from(1),
        "Incorrect balance"
    );
}

#[test]
fn test_erc721_approve_and_transfer_from() {
    let ERC721Setup {
        mut db,
        token,
        owner,
    } = erc721_setup(ALICE);
    let token_owner = BOB;
    let spender = CAROL;
    let recipient = ALICE;
    let token_id = U256::from(1);

    // Mint token to initial owner
    let selector_mint = get_selector_from_sig("mint(address,uint256)");
    let calldata_mint = get_calldata(selector_mint, (token_owner, token_id).abi_encode());

    run_tx(&mut db, &token, calldata_mint, &owner).expect("Error executing mint tx");

    // Approve spender
    let selector_approve = get_selector_from_sig("approve(address,uint256)");
    let calldata_approve = get_calldata(selector_approve, (spender, token_id).abi_encode());

    let approve_result =
        run_tx(&mut db, &token, calldata_approve, &token_owner).expect("Error executing tx");
    assert!(approve_result.status, "Approve transaction failed");

    // Transfer from token_owner to recipient
    let selector_transfer_from = get_selector_from_sig("transfer_from(address,address,uint256)");
    let calldata_transfer_from = get_calldata(
        selector_transfer_from,
        (token_owner, recipient, token_id).abi_encode(),
    );

    let transfer_result =
        run_tx(&mut db, &token, calldata_transfer_from, &spender).expect("Error executing tx");
    assert!(transfer_result.status, "TransferFrom transaction failed");

    // Verify new owner
    let selector_owner_of = get_selector_from_sig("owner_of(uint256)");
    let calldata_owner = get_calldata(selector_owner_of, token_id.abi_encode());

    let owner_result = run_tx(&mut db, &token, calldata_owner, &owner)
        .expect("Error executing tx")
        .output;

    assert_eq!(
        Address::from_word(B256::from_slice(owner_result.as_slice())),
        recipient,
        "Incorrect token owner after transfer"
    );
}

#[test]
fn test_erc721_set_approval_for_all() {
    let ERC721Setup {
        mut db,
        token,
        owner,
    } = erc721_setup(ALICE);
    let token_owner = BOB;
    let operator = CAROL;
    let token_id = U256::from(1);

    // Mint token
    let selector_mint = get_selector_from_sig("mint(address,uint256)");
    let calldata_mint = get_calldata(selector_mint, (token_owner, token_id).abi_encode());

    run_tx(&mut db, &token, calldata_mint, &owner).expect("Error executing mint tx");

    // Set approval for all
    let selector_set_approval = get_selector_from_sig("set_approval_for_all(address,bool)");
    let calldata_set_approval = get_calldata(selector_set_approval, (operator, true).abi_encode());

    let approval_result =
        run_tx(&mut db, &token, calldata_set_approval, &token_owner).expect("Error executing tx");
    assert!(
        approval_result.status,
        "SetApprovalForAll transaction failed"
    );

    // Verify approval status
    let selector_is_approved = get_selector_from_sig("is_approved_for_all(address,address)");
    let calldata_is_approved =
        get_calldata(selector_is_approved, (token_owner, operator).abi_encode());

    let is_approved_result = run_tx(&mut db, &token, calldata_is_approved, &owner)
        .expect("Error executing tx")
        .output;

    assert_eq!(is_approved_result[31], 1, "Incorrect approval status");
}

#[test]
fn test_erc721_mint_already_exists() {
    let ERC721Setup {
        mut db,
        token,
        owner,
    } = erc721_setup(ALICE);
    let recipient = BOB;
    let token_id = U256::from(1);

    // Mint token
    let selector_mint = get_selector_from_sig("mint(address,uint256)");
    let calldata_mint = get_calldata(selector_mint, (recipient, token_id).abi_encode());

    run_tx(&mut db, &token, calldata_mint.clone(), &owner).expect("Error executing first mint tx");

    // Attempt second mint of same token ID
    let result = run_tx(&mut db, &token, calldata_mint, &owner)
        .expect_err("Mint transaction succeeded when it should fail");

    assert!(
        result.matches_custom_error("ERC721Error::AlreadyMinted"),
        "Incorrect error signature"
    );
}

#[test]
fn test_erc721_unauthorized_transfer() {
    let ERC721Setup {
        mut db,
        token,
        owner,
    } = erc721_setup(ALICE);
    let token_owner = BOB;
    let unauthorized = CAROL;
    let recipient = ALICE;
    let token_id = U256::from(1);

    // Mint token
    let selector_mint = get_selector_from_sig("mint(address,uint256)");
    let calldata_mint = get_calldata(selector_mint, (token_owner, token_id).abi_encode());

    run_tx(&mut db, &token, calldata_mint, &owner).expect("Error executing mint tx");

    // Attempt unauthorized transfer
    let selector_transfer_from = get_selector_from_sig("transfer_from(address,address,uint256)");
    let calldata_transfer_from = get_calldata(
        selector_transfer_from,
        (token_owner, recipient, token_id).abi_encode(),
    );

    let result = run_tx(&mut db, &token, calldata_transfer_from, &unauthorized)
        .expect_err("Transfer transaction succeeded when it should fail");

    assert!(
        result.matches_custom_error("ERC721Error::Unauthorized"),
        "Incorrect error signature"
    );
}

#[test]
fn test_erc721_wrong_from_address() {
    let ERC721Setup {
        mut db,
        token,
        owner,
    } = erc721_setup(ALICE);
    let token_owner = BOB;
    let wrong_from = CAROL;
    let recipient = ALICE;
    let token_id = U256::from(1);

    // Mint token
    let selector_mint = get_selector_from_sig("mint(address,uint256)");
    let calldata_mint = get_calldata(selector_mint, (token_owner, token_id).abi_encode());

    run_tx(&mut db, &token, calldata_mint, &owner).expect("Error executing mint tx");

    // Attempt transfer with wrong from address
    let selector_transfer_from = get_selector_from_sig("transfer_from(address,address,uint256)");
    let calldata_transfer_from = get_calldata(
        selector_transfer_from,
        (wrong_from, recipient, token_id).abi_encode(),
    );

    let result = run_tx(&mut db, &token, calldata_transfer_from, &token_owner)
        .expect_err("Transfer transaction succeeded when it should fail");

    assert!(
        result.matches_custom_error("ERC721Error::WrongFrom"),
        "Incorrect error signature"
    );
}

#[test]
fn test_erc721_zero_address_checks() {
    let ERC721Setup {
        mut db,
        token,
        owner,
    } = erc721_setup(ALICE);
    let zero_address = Address::ZERO;
    let token_id = U256::from(1);

    // Test mint to zero address
    let selector_mint = get_selector_from_sig("mint(address,uint256)");
    let calldata_mint = get_calldata(selector_mint, (zero_address, token_id).abi_encode());

    let result = run_tx(&mut db, &token, calldata_mint, &owner)
        .expect_err("Mint transaction succeeded when it should fail");

    assert!(
        result.matches_custom_error("ERC721Error::ZeroAddress"),
        "Incorrect error signature"
    );

    // Test transfer to zero address
    let token_owner = BOB;

    // First mint token normally
    let calldata_mint = get_calldata(selector_mint, (token_owner, token_id).abi_encode());
    run_tx(&mut db, &token, calldata_mint, &owner).expect("Error executing mint tx");

    // Attempt transfer to zero address
    let selector_transfer_from = get_selector_from_sig("transfer_from(address,address,uint256)");
    let calldata_transfer_from = get_calldata(
        selector_transfer_from,
        (token_owner, zero_address, token_id).abi_encode(),
    );

    let result = run_tx(&mut db, &token, calldata_transfer_from, &token_owner)
        .expect_err("Transfer transaction succeeded when it should fail");

    assert!(
        result.matches_custom_error("ERC721Error::ZeroAddress"),
        "Incorrect error signature"
    );
}

#[test]
fn test_erc721_balance_of_zero_address() {
    let ERC721Setup {
        mut db,
        token,
        owner: _,
    } = erc721_setup(ALICE);
    let zero_address = Address::ZERO;

    let selector_balance = get_selector_from_sig("balance_of(address)");
    let calldata_balance = get_calldata(selector_balance, zero_address.abi_encode());

    let result = run_tx(&mut db, &token, calldata_balance, &ALICE)
        .expect_err("Balance query succeeded when it should fail");

    assert!(
        result.matches_custom_error("ERC721Error::ZeroAddress"),
        "Incorrect error signature"
    );
}

#[test]
fn test_erc721_query_non_existent_token() {
    let ERC721Setup {
        mut db,
        token,
        owner: _,
    } = erc721_setup(ALICE);
    let non_existent_token_id = U256::from(999);

    let selector_owner_of = get_selector_from_sig("owner_of(uint256)");
    let calldata_owner = get_calldata(selector_owner_of, non_existent_token_id.abi_encode());

    let result = run_tx(&mut db, &token, calldata_owner, &ALICE)
        .expect_err("Owner query succeeded when it should fail");

    assert!(
        result.matches_custom_error("ERC721Error::NotMinted"),
        "Incorrect error signature"
    );
}
