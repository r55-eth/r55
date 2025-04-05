mod compile;
use compile::{find_r55_contracts, sort_r55_contracts};

mod deployable;
use deployable::generate_deployable;

use std::{fs, path::Path};
use tracing::info;

fn main() -> eyre::Result<()> {
    // Initialize logging
    let tracing_sub = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(false)
        .finish();
    tracing::subscriber::set_global_default(tracing_sub)?;

    // Setup output directory
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let output_dir = project_root.join("r55-output-bytecode");
    fs::create_dir_all(&output_dir)?;

    // TODO: discuss with Leo and Georgios how would importing r55 as a dependency for SC development should look like
    // Find all R55 contracts in examples directory
    let examples_dir = project_root.join("examples");
    let contracts = find_r55_contracts(&examples_dir);

    // Generate deployable files for the dependencies
    if let Some(contracts_with_deps) = contracts.get(&false) {
        for c in contracts_with_deps {
            generate_deployable(c)?;
        }
    }

    // Sort contracts in the correct compilation order
    let contracts = sort_r55_contracts(contracts)?;

    info!(
        "Found {} R55 contracts (in compilation order):\n{}",
        contracts.len(),
        contracts.iter()
            .enumerate()
            .map(|(i, c)| format!("  {}. {}", i+1, c))
            .collect::<Vec<_>>()
            .join("\n")
    ); 

    // Compile each contract
    for contract in contracts {
        info!("Compiling contract: {}", contract.name.ident);

        // Compile deployment code and save in the file
        let deploy_bytecode = contract.compile_r55()?;
        let deploy_path = output_dir.join(format!("{}.bin", contract.name.package));
        fs::write(deploy_path, deploy_bytecode)?;
    }

    Ok(())
}
