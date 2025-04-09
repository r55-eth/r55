#!/bin/bash

mkdir -p test_programs

# Create a hello world C program
cat > test_programs/hello.c << 'EOF'
#include <stdio.h>
int main() {
    printf("Hello, RISC-V world!\n");
    return 0;
}
EOF

# Compile it using RISC-V toolchain
# First make sure the compiler is installed
which riscv64-unknown-elf-gcc > /dev/null
if [ $? -ne 0 ]; then
    echo "Error: riscv64-unknown-elf-gcc not found. Please install the RISC-V toolchain."
    echo "For macOS: brew tap riscv-software-src/riscv && brew install riscv-gnu-toolchain"
    exit 1
fi

# Compile with specific flags suitable for our interpreter
# -march=rv64imac: Use RV64 with integer, multiply/divide, atomic, and compressed instructions
# -mabi=lp64: Long and pointer are 64-bit
# -static: Create statically linked executable (no dynamic linking)
# -nostdlib: Don't use standard system startup files or libraries
# -O0: No optimization to ensure readable assembly
# -g: Generate debug information
riscv64-unknown-elf-gcc -march=rv64imac -mabi=lp64 -static -O0 -g -o test_programs/hello test_programs/hello.c

echo "Test program created at test_programs/hello"
echo "Run it with: cargo run --example simple_execution -- test_programs/hello"
