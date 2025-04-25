use std::{fmt::Write, fs, path::Path};
use toml::Value;
use tracing::{debug, info};

use crate::{
    ast,
    helpers::{format_toml_table, get_contract_name, get_deployable_deps},
    types::{CompileError, ContractProject, ContractTarget, GeneratedContract},
};

/// Generate deployable implementation for the contract dependencies of an R55 contract.
/// Can generate the files in both, the source (working dir), or the temp one (generated inside `target/`).
pub fn generate_deployable(
    contract: &GeneratedContract,
    target_source: bool,
) -> Result<(), CompileError> {
    if contract.deps.is_empty() {
        return Ok(());
    }

    debug!(
        "GENERATING DEPLOYABLE FOR {} (target_source: {})",
        contract.name, target_source
    );

    let mut content = String::new();

    // Add header comments + common imports
    content.push_str("//! Auto-generated based on Cargo.toml dependencies\n");
    content.push_str(
        "//! This file provides `Deployable` implementations for contract dependencies\n",
    );
    content.push_str("//! TODO (phase-2): rather than using `fn deploy(args: Args)`, figure out the constructor selector from the contract dependency\n\n");
    content.push_str("use alloy_core::primitives::{Address, Bytes};\n");
    content.push_str("use eth_riscv_runtime::{create::Deployable, InitInterface, ReadOnly};\n");
    content.push_str("use core::include_bytes;\n\n");

    // Add imports for each dependency
    for (dep_name, same_project) in &contract.deps {
        debug!(" > {} (same project: {})", dep_name, same_project);
        // Keep original module name (lowercase package name)
        let module_name = dep_name.to_lowercase().replace("-", "_");

        // For interface name, use uppercase I + camel case name (IERC20)
        let interface_name = format!("I{}", get_contract_name(dep_name));

        content.push_str(
            if target_source && *same_project {
                let original_name = contract.get_original_name(&module_name);
                format!("use crate::{}::{};\n", original_name, interface_name)
            } else {
                format!("use {}::{};\n", module_name, interface_name)
            }
            .as_str(),
        );
    }
    content.push('\n');

    // Add bytecode constants for each dependency
    for (dep_name, same_project) in &contract.deps {
        // Use uppercase for constant name
        let const_name = dep_name.to_uppercase().replace('-', "_");

        // Calculate the output bytecode path relative to the contract's directory
        let bytecode_path = if !target_source && *same_project {
            Path::new("../../../../../r55-output-bytecode").join(format!("{}.bin", dep_name))
        } else {
            Path::new("../../../../r55-output-bytecode").join(format!("{}.bin", dep_name))
        };

        content.push_str(&format!(
            "const {}_BYTECODE: &'static [u8] = include_bytes!(\"{}\");\n",
            const_name,
            bytecode_path.display()
        ));
    }
    content.push('\n');

    // Add `Deployable` implementation for each dependency
    for (dep_name, _same_project) in &contract.deps {
        let struct_name = get_contract_name(dep_name);
        let interface_name = format!("I{}", struct_name);

        content.push_str(&format!("pub struct {};\n\n", struct_name));
        content.push_str(&format!("impl Deployable for {} {{\n", struct_name));
        content.push_str(&format!(
            "    type Interface = {}<ReadOnly>;\n\n",
            interface_name
        ));
        content.push_str("    fn __runtime() -> &'static [u8] {\n");
        content.push_str(&format!(
            "        {}_BYTECODE\n",
            dep_name.to_uppercase().replace('-', "_")
        ));
        content.push_str("    }\n");
        content.push_str("}\n\n");
    }

    // Write to file
    let output_path = if !target_source {
        debug!("TEMP DIR: {:?}", contract.path);
        contract.path.join("src").join("deployable.rs")
    } else {
        debug!("WORKING DIR: {:?}", contract.original_source_path);
        contract
            .original_source_path
            .join("src")
            .join("deployable.rs")
    };
    fs::write(&output_path, content)?;

    info!(
        "Generated {:?} for contract: {}",
        output_path, contract.name
    );

    Ok(())
}

