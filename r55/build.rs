use std::fs;
use std::path::Path;

fn main() {
    // Setup output directories
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));

    // Check for compiled contracts
    let contracts_dir = project_root.parent().unwrap().join("r55-output-bytecode");
    if !contracts_dir.exists() {
        panic!("No compiled contracts found. Please run `cargo run -p r55-compile` first");
    }

    // Generate `r55/generated/mod.rs` code to get compiled bytecode for tests
    let mut generated = String::from(r#"#![no_std]

//! This module contains auto-generated code.
//! Do not edit manually!

use alloy_core::primitives::Bytes;
use core::include_bytes;
"#);

    // Generate `r55-output-bytecode/lib.rs` code to embed compiled bytecode
    // to other contract's (deployer) bytecode
    let mut output = String::from(r#"#![no_std]

//! This module contains auto-generated code.
//! Do not edit manually!

use alloy_core::primitives::Bytes;
use core::include_bytes;

// Placeholder for initial runtime compilation
#[cfg(not(feature = "with-bytecode"))]
pub fn get_bytecode(_contract_name: &str) -> Bytes { Bytes::new() }

// Necessary to embed bytecode into deployer's runtime
"#);

    // Add bytecode constants
    for entry in fs::read_dir(&contracts_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        
        if path.extension().unwrap_or_default() == "bin" {
            let contract_name = path
                .file_stem()
                .unwrap()
                .to_str()
                .unwrap()
                .to_uppercase();
            
            output.push_str(r#"#[cfg(feature = "with-bytecode")]"#);
            output.push_str(&format!(
                "\npub const {}_BYTECODE: &[u8] = include_bytes!(\"../../r55-output-bytecode/{}.bin\");\n\n",
                contract_name.replace("-", "_"),
                contract_name.to_lowercase()
            ));
            generated.push_str(&format!(
                "\npub const {}_BYTECODE: &[u8] = include_bytes!(\"../../../r55-output-bytecode/{}.bin\");",
                contract_name.replace("-", "_"),
                contract_name.to_lowercase()
            ));
        }
    }

    // Helper function to get the bytecode given a contract name
    let chunk = "\npub fn get_bytecode(contract_name: &str) -> Bytes {\n    let initcode = match contract_name {\n";
    output.push_str(r#"#[cfg(feature = "with-bytecode")]"#);
    output.push_str(chunk);
    generated.push_str(&format!("\n{}", chunk));
    
    for entry in fs::read_dir(&contracts_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        
        if path.extension().unwrap_or_default() == "bin" {
            let contract_name = path
                .file_stem()
                .unwrap()
                .to_str()
                .unwrap();
            
            let chunk = format!(
                "        \"{}\" => {}_BYTECODE,\n",
                contract_name.replace("-", "_"),
                contract_name.replace("-", "_").to_uppercase()
            );
            output.push_str(&chunk);
            generated.push_str(&chunk);
        }
    }
    
    let chunk = r#"        _ => return Bytes::new()
    };

    Bytes::from(initcode)
}
"#;
    output.push_str(chunk);
    generated.push_str(chunk);

    // Write `r55-output-bytecode`
    let output_path = contracts_dir.join("src").join("lib.rs");
    fs::write(output_path, output).unwrap();

    // Write `r55/generated` code
    let generated_path = project_root.join("src").join("generated");
    fs::create_dir_all(&generated_path).unwrap();
    let generated_path = generated_path.join("mod.rs");
    fs::write(generated_path, generated).unwrap();

    // Tell cargo to rerun if any compiled contracts change
    println!("cargo:rerun-if-changed=r55_output_bytecode");
}

