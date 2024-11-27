use rvemu::{bus::DRAM_BASE, dram::DRAM_SIZE, emulator::Emulator};

mod error;
pub use error::{Error, Result};

pub fn setup_from_elf(elf_data: &[u8], call_data: &[u8]) -> Result<Emulator> {
    let elf = goblin::elf::Elf::parse(elf_data)?;

    // Allocate 1MB for the call data
    let mut mem = vec![0; 1024 * 1024];
    {
        assert!(call_data.len() < mem.len() - 8);

        let (size_bytes, data_bytes) = mem.split_at_mut(8);
        size_bytes.copy_from_slice(&(call_data.len() as u64).to_le_bytes());
        data_bytes[..call_data.len()].copy_from_slice(call_data);
    }

    load_sections(&mut mem, &elf, elf_data);

    let mut emu = Emulator::new();

    emu.initialize_dram(mem);
    emu.initialize_pc(elf.header.e_entry);

    Ok(emu)
}

fn load_sections(mem: &mut Vec<u8>, elf: &goblin::elf::Elf, elf_data: &[u8]) {
    for ph in &elf.program_headers {
        if ph.p_type == goblin::elf::program_header::PT_LOAD {
            // The interpreter RAM is DRAM_SIZE starting at DRAM_BASE
            assert!(ph.p_vaddr >= DRAM_BASE);
            assert!(ph.p_memsz <= DRAM_SIZE);

            let start_vec = (ph.p_vaddr - DRAM_BASE) as usize;
            let start_offset = ph.p_offset as usize;

            let end_vec = start_vec + ph.p_memsz as usize;
            if mem.len() < end_vec {
                mem.resize(end_vec, 0);
            }

            // The data available to copy may be smaller than the required size
            let size_to_copy = ph.p_filesz as usize;
            mem[start_vec..(start_vec + size_to_copy)]
                .copy_from_slice(&elf_data[start_offset..(start_offset + size_to_copy)]);
        }
    }
}

#[cfg(test)]
mod tests {
    use rvemu::exception::Exception;

    use super::*;
    use std::fs;
    use std::process::Command;

    #[test]
    fn test_execute_elf() -> eyre::Result<()> {
        let runtime_path = "../asm-runtime-example/runtime";

        // Check if the runtime ELF exists; if not, run `make` to generate it
        if !fs::metadata(runtime_path).is_ok() {
            let status = Command::new("make")
                .current_dir("../asm-runtime-example")
                .status()?;

            // Check if the `make` command succeeded
            if !status.success() {
                panic!("`make` command failed. Please check if the prerequisites are installed.");
            }
        }

        let elf_data = fs::read(runtime_path)?;
        let mut emu = setup_from_elf(&elf_data, &[])?;
        let result = emu.start();
        assert_eq!(result, Err(Exception::EnvironmentCallFromMMode));
        let t0 = emu.cpu.xregs.read(5);
        let a0 = emu.cpu.xregs.read(10);
        let a1 = emu.cpu.xregs.read(11);
        // t0: 0, opcode for return, a0: memory address of data, a1: length of data, in bytes
        assert!(t0 == 0); // return opcode
        assert_eq!(a1, 8); // data returned should be a little-endian u64
        let data_bytes = emu.cpu.bus.get_dram_slice(a0..(a0 + a1)).unwrap();

        let data = u64::from_le_bytes(data_bytes.try_into()?);
        assert_eq!(data, 5);

        Ok(())
    }
}
