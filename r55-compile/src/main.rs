use std::fs;
use std::path::Path;
use std::fs::File;
use std::io::Read;
use std::process::Command;
use tracing::{error, info};

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

    // Find all contract directories in `examples/`
    let examples_dir = project_root.join("examples");
    for entry in fs::read_dir(examples_dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_dir() {
            let contract_name = path.file_name().unwrap().to_str().unwrap();
            
            // Skip if not a contract project
            if !path.join("Cargo.toml").exists() {
                continue;
            }

            info!("Compiling contract: {}", contract_name);

            // Compile contract using existing working code
            let bytecode = compile_with_prefix(compile_deploy, path.to_str().unwrap())?;

            // Save bytecode file
            let bytecode_path = output_dir.join(format!("{}.bin", contract_name));
            fs::write(bytecode_path, bytecode)?;
        }
    }

    Ok(())
}

fn compile_runtime(path: &str) -> eyre::Result<Vec<u8>> {
    info!("Compiling runtime: {}", path);
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

    let path = format!(
        "{}/target/riscv64imac-unknown-none-elf/release/runtime",
        path
    );
    let mut file = match File::open(path) {
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

pub fn compile_deploy(path: &str) -> eyre::Result<Vec<u8>> {
    compile_runtime(path)?;
    info!("Compiling deploy: {}", path);
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

    let path = format!(
        "{}/target/riscv64imac-unknown-none-elf/release/deploy",
        path
    );
    let mut file = match File::open(path) {
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

pub fn compile_with_prefix<F>(compile_fn: F, path: &str) -> eyre::Result<Vec<u8>>
where
    F: FnOnce(&str) -> eyre::Result<Vec<u8>>,
{
    let bytecode = compile_fn(path)?;
    let mut prefixed_bytecode = vec![0xff]; // Add the 0xff prefix
    prefixed_bytecode.extend_from_slice(&bytecode);
    Ok(prefixed_bytecode)
}
