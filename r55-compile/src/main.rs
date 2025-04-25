mod ast;
mod generate;
mod helpers;
mod types;

use generate::{generate_deployable, generate_temp_crates};
use helpers::{find_r55_projects, sort_generated_contracts};

use std::{fs, path::Path};
use tracing::{debug, info};

fn main() -> eyre::Result<()> {
    // Initialize logging
    let tracing_sub = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(false)
        .finish();
    tracing::subscriber::set_global_default(tracing_sub)?;

    // Setup output directory for the compiled bytecode
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let output_dir = project_root.join("r55-output-bytecode");
    fs::create_dir_all(&output_dir)?;

    // Setup temporary directory for the generated crates
    let temp_dir = project_root.join("target").join("r55-generated");
    fs::create_dir_all(&temp_dir)?;

    // Find all R55 projects under the examples directory
    let target_dir = project_root.join("examples");
    let projects = find_r55_projects(&target_dir)?;

    // Log discovered examples and their contracts
    info!("Found {} R55 project:", projects.len());
    for (i, example) in projects.iter().enumerate() {
        info!(
            "  {}. {} with {} contracts:",
            i + 1,
            example.name,
            example.targets.len()
        );
        for target in &example.targets {
            info!("     - {}", target.ident);
        }
    }

    // Generate temp version of all R55 contracts
    let generated_contracts = generate_temp_crates(&projects, &temp_dir, project_root)?;
    debug!("GENERATED CONTRACTS:");
    for (i, contract) in generated_contracts.iter().enumerate() {
        debug!(" {}. {:?}", i, contract);
    }

    // Compile each contract in order
    let sorted_contracts = sort_generated_contracts(generated_contracts)?;
    for contract in sorted_contracts {
        info!("Generating deployable.rs for contract: {}", contract.name);

        // Generate `deployable.rs` in the working dir, and the temp one
        generate_deployable(&contract, true)?;
        generate_deployable(&contract, false)?;

        // Compile deployment code and save in the file
        info!("Compiling: {}", contract.name);
        let deploy_bytecode = contract.compile()?;
        let deploy_path = output_dir.join(format!("{}.bin", contract.name));

        fs::write(deploy_path, deploy_bytecode)?;
    }

    Ok(())
}
