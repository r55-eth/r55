use std::{
    collections::{HashMap, HashSet},
    fmt, fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};
use syn::Item;
use thiserror::Error;
use toml::Value;
use tracing::{debug, error, info};

use crate::helpers::{get_struct_name, has_contract_attribute};

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Parsing error: {0}")]
    SynError(#[from] syn::Error),
    #[error("Invalid TOML format: {0}")]
    TomlError(#[from] toml::de::Error),
    #[error("Formatting error: {0}")]
    FormattingError(#[from] std::fmt::Error),
    #[error("Invalid path: {0}")]
    PathError(String),
    #[error("No contract found in file: {0}")]
    NoContractFound(String),
    #[error("Cyclic dependency")]
    CyclicDependency,
    #[error("Invalid import: {0}")]
    InvalidImport(&'static str),
}

/// Represents a contract target within a project
#[derive(Debug, Clone)]
pub struct ContractTarget {
    /// The contract struct name
    pub ident: String,
    /// The module name where the contract is defined
    pub module: String,
    /// Path to the source file
    pub source_file: PathBuf,
    /// Generated package name
    pub generated_package: String,
}

/// Represents a source project that may contain multiple smart-contracts
#[derive(Debug, Clone)]
pub struct ContractProject {
    /// Directory path of the example
    pub path: PathBuf,
    /// Name of the project directory
    pub name: String,
    /// Contract targets within this project
    pub targets: Vec<ContractTarget>,
    /// Shared modules used by contracts
    pub shared_modules: HashSet<String>,
    /// Dependencies from `Cargo.toml`
    pub deps: HashMap<String, Value>,
    /// Deployable contract dependencies
    pub deployable_deps: HashMap<String, bool>,
}

/// Represents a generated (temporary) crate under `target/`
#[derive(Debug, Clone)]
pub struct GeneratedContract {
    /// Path to the generated crate
    pub path: PathBuf,
    /// Package name of the generated crate
    pub name: String,
    /// Deployable dependencies contracts
    pub deps: Vec<(String, bool)>,
    /// Original source file path
    pub original_source_path: PathBuf,
}

impl ContractTarget {
    pub fn is_self_reference(&self, dep_name: &String) -> bool {
        dep_name == &self.module || dep_name == &self.generated_package
    }
}

impl fmt::Display for GeneratedContract {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.deps.is_empty() {
            write!(f, "{}", self.name)
        } else {
            write!(f, "{} with deps: [", self.name)?;
            for (i, dep) in self.deps.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}", dep.0)?;
            }
            write!(f, "]")
        }
    }
}

impl GeneratedContract {
    pub fn get_original_name(&self, gen_mod_name: &String) -> String {
        let mut project_name = self
            .original_source_path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .replace("-", "_");
        project_name.push('_');

        match gen_mod_name.split_once(&project_name) {
            Some((_, name)) => name.into(),
            None => gen_mod_name.into(),
        }
    }

    pub fn compile(&self) -> eyre::Result<Vec<u8>> {
        // First compile runtime
        self.compile_runtime()?;

        // Then compile deployment code
        let bytecode = self.compile_deploy()?;
        let mut prefixed_bytecode = vec![0xff]; // Add the 0xff prefix
        prefixed_bytecode.extend_from_slice(&bytecode);

        Ok(prefixed_bytecode)
    }

    fn compile_runtime(&self) -> eyre::Result<Vec<u8>> {
        debug!("Compiling runtime: {}", self.name);

        let path = self
            .path
            .to_str()
            .ok_or_else(|| eyre::eyre!("Failed to convert path to string: {:?}", self.path))?;

        let status = Command::new("cargo")
            .arg("+nightly-2025-01-07")
            .arg("build")
            .arg("-r")
            .arg("--lib")
            .arg("-Z")
            .arg("build-std=core,alloc")
            .arg("--target")
            .arg("riscv64imac-unknown-none-elf")
            .arg("--bin")
            .arg("runtime")
            .current_dir(path)
            .status()
            .expect("Failed to execute cargo command");

        if !status.success() {
            error!("Cargo command failed with status: {}", status);
            std::process::exit(1);
        } else {
            info!("Cargo command completed successfully");
        }

        let bin_path = PathBuf::from(path)
            .join("target")
            .join("riscv64imac-unknown-none-elf")
            .join("release")
            .join("runtime");

        let mut file = fs::File::open(&bin_path).map_err(|e| {
            eyre::eyre!(
                "Failed to open runtime binary {}: {}",
                bin_path.display(),
                e
            )
        })?;

        // Read the file contents into a vector
        let mut bytecode = Vec::new();
        file.read_to_end(&mut bytecode)
            .map_err(|e| eyre::eyre!("Failed to read runtime binary: {}", e))?;

        Ok(bytecode)
    }

    // Requires previous runtime compilation
    fn compile_deploy(&self) -> eyre::Result<Vec<u8>> {
        debug!("Compiling deploy: {}", self.name);

        let path = self
            .path
            .to_str()
            .ok_or_else(|| eyre::eyre!("Failed to convert path to string: {:?}", self.path))?;

        let status = Command::new("cargo")
            .arg("+nightly-2025-01-07")
            .arg("build")
            .arg("-r")
            .arg("--lib")
            .arg("-Z")
            .arg("build-std=core,alloc")
            .arg("--target")
            .arg("riscv64imac-unknown-none-elf")
            .arg("--bin")
            .arg("deploy")
            .arg("--features")
            .arg("deploy")
            .current_dir(path)
            .status()
            .expect("Failed to execute cargo command");

        if !status.success() {
            error!("Cargo command failed with status: {}", status);
            std::process::exit(1);
        } else {
            info!("Cargo command completed successfully");
        }

        let bin_path = PathBuf::from(path)
            .join("target")
            .join("riscv64imac-unknown-none-elf")
            .join("release")
            .join("deploy");

        let mut file = fs::File::open(&bin_path).map_err(|e| {
            eyre::eyre!("Failed to open deploy binary {}: {}", bin_path.display(), e)
        })?;

        // Read the file contents into a vector
        let mut bytecode = Vec::new();
        file.read_to_end(&mut bytecode)
            .map_err(|e| eyre::eyre!("Failed to read deploy binary: {}", e))?;

        Ok(bytecode)
    }
}

