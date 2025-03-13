mod compile;
use compile::{compile_deploy, compile_with_prefix};

use std::{
    fmt,
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use toml::Value;
use tracing::{debug, info, warn};
use thiserror::Error;

#[derive(Debug, Error)]
enum ContractError {
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
    #[error("Cyclic dependency detected")]
    CyclicDependency
}

#[derive(Debug, Clone, PartialEq)]
struct Contract {
    name: String,
    path: PathBuf,
}

#[derive(Debug, Clone)]
struct ContractWithDeps {
    name: String,
    path: PathBuf,
    deps: Vec<Contract>,
}

// Implement Display for Contract
impl fmt::Display for Contract {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

// Implement Display for ContractWithDeps
impl fmt::Display for ContractWithDeps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.deps.is_empty() {
            write!(f, "{}", self.name)
        } else {
            write!(f, "{} with deps: [", self.name)?;
            for (i, dep) in self.deps.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}", dep.name)?;
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
                            name: name.into(),
                            path,
                        });
                    }
                }
            }
        }

        let contract = Self {
            name,
            deps: contract_deps,
            path: parent_dir.to_owned(),
        };

        Ok(contract)
    }
}

fn find_r55_contracts(examples_dir: &Path) -> HashMap<bool, Vec<ContractWithDeps>> {
    let mut contracts = HashMap::new();

    // TODO: discuss with Leo and Georgios how r55 integration for SC development should look like
    // Only scan direct subdirectories of examples_dir
    if let Ok(entries) = fs::read_dir(examples_dir) {
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
                    debug!("Found R55 contract: {} at {:?}", contract.name, contract.path);
                    contracts.entry(contract.deps.len() == 0).or_insert_with(Vec::new).push(contract);
                }
                Err(ContractError::MissingDependencies) => continue,
                Err(ContractError::MissingBinaries) => continue,
                Err(ContractError::MissingFeatures) => continue,
                Err(e) => warn!("Error parsing potential contract at {:?}: {:?}", cargo_path, e),
            }
        }
    }

    contracts
}

fn all_handled(deps: &Vec<Contract>, handled: &Vec<Contract>) -> bool {
    for d in deps {
        if !handled.contains(d) {
            return false;
        } 
    }

    true
}

fn sort_contracts(mut map: HashMap<bool, Vec<ContractWithDeps>>) -> Result<Vec<Contract>, ContractError> {
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

fn main() -> eyre::Result<()> {
    // Initialize logging
    let tracing_sub = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(false)
        .finish();
    tracing::subscriber::set_global_default(tracing_sub)?;

    // Setup output directory
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let output_dir = project_root.join("r55-output-bytecode");
    fs::create_dir_all(&output_dir)?;

    // Find all R55 contracts in examples directory
    let examples_dir = project_root.join("examples");
    let contracts = find_r55_contracts(&examples_dir);
    let contracts = sort_contracts(contracts)?;

    info!(
        "Found {} R55 contracts (in compilation order):\n{}",
        contracts.len(),
        contracts.iter()
            .enumerate()
            .map(|(i, c)| format!("  {}. {}", i+1, c))
            .collect::<Vec<_>>()
            .join("\n")
    ); 

    // Compile each contract
    for contract in contracts {
        info!("Compiling contract: {}", contract.name);

        // Get the contract directory path as a string
        let contract_path = contract
            .path
            .to_str()
            .ok_or_else(|| eyre::eyre!("Failed to convert path to string: {:?}", contract.path))?;

        // Compile initcode and save in the file
        let deploy_bytecode = compile_with_prefix(compile_deploy, contract_path)?;
        let deploy_path = output_dir.join(format!("{}.bin", contract.name));
        fs::write(deploy_path, deploy_bytecode)?;
    }

    Ok(())
}

