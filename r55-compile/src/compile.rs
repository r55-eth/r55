use std::{collections::HashMap, fmt, fs, io::Read, path::{Path, PathBuf}, process::Command};
use tracing::{debug, error, info, warn};
use toml::Value;
use thiserror::Error;
use syn::{Item, ItemImpl, Attribute};

#[derive(Debug, Error)]
pub enum ContractError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Invalid TOML format")]
    NotToml,
    #[error("Missing required dependencies")]
    MissingDependencies,
    #[error("Missing required binaries")]
    MissingBinaries,
    #[error("Missing required features")]
    MissingFeatures,
    #[error("Invalid path")]
    WrongPath,
    #[error("Cyclic dependency")]
    CyclicDependency
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContractName {
    pub package: String,
    pub ident: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Contract {
    pub path: PathBuf,
    pub name: ContractName,
}

#[derive(Debug, Clone)]
pub struct ContractWithDeps {
    pub path: PathBuf,
    pub name: ContractName,
    pub deps: Vec<Contract>,
}

// Implement Display for Contract
impl fmt::Display for Contract {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name.ident)
    }
}

// Implement Display for ContractWithDeps
impl fmt::Display for ContractWithDeps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.deps.is_empty() {
            write!(f, "{}", self.name.ident)
        } else {
            write!(f, "{} with deps: [", self.name.ident)?;
            for (i, dep) in self.deps.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}", dep.name.ident)?;
            }
            write!(f, "]")
        }
    }
}

impl Into<Contract> for ContractWithDeps {
    fn into(self) -> Contract {
        Contract {
            name: self.name,
            path: self.path
        }
    }
} 

impl TryFrom<&PathBuf> for ContractWithDeps {
    type Error = ContractError;

    fn try_from(cargo_toml_path: &PathBuf) -> Result<Self, Self::Error> {
        let parent_dir = cargo_toml_path.parent().ok_or(ContractError::NotToml)?;
        let content = fs::read_to_string(cargo_toml_path)?;
        let cargo_toml = content
            .parse::<Value>()
            .map_err(|_| ContractError::NotToml)?;

        // Get package name
        let name = cargo_toml
            .get("package")
            .and_then(|f| f.get("name"))
            .ok_or(ContractError::NotToml)?
            .as_str()
            .ok_or(ContractError::NotToml)?
            .to_string();

        // Check for required features
        let has_features = match &cargo_toml.get("features") {
            Some(Value::Table(feat)) => {
                feat.contains_key("default")
                    && feat.contains_key("deploy")
                    && feat.contains_key("interface-only")
            }
            _ => false,
        };

        if !has_features {
            return Err(ContractError::MissingFeatures);
        }

        // Check for required binaries
        let has_required_bins = match &cargo_toml.get("bin") {
            Some(Value::Array(bins)) => {
                let mut has_runtime = false;
                let mut has_deploy = false;

                for bin in bins {
                    if let Value::Table(bin_table) = bin {
                        if let Some(Value::String(name)) = bin_table.get("name") {
                            if name == "runtime"
                                && bin_table.get("path").and_then(|p| p.as_str())
                                    == Some("src/lib.rs")
                            {
                                has_runtime = true;
                            } else if name == "deploy"
                                && bin_table.get("path").and_then(|p| p.as_str())
                                    == Some("src/lib.rs")
                                && bin_table
                                    .get("required-features")
                                    .map(|f| match f {
                                        Value::String(s) => s == "deploy",
                                        Value::Array(arr) => {
                                            arr.contains(&Value::String("deploy".to_string()))
                                        }
                                        _ => false,
                                    })
                                    .unwrap_or(false)
                            {
                                has_deploy = true;
                            }
                        }
                    }
                }

                has_runtime && has_deploy
            }
            _ => false,
        };

        if !has_required_bins {
            return Err(ContractError::MissingBinaries);
        }

        // Get package dependencies
        let mut contract_deps = Vec::new();
        if let Some(Value::Table(deps)) = cargo_toml.get("dependencies") {
            // Ensure required dependencies
            if !(deps.contains_key("contract-derive") && deps.contains_key("eth-riscv-runtime")) {
                return Err(ContractError::MissingDependencies);
            }

            for (name, dep) in deps {
                if let Value::Table(dep_table) = dep {
                    // Ensure "interface-only" feature
                    let has_interface_only = match dep_table.get("features") {
                        Some(Value::Array(features)) => {
                            features.contains(&Value::String("interface-only".to_string()))
                        }
                        _ => false,
                    };

                    if !has_interface_only {
                        continue;
                    }

                    // Ensure local path
                    if let Some(Value::String(rel_path)) = dep_table.get("path") {
                        let path = parent_dir
                            .join(rel_path)
                            .canonicalize()
                            .map_err(|_| ContractError::WrongPath)?;
                        contract_deps.push(Contract {
                            name: ContractName { ident: String::new(), package: name.to_owned() },
                            path,
                        });
                    }
                }
            }
        }

        let contract = Self {
            name: ContractName { ident: String::new(), package: name },
            deps: contract_deps,
            path: parent_dir.to_owned(),
        };

        Ok(contract)
    }
}

