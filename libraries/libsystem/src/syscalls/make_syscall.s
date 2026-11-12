.global make_syscall
.global make_syscall_raw
# `make_syscall_raw` takes the syscall number as an integer.
make_syscall:
make_syscall_raw:
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15

    # Normally, the 4th argument is in RCX, but we use R8 (the 5th) since
    # syscall uses RCX to store the caller's RIP
    mov r8, rcx

    syscall

    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp

    ret
