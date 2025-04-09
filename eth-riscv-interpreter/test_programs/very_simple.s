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