impl Contract {
    pub fn path_str(&self) -> eyre::Result<&str> {
        self.path.to_str().ok_or_else(|| eyre::eyre!("Failed to convert path to string {:?}", self.path))
    }

    pub fn compile_r55(&self) -> eyre::Result<Vec<u8>> {
        // First compile runtime
        self.compile_runtime()?;

        // Then compile deployment code 
        let bytecode = self.compile_deploy()?;
        let mut prefixed_bytecode = vec![0xff]; // Add the 0xff prefix
        prefixed_bytecode.extend_from_slice(&bytecode);

        Ok(prefixed_bytecode)
    }

    fn compile_runtime(&self) -> eyre::Result<Vec<u8>> {
        debug!("Compiling runtime: {}", self.name.package);

        let path = self.path_str()?;
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

        let path = format!("{}/target/riscv64imac-unknown-none-elf/release/runtime", path);
        let mut file = match fs::File::open(path) {
            Ok(file) => file,
            Err(e) => {
                eyre::bail!("Failed to open file: {}", e);
            }
        };

        // Read the file contents into a vector.
        let mut bytecode = Vec::new();
        if let Err(e) = file.read_to_end(&mut bytecode) {
            eyre::bail!("Failed to read file: {}", e);
        }

        Ok(bytecode)
    }

    // Requires previous runtime compilation
    fn compile_deploy(&self) -> eyre::Result<Vec<u8>> {
        debug!("Compiling deploy: {}", self.name.package);

        let path = self.path_str()?;
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

        let path = format!("{}/target/riscv64imac-unknown-none-elf/release/deploy", path);
        let mut file = match fs::File::open(path) {
            Ok(file) => file,
            Err(e) => {
                eyre::bail!("Failed to open file: {}", e);
            }
        };

        // Read the file contents into a vector.
        let mut bytecode = Vec::new();
        if let Err(e) = file.read_to_end(&mut bytecode) {
            eyre::bail!("Failed to read file: {}", e);
        }

        Ok(bytecode)
    }
}

