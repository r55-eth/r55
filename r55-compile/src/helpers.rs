use std::{collections::HashMap, fmt::Write, fs, path::Path};
use syn::{Attribute, ItemImpl};
use toml::{map::Map, Value};
use tracing::debug;

use crate::types::{CompileError, ContractProject, ContractTarget, GeneratedContract};

/// Finds all R55 smart-contract projects in a directory
pub fn find_r55_projects(dir: &Path) -> Result<Vec<ContractProject>, CompileError> {
    let mut examples = Vec::new();

    // Scan subdirectories for potential examples
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();

            // Skip if not a directory
            if !path.is_dir() {
                continue;
            }

            // Check for `Cargo.toml`
            let cargo_path = path.join("Cargo.toml");
            if !cargo_path.exists() {
                continue;
            }

            // Try to parse as R55 contract project
            match ContractProject::try_from_path(&cargo_path) {
                Ok(project) => {
                    examples.push(project);
                }
                Err(e) => {
                    debug!("Skipping directory {:?}: {}", path, e);
                }
            }
        }
    }

    Ok(examples)
}

/// Sort generated contracts based on their dependencies
pub fn sort_generated_contracts(
    contracts: Vec<GeneratedContract>,
) -> Result<Vec<GeneratedContract>, CompileError> {
    // Create dependency mapping
    let mut dependency_map: HashMap<String, Vec<String>> = HashMap::new();
    for contract in &contracts {
        dependency_map.insert(
            contract.name.clone(),
            contract.deps.iter().map(|(d, _)| d.into()).collect(),
        );
    }

    // Keep track of sorted and remaining contracts
    let mut sorted = Vec::new();
    let mut remaining = contracts;

    // Continue until all contracts are sorted
    while !remaining.is_empty() {
        let initial_len = remaining.len();
        let mut next_remaining = Vec::new();

        for contract in remaining {
            let deps = dependency_map.get(&contract.name).unwrap();

            // Check if all dependencies are already in sorted list
            let all_deps_sorted = deps
                .iter()
                .all(|dep| sorted.iter().any(|c: &GeneratedContract| &c.name == dep));

            if all_deps_sorted {
                sorted.push(contract);
            } else {
                next_remaining.push(contract);
            }
        }

        remaining = next_remaining;

        // If no progress was made, we have a cycle
        if remaining.len() == initial_len && !remaining.is_empty() {
            return Err(CompileError::CyclicDependency);
        }
    }

    Ok(sorted)
}

/// Whether a an array of attributes contains `contract`
pub fn has_contract_attribute(attrs: &[Attribute]) -> bool {
    attrs
        .iter()
        .any(|attr| attr.path.segments.len() == 1 && attr.path.segments[0].ident == "contract")
}

/// Extract the struct name from an item implementation
pub fn get_struct_name(item_impl: &ItemImpl) -> Option<String> {
    match &*item_impl.self_ty {
        syn::Type::Path(type_path) if !type_path.path.segments.is_empty() => {
            // Get the last segment of the path (the type name)
            let segment = type_path.path.segments.last().unwrap();
            Some(segment.ident.to_string())
        }
        _ => None,
    }
}

/// Extract a camel-cased contract identifier from a package name
pub fn get_contract_name(package_name: &str) -> String {
    let package_name = package_name.replace('_', "-");
    package_name
        .split('-')
        .map(|part| {
            if part.to_lowercase().starts_with("erc") {
                part.to_uppercase()
            } else {
                part.chars()
                    .enumerate()
                    .map(|(i, c)| {
                        if i == 0 {
                            c.to_uppercase().to_string()
                        } else {
                            c.to_string()
                        }
                    })
                    .collect::<String>()
            }
        })
        .collect::<String>()
}

/// Filter out the contract from the list of deplyable dependencies in the project
pub fn get_deployable_deps(
    project: &ContractProject,
    target: &ContractTarget,
) -> Vec<(String, bool)> {
    let mut dependencies = Vec::new();

    for (dep_name, same_project) in project.deployable_deps.iter() {
        // Skip self-reference
        if dep_name == &target.generated_package || dep_name == &target.module {
            continue;
        }

        dependencies.push((dep_name.clone(), *same_project));
    }

    dependencies
}

/// Format a TOML table as a string
pub fn format_toml_table(table: &Map<String, Value>) -> Result<String, CompileError> {
    if table.is_empty() {
        return Ok("{}".into());
    }

    let mut result = String::from("{ ");
    let mut first = true;

    for (k, v) in table {
        if !first {
            result.push_str(", ");
        }
        first = false;
        write!(
            result,
            "{} = {}",
            k,
            if let Value::Table(table) = v {
                format_toml_table(table)?
            } else {
                v.to_string()
            }
        )?;
    }

    result.push_str(" }");
    Ok(result)
}
