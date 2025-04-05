use std::fs;
use tracing::info;

use crate::compile::ContractWithDeps;

pub fn generate_deployable(contract: &ContractWithDeps) -> eyre::Result<()> {
    // Generate the file content
    let mut content = String::new();

    // Add header comments + common imports
    content.push_str("//! Auto-generated based on Cargo.toml dependencies\n");
    content
        .push_str("//! This file provides Deployable implementations for contract dependencies\n");
    content.push_str("//! TODO (phase-2): rather than using `fn deploy(args: Args)`, figure out the constructor selector from the contract dependency\n\n");
    content.push_str("use alloy_core::primitives::{Address, Bytes};\n");
    content.push_str("use eth_riscv_runtime::{create::Deployable, InitInterface, ReadOnly};\n");
    content.push_str("use core::include_bytes;\n\n");

    // Add imports for each dependency
    for dep in &contract.deps {
        let interface_name = format!("I{}", dep.name.ident);
        content.push_str(&format!("use {}::{};\n", dep.name.package, interface_name));
    }
    content.push('\n');

    // Add bytecode constants for each dependency
    for dep in &contract.deps {
        // Convert dependency name to uppercase for the constant name
        let const_name = dep.name.ident.to_uppercase();
        content.push_str(&format!(
            "const {}_BYTECODE: &'static [u8] = include_bytes!(\"../../../r55-output-bytecode/{}.bin\");\n", 
            const_name, dep.name.package
        ));
    }
    content.push('\n');

    // Add Deployable implementation for each dependency
    for dep in &contract.deps {
        let interface_name = format!("I{}", dep.name.ident);
        let const_name = dep.name.ident.to_uppercase();

        content.push_str(&format!("pub struct {};\n\n", dep.name.ident));
        content.push_str(&format!("impl Deployable for {} {{\n", dep.name.ident));
        content.push_str(&format!(
            "    type Interface = {}<ReadOnly>;\n\n",
            interface_name
        ));
        content.push_str("    fn __runtime() -> &'static [u8] {\n");
        content.push_str(&format!("        {}_BYTECODE\n", const_name));
        content.push_str("    }\n");
        content.push_str("}\n\n");
    }

    // Write the file
    let output_path = contract.path.join("src").join("deployable.rs");
    fs::write(&output_path, content)?;

    info!(
        "Generated {:?} for contract: {}",
        output_path, contract.name.ident
    );

    Ok(())
}
