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

// Function to print a hex dump of memory
fn hex_dump(data: &[u8], addr: u64) {
    const BYTES_PER_LINE: usize = 16;
    
    for (i, chunk) in data.chunks(BYTES_PER_LINE).enumerate() {
        print!("{:08x}: ", addr + (i * BYTES_PER_LINE) as u64);
        
        for b in chunk {
            print!("{:02x} ", b);
        }
        
        // Print ASCII representation
        print!("  ");
        for &b in chunk {
            if b >= 32 && b <= 126 {
                // Printable ASCII
                print!("{}", b as char);
            } else {
                // Non-printable
                print!(".");
            }
        }
        println!();
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: {} <path-to-elf-file>", args[0]);
        return Ok(());
    }
    
    // Read ELF file
    println!("Reading ELF file: {}", args[1]);
    let elf_data = read(&args[1])?;
    println!("ELF file size: {} bytes", elf_data.len());
    
    // Parse ELF header to get entry point
    let elf = goblin::elf::Elf::parse(&elf_data)?;
    println!("ELF Entry point: 0x{:x}", elf.header.e_entry);

    // Add debug information about the ELF file
    println!("ELF Program Headers:");
    for (i, ph) in elf.program_headers.iter().enumerate() {
        if ph.p_type == goblin::elf::program_header::PT_LOAD {
            println!("  PH#{}: LOAD - VA: 0x{:x}, PA: 0x{:x}, Size: 0x{:x}, Flags: {}{}{}", 
                    i, 
                    ph.p_vaddr, 
                    ph.p_paddr, 
                    ph.p_memsz,
                    if ph.p_flags & 0x4 != 0 { "R" } else { "-" },
                    if ph.p_flags & 0x2 != 0 { "W" } else { "-" },
                    if ph.p_flags & 0x1 != 0 { "X" } else { "-" });
        }
    }
    
    // Create interpreter instance based on selected backend
    #[cfg(feature = "rvemu-backend")]
    let mut interpreter = RvEmuBackend::new();
    
    #[cfg(all(feature = "unicorn-backend", not(feature = "rvemu-backend")))]
    let mut interpreter = UnicornBackend::new()?;
    
    // Load ELF
    println!("Loading ELF into interpreter...");
    interpreter.load_elf(&elf_data)?;
    
    // Add syscall hook for syscall 1 (write)
    println!("Adding syscall hook for write(2)...");
    interpreter.add_syscall_hook(64, |interp, syscall_num| {  // 64 is write in RISCV
        println!("Syscall {} triggered (write)", syscall_num);
        
        // Read arguments from registers (RISC-V calling convention)
        let fd = interp.read_register(10)?;  // a0 = fd
        let buf_addr = interp.read_register(11)?;  // a1 = buffer address
        let buf_size = interp.read_register(12)? as usize;  // a2 = buffer size
        
        println!("  fd: {}, buf_addr: 0x{:x}, size: {}", fd, buf_addr, buf_size);
        
        if fd == 1 || fd == 2 {  // stdout or stderr
            // Read string from memory
            let buf = interp.read_memory(buf_addr, buf_size)?;
            
            // Print hex dump for debugging
            println!("Buffer contents:");
            hex_dump(&buf, buf_addr);
            
            // Convert to string and print
            match std::str::from_utf8(&buf) {
                Ok(s) => println!("Output: {}", s),
                Err(_) => println!("Output: (contains non-UTF8 data)"),
            }
            
            // Return bytes written
            Ok(buf_size as u64)
        } else {
            Err(InterpreterError::SyscallError(format!("Unsupported file descriptor: {}", fd)))
        }
    })?;
    
    // Add syscall hook for syscall 93 (exit)
    println!("Adding syscall hook for exit(93)...");
    interpreter.add_syscall_hook(93, |_interp, syscall_num| {
        println!("Syscall {} triggered (exit)", syscall_num);
        // Just return 0 since we don't want to actually exit the process
        Ok(0)
    })?;

    // Add more syscalls
    println!("Adding hook for basic syscalls...");
    // brk syscall (used by C runtime)
    interpreter.add_syscall_hook(214, |_interp, syscall_num| {
        println!("Syscall {} triggered (brk)", syscall_num);
        Ok(0)
    })?;
    
    // Add a hook for fstat syscall
    interpreter.add_syscall_hook(80, |_interp, syscall_num| {
        println!("Syscall {} triggered (fstat)", syscall_num);
        Ok(0)
    })?;
    
    // Start execution at the entry point with debug info
    println!("Starting program execution at address: 0x{:x}...", elf.header.e_entry);
    let result = interpreter.start(elf.header.e_entry, 0);
    
    match result {
        Ok(_) => println!("Program executed successfully"),
        Err(e) => println!("Program execution failed: {}", e),
    }
    
    Ok(())
}