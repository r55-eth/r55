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