pub fn find_r55_contracts(dir: &Path) -> HashMap<bool, Vec<ContractWithDeps>> {
    let mut contracts: HashMap<bool, Vec<ContractWithDeps>> = HashMap::new();

    // Only scan direct subdirectories of given directory
    let mut temp_contracts = Vec::new();
    let mut temp_idents = HashMap::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();

            // Skip if not a directory
            if !path.is_dir() {
                continue;
            }

            // Check for Cargo.toml
            let cargo_path = path.join("Cargo.toml");
            if !cargo_path.exists() {
                continue;
            }

            // Try to parse as R55 contract
            match ContractWithDeps::try_from(&cargo_path) {
                Ok(contract) => {
                    let lib_path = contract.path.join("src").join("lib.rs");
                    let ident = match find_contract_ident(&lib_path) {
                        Ok(ident) => ident,
                        Err(e) => {
                            error!("Unable to find contract identifier at {:?}: {:?}", lib_path, e);
                            continue;
                        }
                    };
                    debug!("Found R55 contract: ({} with ident: {}) at {:?}", contract.name.package, ident, contract.path);
                    temp_idents.insert(contract.path.to_owned(), ident);
                    temp_contracts.push(contract);
                }
                Err(ContractError::MissingDependencies) => continue,
                Err(ContractError::MissingBinaries) => continue,
                Err(ContractError::MissingFeatures) => continue,
                Err(e) => warn!("Error parsing potential contract at {:?}: {:?}", cargo_path, e),
            }
        }

        // Set the identifiers for the contract and its dependencies 
        for mut contract in temp_contracts {
            if let Some(ident) = temp_idents.get(&contract.path) {
                contract.name.ident = ident.to_owned();
            }
            
            for dep in &mut contract.deps {
                if let Some(ident) = temp_idents.get(&dep.path) {
                    dep.name.ident = ident.to_owned();
                }
            }

            contracts.entry(contract.deps.is_empty()).or_default().push(contract)
        }
    }

    contracts
}

pub fn sort_r55_contracts(mut map: HashMap<bool, Vec<ContractWithDeps>>) -> Result<Vec<Contract>, ContractError> {
    // Add contracts without dependencies to the compilation queue
    let mut queue: Vec<Contract> = match map.remove(&true) {
        Some(contracts) => contracts.into_iter().map(|c| c.into()).collect(),
        None => vec![]
    };
    debug!("{} Contracts without deps", queue.len());

    // Contracts with dependencies can only be added when their dependencies are already in the queue 
    let mut pending = map.remove(&false).unwrap_or_default();
    debug!("{} Contracts with deps", pending.len());

        while !pending.is_empty() {
            let prev_pending = pending.len();

            let mut next_pending = Vec::new();
            for p in pending.into_iter() {
               if all_handled(&p.deps, &queue) {
                    queue.push(p.to_owned().into());
                } else {
                    next_pending.push(p);
                }
            }
            pending = next_pending;

            // If no contracts were processed, there is a cyclical dependency
            if prev_pending == pending.len() {
                return Err(ContractError::CyclicDependency);
            }
        }

    Ok(queue)
}

fn all_handled(deps: &[Contract], handled: &[Contract]) -> bool {
    for d in deps {
        if !handled.contains(d) {
            return false;
        } 
    }

    true
}

pub fn find_contract_ident(file_path: &Path) -> eyre::Result<String> {
    // Read and parse the file content
    let content = fs::read_to_string(file_path)?;
    let file = syn::parse_file(&content)?;
    
    // Look for impl blocks with #[contract] attribute
    for item in file.items {
        if let Item::Impl(item_impl) = item {
            // Check if this impl block has the #[contract] attribute
            if has_contract_attribute(&item_impl.attrs) {
                // Extract the type name from the impl block
                if let Some(ident) = extract_ident(&item_impl) {
                    return Ok(ident);
                }
            }
        }
    }
    
    eyre::bail!("No contract implementation found in file: {:?}", file_path)
}

// Check if attributes contain #[contract]
fn has_contract_attribute(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path.segments.len() == 1 && attr.path.segments[0].ident == "contract"
    })
}

// Extract the type name from its impl block
fn extract_ident(item_impl: &ItemImpl) -> Option<String> {
    match &*item_impl.self_ty {
        syn::Type::Path(type_path) if !type_path.path.segments.is_empty() => {
            // Get the last segment of the path (the type name)
            let segment = type_path.path.segments.last().unwrap();
            Some(segment.ident.to_string())
        },
        _ => None,
    }
}
