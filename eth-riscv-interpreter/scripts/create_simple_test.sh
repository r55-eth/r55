#!/bin/bash

mkdir -p test_programs

# Create a very simple assembly program that just exits
cat > test_programs/very_simple.s << 'EOF'
.section .text
.global _start
_start:
    # Set a known value in a register to check if execution worked
    li a0, 42
    
    # Exit with status code 0
    li a7, 93         # syscall number for exit
    ecall             # make the syscall

# Keep this small and simple
.section .data
dummy:
    .byte 0
EOF

# Create a simple assembly program that just prints "Hello" and exits
cat > test_programs/simple.s << 'EOF'
.section .text
.global _start
_start:
    # Print "Hello, RISC-V!\n"
    li a7, 64         # syscall number for write
    li a0, 1          # file descriptor 1 (stdout)
    la a1, message    # address of message
    li a2, 15         # length of message
    ecall             # make the syscall
    
    # Exit with status code 0
    li a7, 93         # syscall number for exit
    li a0, 0          # exit status
    ecall             # make the syscall

.section .data
message:
    .ascii "Hello, RISC-V!\n"
EOF

# Compile it using RISC-V toolchain with minimal instructions
which riscv64-unknown-elf-gcc > /dev/null
if [ $? -ne 0 ]; then
    echo "Error: riscv64-unknown-elf-gcc not found. Please install the RISC-V toolchain."
    echo "For macOS: brew tap riscv-software-src/riscv && brew install riscv-gnu-toolchain"
    exit 1
fi

# Compile with very basic RV64I instruction set only
# -march=rv64i: RV64 with only basic integer instructions (no extensions)
# -nostdlib: Don't use standard system startup files or libraries
# -static: Create statically linked executable
riscv64-unknown-elf-gcc -march=rv64i -mabi=lp64 -nostdlib -nostartfiles -static \
    -o test_programs/simple test_programs/simple.s

# Compile the very simple program too
riscv64-unknown-elf-gcc -march=rv64i -mabi=lp64 -nostdlib -nostartfiles -static \
    -o test_programs/very_simple test_programs/very_simple.s

echo "Simple test programs created at test_programs/simple and test_programs/very_simple"
echo "Run with: cargo run --example simple_execution -- test_programs/very_simple"
