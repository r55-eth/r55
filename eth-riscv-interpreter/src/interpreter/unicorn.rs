use super::{InterpreterError, RiscVInterpreter};
use unicorn_engine::Unicorn;
use unicorn_engine::unicorn_const::{Arch, Mode, Permission, HookType, MemType};
use std::collections::HashMap;

pub struct UnicornBackend {
    emu: Unicorn<()>,
    syscall_hooks: HashMap<u64, Box<dyn FnMut(&mut Self, u64) -> Result<u64, InterpreterError>>>,
}

impl UnicornBackend {
    pub fn new() -> Result<Self, InterpreterError> {
        let emu = Unicorn::new(Arch::RISCV, Mode::RISCV64)
            .map_err(|e| InterpreterError::EmulationError(e.to_string()))?;
            
        Ok(Self {
            emu,
            syscall_hooks: HashMap::new(),
        })
    }
}

impl RiscVInterpreter for UnicornBackend {
    fn load_elf(&mut self, elf_data: &[u8]) -> Result<(), InterpreterError> {
        // Use goblin to parse ELF and load segments into Unicorn's memory
        let elf = goblin::elf::Elf::parse(elf_data)
            .map_err(|e| InterpreterError::ElfLoadError(e.to_string()))?;

        // Load program headers
        for ph in elf.program_headers.iter() {
            if ph.p_type == goblin::elf::program_header::PT_LOAD {
                let file_offset = ph.p_offset as usize;
                let file_size = ph.p_filesz as usize;
                let mem_addr = ph.p_vaddr;
                let mem_size = ph.p_memsz as usize;
                
                // Round up memory size to page size (4KB)
                let rounded_size = (mem_size + 0xFFF) & !0xFFF;
                
                // Map memory for the segment with appropriate permissions
                let perm = Permission::READ | Permission::WRITE;
                if ph.p_flags & 0x1 != 0 {
                    // Executable segment
                    self.emu.mem_map(mem_addr, rounded_size, perm | Permission::EXEC)
                        .map_err(|e| InterpreterError::MemoryError(e.to_string()))?;
                } else {
                    self.emu.mem_map(mem_addr, rounded_size, perm)
                        .map_err(|e| InterpreterError::MemoryError(e.to_string()))?;
                }
                
                // Write segment data to memory
                if file_size > 0 {
                    let segment_data = &elf_data[file_offset..file_offset + file_size];
                    self.emu.mem_write(mem_addr, segment_data)
                        .map_err(|e| InterpreterError::MemoryError(e.to_string()))?;
                }
            }
        }
        
        // Set PC to entry point
        self.emu.reg_write(unicorn_engine::unicorn_const::RegisterRISCV::PC as i32, elf.header.e_entry)
            .map_err(|e| InterpreterError::RegisterError(e.to_string()))?;
        
        Ok(())
    }
    
    fn read_register(&self, reg: u32) -> Result<u64, InterpreterError> {
        // Map from RISC-V register number to Unicorn register ID
        let reg_id = match reg {
            0..=31 => reg as i32, // x0-x31 have the same IDs
            _ => return Err(InterpreterError::RegisterError(format!("Invalid register: {}", reg))),
        };
        
        self.emu.reg_read(reg_id)
            .map_err(|e| InterpreterError::RegisterError(e.to_string()))
    }
    
    fn write_register(&mut self, reg: u32, value: u64) -> Result<(), InterpreterError> {
        // Map from RISC-V register number to Unicorn register ID
        let reg_id = match reg {
            0..=31 => reg as i32, // x0-x31 have the same IDs
            _ => return Err(InterpreterError::RegisterError(format!("Invalid register: {}", reg))),
        };
        
        self.emu.reg_write(reg_id, value)
            .map_err(|e| InterpreterError::RegisterError(e.to_string()))
    }
    
    fn read_memory(&self, address: u64, size: usize) -> Result<Vec<u8>, InterpreterError> {
        self.emu.mem_read(address, size)
            .map_err(|e| InterpreterError::MemoryError(e.to_string()))
    }
    
    fn write_memory(&mut self, address: u64, data: &[u8]) -> Result<(), InterpreterError> {
        self.emu.mem_write(address, data)
            .map_err(|e| InterpreterError::MemoryError(e.to_string()))
    }
    
    fn add_syscall_hook<F>(&mut self, syscall_num: u64, handler: F) -> Result<(), InterpreterError>
    where
        F: FnMut(&mut Self, u64) -> Result<u64, InterpreterError> + 'static,
    {
        // Store the handler for this syscall number
        self.syscall_hooks.insert(syscall_num, Box::new(handler));
        
        // If this is the first syscall hook, set up the instruction hook to catch ECALL instructions
        if self.syscall_hooks.len() == 1 {
            let self_ptr = self as *mut Self;
            
            // Hook on instruction to capture ECALL (environment call) instruction
            self.emu.add_insn_hook(
                move |uc, address, size| {
                    let self_ref = unsafe { &mut *self_ptr };
                    let mut buffer = [0u8; 4]; // RISC-V instructions are 4 bytes
                    
                    if let Ok(()) = uc.mem_read(address, &mut buffer) {
                        // Check if instruction is ECALL (0x73)
                        if buffer[0] == 0x73 && buffer[1] == 0x00 && buffer[2] == 0x00 && buffer[3] == 0x00 {
                            // Get syscall number from a7 register
                            if let Ok(syscall_num) = uc.reg_read(17) { // a7 is register 17
                                if let Some(hook) = self_ref.syscall_hooks.get_mut(&syscall_num) {
                                    match hook(self_ref, syscall_num) {
                                        Ok(ret_val) => {
                                            // Set return value in a0 register
                                            let _ = uc.reg_write(10, ret_val); // a0 is register 10
                                        },
                                        Err(_) => {
                                            // Halt emulation on error
                                            let _ = uc.emu_stop();
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                1, 0, // Hooks for all instructions
            ).map_err(|e| InterpreterError::SyscallError(e.to_string()))?;
        }
        
        Ok(())
    }
    
    fn start(&mut self, start_addr: u64, end_addr: u64) -> Result<(), InterpreterError> {
        // Unicorn's API requires an end address, if 0 is provided, use a very large value
        let real_end_addr = if end_addr == 0 {
            0xFFFF_FFFF_FFFF_FFFF
        } else {
            end_addr
        };
        
        self.emu.emu_start(start_addr, real_end_addr, 0, 0)
            .map_err(|e| InterpreterError::EmulationError(e.to_string()))
    }
}