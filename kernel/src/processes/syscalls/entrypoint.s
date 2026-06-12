.global syscall_entry
syscall_entry:
    # SFMASK cleared IF on syscall entry — no cli needed here.
    mov rbx, rsp
    swapgs
    mov rsp, gs:[0]
    push rbx # Save caller RSP
    push rcx # Save caller RIP
    push r11 # Save caller RFLAGS
    sti      # Safe: on kernel stack with kernel GS-base

    # Move second argument back to RCX
    mov rcx, r8

    call syscall

    cli
    pop r11
    pop rcx
    pop rbx
    swapgs
    mov rsp, rbx
    # sysretq restores RFLAGS (including IF) from r11 atomically — no sti here.
    sysretq
