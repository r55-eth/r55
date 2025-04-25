use std::{collections::HashSet, fmt::Write};
use syn::{parse_file, File, Item, UseGroup, UseName, UsePath, UseTree};

use crate::types::CompileError;

pub fn process_lib(
    content: &str,
    contract_mods: &[&String],
) -> Result<(Vec<Item>, HashSet<String>), CompileError> {
    let file = parse_file(content)?;

    // Extract normalized imports
    let raw_imports = get_imports(&file)?;

    let mut lib_imports = HashSet::new();
    for import in raw_imports.into_iter() {
        _ = import.strip_prefix("crate::");
        if !contract_mods.iter().any(|m| import.starts_with(*m)) {
            lib_imports.insert(import);
        };
    }

    // Create a filtered version of the file
    let mut filtered_items = Vec::new();

    // Filter out contract module declarations and process other items
    for item in file.items.iter() {
        match item {
            // Check for module declarations
            Item::Mod(item_mod) => {
                let mod_name = item_mod.ident.to_string();

                // Skip module declarations for contract modules
                if contract_mods.iter().any(|m| **m == mod_name) {
                    continue;
                }

                // Skip generated deployable module (manually added when needed by the contract)
                if mod_name == "deployable" {
                    continue;
                }

                // Keep other module declarations
                filtered_items.push(item.clone());
            }

            // Skip all use statements as they are proccessed independently
            Item::Use(_) => continue,

            // Keep all other items
            _ => filtered_items.push(item.clone()),
        }
    }

    Ok((filtered_items, lib_imports))
}

pub fn process_contract_module(
    content: &str,
    lib_imports: &mut HashSet<String>,
) -> Result<Vec<Item>, CompileError> {
    let file = parse_file(content)?;

    // Create a filtered version of the file
    let mut filtered_items = Vec::new();

    // Extract normalized imports, and flatten them
    for import in get_imports(&file)?.into_iter() {
        let adj_import = match import.split_once("crate::") {
            Some((_, i)) => {
                if i == "*" || !i.contains("::") {
                    continue;
                } else {
                    i.into()
                }
            }
            None => match import.split_once("super::") {
                Some((_, i)) => {
                    if i == "*" || !i.contains("::") {
                        continue;
                    } else {
                        i.into()
                    }
                }
                None => import,
            },
        };

        lib_imports.insert(adj_import);
    }

    // Process each item
    for item in file.items.iter() {
        // Skip all use statements as they are proccessed independently
        if let Item::Use(_) = item {
            continue;
        }

        filtered_items.push(item.clone());
    }

    Ok(filtered_items)
}

pub fn flatten_lib(
    mut lib_ast: Vec<Item>,
    contract_mod_ast: Vec<Item>,
    imports: HashSet<String>,
) -> String {
    lib_ast.extend(contract_mod_ast);

    // Create the new file AST with the flattened items
    let file = File {
        shebang: None,
        items: lib_ast,
        attrs: Vec::new(),
    };

    // Craft the new file by defining the attributes, joining all imports, and the unparsed items
    let mut flattened = "#![no_std]\n#![no_main]\n\n".to_string();
    if imports.iter().any(|i| i.starts_with("deployable")) {
        flattened.push_str("mod deployable;\n");
    }
    for import in imports {
        _ = writeln!(flattened, "use {};", import);
    }

    format!("{}\n{}", flattened, prettyplease::unparse(&file))
}

fn get_imports(file: &File) -> Result<HashSet<String>, CompileError> {
    let mut imports = HashSet::new();

    // Iterate through all items in the file
    for item in file.items.iter() {
        if let Item::Use(use_item) = item {
            // Process each use statement
            process_use_tree(&mut imports, "", 0, &use_item.tree)?;
        }
    }

    Ok(imports)
}

fn process_use_tree(
    result: &mut HashSet<String>,
    prefix: &str,
    depth: usize,
    tree: &UseTree,
) -> Result<(), CompileError> {
    match tree {
        UseTree::Path(UsePath { ident, tree, .. }) => {
            let new_prefix = if depth == 0 {
                ident.to_string()
            } else {
                format!("{}::{}", prefix, ident)
            };
            process_use_tree(result, &new_prefix, depth + 1, tree)?;
        }
        UseTree::Group(UseGroup { items, .. }) => {
            for item in items.iter() {
                process_use_tree(result, prefix, depth + 1, item)?;
            }
        }
        UseTree::Name(UseName { ident, .. }) => {
            let path = if depth == 0 {
                ident.to_string()
            } else {
                format!("{}::{}", prefix, ident)
            };
            result.insert(path);
        }
        UseTree::Rename(rename) => {
            let path = if depth == 0 {
                format!("{} as {}", rename.ident, rename.rename)
            } else {
                format!("{}::{} as {}", prefix, rename.ident, rename.rename)
            };
            result.insert(path);
        }
        UseTree::Glob(_) => {
            let path = if depth == 0 {
                return Err(CompileError::InvalidImport(
                    "Global imports are not allowed",
                ));
            } else {
                format!("{}::*", prefix)
            };
            result.insert(path);
        }
    }

    Ok(())
}
