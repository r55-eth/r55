use eth_riscv_interpreter::{
    RiscVInterpreter, 
    InterpreterError,
};

#[cfg(feature = "rvemu-backend")]
use eth_riscv_interpreter::RvEmuBackend;

#[cfg(feature = "unicorn-backend")]
use eth_riscv_interpreter::UnicornBackend;

use std::fs::read;
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: {} <path-to-elf-file>", args[0]);
        return Ok(());
    }
    
    // Read ELF file
    let elf_data = read(&args[1])?;
    
    // Create interpreter instance based on selected backend
    #[cfg(feature = "rvemu-backend")]
    let mut interpreter = RvEmuBackend::new();
    
    #[cfg(all(feature = "unicorn-backend", not(feature = "rvemu-backend")))]
    let mut interpreter = UnicornBackend::new()?;
    
    // Load ELF
    interpreter.load_elf(&elf_data)?;
    
    // Add syscall hook for syscall 1 (typically print/write)
    interpreter.add_syscall_hook(1, |interp, syscall_num| {
        println!("Syscall {} triggered", syscall_num);
        
        // Read arguments from registers
        let fd = interp.read_register(10)?;
        let buf_addr = interp.read_register(11)?;
        let buf_size = interp.read_register(12)? as usize;
        
        if fd == 1 || fd == 2 {  // stdout or stderr
            let buf = interp.read_memory(buf_addr, buf_size)?;
            let output = String::from_utf8_lossy(&buf);
            print!("{}", output);
            Ok(buf_size as u64)
        } else {
            Err(InterpreterError::SyscallError("Unsupported file descriptor".to_string()))
        }
    })?;
    
    // Start execution
    println!("Starting program execution...");
    let result = interpreter.start(0, 0);
    
    match result {
        Ok(_) => println!("Program executed successfully"),
        Err(e) => println!("Program execution failed: {}", e),
    }
    
    Ok(())
}