/// Generate temporary crates for all input R55 contract targets
pub fn generate_temp_crates(
    projects: &[ContractProject],
    temp_dir: &Path,
    project_root: &Path,
) -> Result<Vec<GeneratedContract>, CompileError> {
    let mut generated_contracts = Vec::new();

    for project in projects {
        debug!("Generating temporary crates for project: {}", project.name);

        for target in &project.targets {
            let target_temp_dir = temp_dir.join(&project.name).join(&target.module);

            fs::create_dir_all(&target_temp_dir)?;
            fs::create_dir_all(target_temp_dir.join("src"))?;

            // Create the `GeneratedContract` instance
            let deployable_deps = get_deployable_deps(project, target);
            let contract = GeneratedContract {
                path: target_temp_dir,
                name: target.generated_package.clone(),
                deps: deployable_deps,
                original_source_path: target
                    .source_file
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .to_path_buf(),
            };

            // Populate the temp dir with the modified files from the working dir
            generate_cargo_toml(project, target, &contract)?;
            decouple_contract_module(project, target, &contract.path)?;
            generate_cargo_config(&contract.path, project_root)?;

            generated_contracts.push(contract);
        }
    }

    Ok(generated_contracts)
}

/// Generate a modified `Cargo.toml` for a R55 temporary crate
fn generate_cargo_toml(
    project: &ContractProject,
    target: &ContractTarget,
    contract: &GeneratedContract,
) -> Result<(), CompileError> {
    let mut cargo_toml = String::new();

    // Basic R55 template
    writeln!(
        cargo_toml,
        r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[workspace]

[features]
default = []
deploy = []
interface-only = []

[dependencies]
"#,
        contract.name
    )?;

    // Process base dependencies
    for (dep_name, dep_info) in &project.deps {
        match dep_info {
            Value::Table(dep_table) => {
                let mut dep_table_adj = dep_table.clone();
                let mut is_generated = false;

                // Adjust path dependencies
                if let Some(Value::String(rel_path)) = dep_table.get("path") {
                    if rel_path == "." {
                        if target.is_self_reference(dep_name) {
                            continue;
                        }

                        if contract.name != project.name {
                            is_generated = true;
                        }
                    }

                    dep_table_adj.insert(
                        "path".into(),
                        // For contract deps from the same project, use relative path to the generated dir
                        if is_generated {
                            Value::String(format!("../{}", dep_name))
                        }
                        // Otherwise, calculate relative path to the dependency location
                        else {
                            let source_rel_path = Path::new(rel_path);
                            let source_abs_path =
                                project.path.join(source_rel_path).canonicalize()?;
                            let rel_path_from_target =
                                pathdiff::diff_paths(&source_abs_path, &contract.path).ok_or_else(
                                    || {
                                        CompileError::PathError(format!(
                                            "Failed to calculate relative path from {:?} to {:?}",
                                            &contract.path, source_abs_path
                                        ))
                                    },
                                )?;
                            Value::String(rel_path_from_target.display().to_string())
                        },
                    );
                }

                writeln!(
                    cargo_toml,
                    "{} = {}",
                    if is_generated {
                        format!("{}-{}", &project.name, dep_name)
                    } else {
                        dep_name.into()
                    },
                    format_toml_table(&dep_table_adj)?
                )?;
            }
            _ => writeln!(cargo_toml, "{} = {}", dep_name, dep_info)?,
        }
    }

    // Process deployable dependencies
    for (dep_name, same_project) in &project.deployable_deps {
        if *same_project && dep_name != &contract.name {
            if let Some((_, original_mod_name)) =
                dep_name.split_once(&format!("{}-", &project.name))
            {
                _ = writeln!(
                    cargo_toml,
                    r#"{} = {{ path = "../{}", features = ["interface-only"] }}"#,
                    dep_name, original_mod_name
                );
            }
        }
    }

    // Add bin targets
    cargo_toml.push_str(
        r#"
[[bin]]
name = "runtime"
path = "src/lib.rs"

[[bin]]
name = "deploy"
path = "src/lib.rs"
required-features = ["deploy"]

[profile.release]
lto = true
opt-level = "z"
"#,
    );

    // Write to file
    fs::write(contract.path.join("Cargo.toml"), cargo_toml)?;

    Ok(())
}

