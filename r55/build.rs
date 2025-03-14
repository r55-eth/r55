use std::fs;
use std::path::Path;

fn main() {
    // Setup output directories
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));

    // Check for compiled contracts
    let contracts_dir = project_root.parent().unwrap().join("r55-output-bytecode");
    if !contracts_dir.exists() {
        panic!("No compiled contracts found. Please run `cargo compile` first");
    }

    // Generate `r55/generated/mod.rs` code to get compiled bytecode for tests
    let mut generated = String::from(r#"#![no_std]

//! This module contains auto-generated code.
//! Do not edit manually!

use alloy_core::primitives::Bytes;
use core::include_bytes;
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
            
            generated.push_str(&format!(
                "\npub const {}_BYTECODE: &[u8] = include_bytes!(\"../../../r55-output-bytecode/{}.bin\");",
                contract_name.replace("-", "_"),
                contract_name.to_lowercase()
            ));
        }
    }

    // Helper function to get the bytecode given a contract name
    generated.push_str(&format!("\n{}", "\npub fn get_bytecode(contract_name: &str) -> Bytes {\n    let initcode = match contract_name {\n"));
    
    for entry in fs::read_dir(&contracts_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        
        if path.extension().unwrap_or_default() == "bin" {
            let contract_name = path
                .file_stem()
                .unwrap()
                .to_str()
                .unwrap();
            
            generated.push_str(&format!(
                "        \"{}\" => {}_BYTECODE,\n",
                contract_name.replace("-", "_"),
                contract_name.replace("-", "_").to_uppercase()
            ));
        }
    }
    
    generated.push_str(r#"        _ => return Bytes::new()
    };

    Bytes::from(initcode)
}
"#);

    // Write `r55/generated` code
    let generated_path = project_root.join("src").join("generated");
    fs::create_dir_all(&generated_path).unwrap();
    let generated_path = generated_path.join("mod.rs");
    fs::write(generated_path, generated).unwrap();

    // Tell cargo to rerun if any compiled contracts change
    println!("cargo:rerun-if-changed=r55_output_bytecode");
}

