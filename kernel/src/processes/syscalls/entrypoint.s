.global syscall_entry
syscall_entry:
    cli
    mov rbx, rsp
    swapgs
    mov rsp, gs:[0]
    push rbx # Save caller RSP
    push rcx # Save caller RIP
    push r11 # Save caller RFLAGS
    sti

    # Move second argument back to RCX
    mov rcx, r8

    call syscall

    cli
    pop r11
    pop rcx
    pop rbx
    swapgs
    mov rsp, rbx
    sti
    
    sysretq
