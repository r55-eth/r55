use r55::{
    compile_deploy, compile_with_prefix, exec::deploy_contract, test_utils::initialize_logger,
};
use revm::InMemoryDB;
use tracing::{error, info};

const ERC20X_ALONE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../erc20x_standalone");

#[test]
fn deploy_erc20x_without_contract_dependencies() {
    initialize_logger();

    let mut db = InMemoryDB::default();

    info!(
        "Compiling erc20x using user-defined IERC20 (without contract dependencies). Contract path: {}",
        ERC20X_ALONE_PATH
    );
    let bytecode = match compile_with_prefix(compile_deploy, ERC20X_ALONE_PATH) {
        Ok(code) => code,
        Err(e) => {
            error!("Failed to compile ERC20X: {:?}", e);
            panic!("ERC20X compilation failed");
        }
    };

    match deploy_contract(&mut db, bytecode, None) {
        Ok(addr) => info!("Contract deployed at {}", addr),
        Err(e) => {
            error!("Failed to deploy ERC20X: {:?}", e);
            panic!("ERC20X deployment failed")
        }
    }
}
