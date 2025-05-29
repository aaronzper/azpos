fn main() {
    let args: String = std::env::args().collect();

    // read env variables that were set in build script
    let uefi_path = env!("UEFI_PATH");
    let bios_path = env!("BIOS_PATH");
    
    // choose whether to start the UEFI or BIOS image
    let uefi = args.contains("-uefi");

    // whether to run QEMU with GDB
    let gdb = args.contains("-gdb");

    let mut cmd = std::process::Command::new("qemu-system-x86_64");
    cmd.arg("-m").arg("2G");
    cmd.arg("-serial").arg("stdio");

    let path = if uefi {
        cmd.arg("-bios").arg(ovmf_prebuilt::ovmf_pure_efi());
        uefi_path
    } else {
        bios_path
    };
    cmd.arg("-drive")
        .arg(format!("format=raw,file={path},if=ide,media=disk,index=0,"));

    if gdb {
        cmd.arg("-s").arg("-S");
    }

    let mut child = cmd.spawn().unwrap();
    child.wait().unwrap();
}