/// Create `.cargo/config.toml` in the temporary crate
fn generate_cargo_config(target_dir: &Path, project_root: &Path) -> Result<(), CompileError> {
    let cargo_dir = target_dir.join(".cargo");
    fs::create_dir_all(&cargo_dir)?;

    // Calculate relative path from target directory to project root's r5-rust-rt.x
    let rust_rt_path = project_root.join("r5-rust-rt.x");
    let rel_rust_rt_path = pathdiff::diff_paths(&rust_rt_path, target_dir).ok_or_else(|| {
        CompileError::PathError(format!(
            "Failed to calculate relative path from {:?} to {:?}",
            target_dir, rust_rt_path
        ))
    })?;

    let config_content = format!(
        r#"[target.riscv64imac-unknown-none-elf]
rustflags = [
  "-C", "link-arg=-T{}",
  "-C", "llvm-args=--inline-threshold=275"
]

[build]
target = "riscv64imac-unknown-none-elf"
"#,
        rel_rust_rt_path.display()
    );

    fs::write(cargo_dir.join("config.toml"), config_content)?;

    Ok(())
}

/// Decouple a contract module from a project, by merging and flattening the project lib, the common modules, and the contract module itself.
///
/// If the contract is directly defined on `lib.rs`, the generated crate matches the working dir
fn decouple_contract_module(
    project: &ContractProject,
    target: &ContractTarget,
    target_dir: &Path,
) -> Result<(), CompileError> {
    let src_dir = target_dir.join("src");
    let contract_modules: Vec<_> = project.targets.iter().map(|t| &t.module).collect();

    // If a contract is defined in `lib.rs` directly, keep as it is, as it must be a single-contract project
    if target.source_file.file_name().unwrap().to_str().unwrap() == "lib.rs" {
        let lib_content = fs::read_to_string(&target.source_file)?;
        fs::write(src_dir.join("lib.rs"), lib_content)?;
    }
    // Otherwise, merge and flatten both `lib.rs` and the contract module
    else {
        // Filter `lib.rs` to remove imports, attributes, and contract module declarations
        let lib_rs_path = project.path.join("src").join("lib.rs");
        let lib_content = fs::read_to_string(&lib_rs_path)?;

        let (lib_ast, mut lib_imports) = ast::process_lib(&lib_content, &contract_modules)?;

        // Filter contract module content to remove imports and attributes
        let contract_module_path = target.source_file.clone();
        let contract_content = fs::read_to_string(&contract_module_path)?;

        let contract_ast = ast::process_contract_module(&contract_content, &mut lib_imports)?;

        // Combine the filtered content and imports. Then write it into the generated `lib.rs`
        let flattened_content = ast::flatten_lib(lib_ast, contract_ast, lib_imports);
        fs::write(src_dir.join("lib.rs"), flattened_content)?;

        debug!(
            "Generated flattened `lib.rs` for contract: {}",
            target.ident
        );
    }

    // Always copy shared modules (without contracts)
    for module_name in &project.shared_modules {
        let module_path = project.path.join("src").join(format!("{}.rs", module_name));

        if module_path.exists() {
            let module_content = fs::read_to_string(&module_path)?;
            fs::write(src_dir.join(format!("{}.rs", module_name)), module_content)?;
        } else {
            // Try `module/mod.rs` structure
            let mod_dir_path = project.path.join("src").join(module_name);
            let mod_file_path = mod_dir_path.join("mod.rs");

            if mod_file_path.exists() {
                // Create the module directory and copy its content
                let target_mod_dir = src_dir.join(module_name);
                fs::create_dir_all(&target_mod_dir)?;

                let mod_content = fs::read_to_string(&mod_file_path)?;
                fs::write(target_mod_dir.join("mod.rs"), mod_content)?;

                if let Ok(entries) = fs::read_dir(mod_dir_path) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        let path = entry.path();
                        if path.is_file() && path.file_name().unwrap() != "mod.rs" {
                            let file_name = path.file_name().unwrap();
                            fs::copy(&path, target_mod_dir.join(file_name))?;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
