use std::fs;
use std::path::Path;

fn main() {
    // Setup output directories
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let generated_dir = project_root.join("src/generated");
    fs::create_dir_all(&generated_dir).unwrap();

    // Check for compiled contracts
    let contracts_dir = project_root.parent().unwrap().join("r55-output-bytecode");
    if !contracts_dir.exists() {
        panic!(
            "No compiled contracts found. Please run `cargo run -p r55-compile` first"
        );
    }

    // Generate code to get compiled bytecode
    let mut code = String::from(r#"//! This module contains auto-generated code.
//! Do not edit manually!

use alloy_primitives::Bytes;

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
            
            code.push_str(&format!(
                "pub const {}_BYTECODE: &[u8] = include_bytes!(\"../../../r55-output-bytecode/{}.bin\");\n",
                contract_name.replace("-", "_"),
                contract_name.to_lowercase()
            ));
        }
    }

    // Helper function to get the bytecode given a contract name
    code.push_str("\npub fn get_bytecode(contract_name: &str) -> Bytes {\n");
    code.push_str("    let initcode = match contract_name {\n");
    
    for entry in fs::read_dir(&contracts_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        
        if path.extension().unwrap_or_default() == "bin" {
            let contract_name = path
                .file_stem()
                .unwrap()
                .to_str()
                .unwrap();
            
            code.push_str(&format!(
                "        \"{}\" => {}_BYTECODE,\n",
                contract_name.replace("-", "_"),
                contract_name.replace("-", "_").to_uppercase()
            ));
        }
    }
    
    code.push_str(
r#"        _ => panic!("Contract not found: {}", contract_name)
    };

    Bytes::from(initcode)
}
"#);

    // Write generated code
    let registry_path = generated_dir.join("mod.rs");
    fs::write(registry_path, code).unwrap();

    // Tell cargo to rerun if any compiled contracts change
    println!("cargo:rerun-if-changed=r55_output_bytecode");
}