impl ContractProject {
    pub fn try_from_path(cargo_toml_path: &Path) -> Result<ContractProject, CompileError> {
        let project_dir = cargo_toml_path.parent().ok_or_else(|| {
            CompileError::PathError(format!(
                "Failed to get parent directory of {:?}",
                cargo_toml_path
            ))
        })?;

        let project_name = project_dir
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| {
                CompileError::PathError(format!(
                    "Failed to get directory name from {:?}",
                    project_dir
                ))
            })?
            .to_string();

        // Parse Cargo.toml
        let cargo_content = fs::read_to_string(cargo_toml_path)?;
        let cargo_toml: Value = toml::from_str(&cargo_content)?;

        // Extract base package name
        let base_package_name = cargo_toml
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .ok_or_else(|| {
                CompileError::PathError("Missing package.name in Cargo.toml".to_string())
            })?
            .to_string();

        // Extract dependencies
        let mut deps = HashMap::new();
        let mut deployable_deps = HashMap::new();

        if let Some(Value::Table(table_deps)) = cargo_toml.get("dependencies") {
            for (name, details) in table_deps {
                deps.insert(name.to_owned(), details.to_owned());
            }
        }

        if let Some(package_table) = cargo_toml.get("package").and_then(Value::as_table) {
            if let Some(metadata_table) = package_table.get("metadata").and_then(Value::as_table) {
                if let Some(deployable_deps_table) = metadata_table
                    .get("deployable_deps")
                    .and_then(Value::as_table)
                {
                    for (name, details) in deployable_deps_table {
                        if let Some(dep_table) = details.as_table() {
                            let same_project = match dep_table.get("path") {
                                Some(Value::String(rel_path)) => rel_path == ".",
                                _ => false,
                            };

                            if same_project {
                                deployable_deps.insert(format!("{}-{}", project_name, name), true)
                            } else {
                                deployable_deps.insert(name.clone(), false)
                            };
                        }
                    }
                }
            }
        }

        // Scan src directory for contract targets and shared modules
        let src_dir = project_dir.join("src");
        let lib_rs_path = src_dir.join("lib.rs");

        if !lib_rs_path.exists() {
            return Err(CompileError::PathError(format!(
                "Missing src/lib.rs in {:?}",
                project_dir
            )));
        }

        // Parse lib.rs to find module declarations
        let lib_content = fs::read_to_string(&lib_rs_path)?;
        let lib_ast = syn::parse_file(&lib_content)?;

        let mut module_names = HashSet::new();
        for item in &lib_ast.items {
            if let Item::Mod(item_mod) = item {
                if item_mod.content.is_none() {
                    // External module
                    module_names.insert(item_mod.ident.to_string());
                }
            }
        }

        // Find contract targets in each module
        let mut targets = Vec::new();
        let mut shared_modules = HashSet::new();

        for module_name in &module_names {
            let module_path = src_dir.join(format!("{}.rs", module_name));

            if !module_path.exists() {
                // Try module/mod.rs structure
                let alt_module_path = src_dir.join(module_name).join("mod.rs");
                if !alt_module_path.exists() {
                    debug!(
                        "Could not find module file for {} at {:?} or {:?}",
                        module_name, module_path, alt_module_path
                    );
                    continue;
                }
            }

            // Parse the module file
            let module_content = fs::read_to_string(&module_path)?;
            let module_ast = syn::parse_file(&module_content)?;

            // Look for `#[contract]` annotation on impl blocks
            let mut has_contract = false;
            for item in &module_ast.items {
                if let Item::Impl(item_impl) = item {
                    if has_contract_attribute(&item_impl.attrs) {
                        // Found a contract
                        if let Some(struct_name) = get_struct_name(item_impl) {
                            has_contract = true;

                            // Generate package name based on project name and module name
                            let generated_pkg_name = format!("{}-{}", project_name, module_name);

                            targets.push(ContractTarget {
                                ident: struct_name,
                                module: module_name.clone(),
                                source_file: module_path.clone(),
                                generated_package: generated_pkg_name,
                            });
                        }
                    }
                }
            }

            if !has_contract {
                // If no contract was found, this is a shared module
                shared_modules.insert(module_name.clone());
            }
        }

        if targets.is_empty() {
            // No contracts found - try to find a contract in `lib.rs`
            for item in &lib_ast.items {
                if let Item::Impl(item_impl) = item {
                    if has_contract_attribute(&item_impl.attrs) {
                        if let Some(struct_name) = get_struct_name(item_impl) {
                            // When contract is in lib.rs, use the project name as the generated name
                            targets.push(ContractTarget {
                                ident: struct_name,
                                module: String::new(),
                                source_file: lib_rs_path.clone(),
                                generated_package: base_package_name.clone(),
                            });
                        }
                    }
                }
            }
        }

        if targets.is_empty() {
            return Err(CompileError::NoContractFound(
                project_dir.to_string_lossy().into(),
            ));
        }

        Ok(ContractProject {
            path: project_dir.to_path_buf(),
            name: project_name,
            targets,
            shared_modules,
            deps,
            deployable_deps,
        })
    }
}
