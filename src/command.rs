use std::collections::HashMap;
use std::time::SystemTime;
use log::{debug, error, info};
use pest::iterators::{Pair, Pairs};
use pollster::FutureExt;
use uuid::{Error, Uuid};
use crate::parser::Rule;
use crate::session::SessionData;
use crate::storage::SessionStore;

#[derive(Debug)]
pub enum Command {
    SessionShow(Option<u128>),
    SessionCreateAlias {
        session: u128,
        alias: String,
    },
    SessionOpen(u128),
    SessionRemove(u128),
    FlashFirmware(Option<String>),
    Help(HelpCommand),
}

#[derive(Debug)]
pub enum HelpCommand {
    All,
    Session,
}

#[derive(Debug, Clone)]
struct Token {
    rule: Rule,
    value: String,
}

impl From<Pair<'_, Rule>> for Token {
    fn from(value: Pair<Rule>) -> Self {
        Self {
            rule: value.as_rule(),
            value: value.as_str().to_string(),
        }
    }
}

impl Command {
    pub fn from_pairs(pairs: Pairs<Rule>, session_store: &SessionStore, alias_store: &HashMap<String, u128>) -> Result<Self, String> {
        let rules = pairs.flatten().map(Token::from).collect::<Vec<_>>();
        let rules = rules.iter().collect::<Vec<_>>();
        match rules.as_slice() {
            [Token { rule: Rule::session_command, .. }, Token { rule: Rule::session_op_show, .. }, Token { rule: Rule::EOI, .. }] => {
                Ok(Self::SessionShow(None))
            }
            [Token { rule: Rule::session_command, .. }, Token { rule: Rule::object, value }, other @ ..] => {
                let uuid = Self::object_to_uuid(value, session_store, alias_store).ok_or(format!("Invalid/Unknown uuid or alias: {value:?}"))?;
                match other {
                    [Token { rule: Rule::session_op_show, .. }, Token { rule: Rule::EOI, .. }] => Ok(Self::SessionShow(Some(uuid))),
                    [Token { rule: Rule::session_op_open, .. }, Token { rule: Rule::EOI, .. }] => Ok(Self::SessionOpen(uuid)),
                    [Token { rule: Rule::session_op_remove, .. }, Token { rule: Rule::EOI, .. }] => Ok(Self::SessionRemove(uuid)),
                    [Token { rule: Rule::session_op_alias, .. }, Token { rule: Rule::alias, value: alias }, Token { rule: Rule::EOI, .. }] => Ok(Self::SessionCreateAlias { session: uuid, alias: alias.to_string() }),
                    _ => Err("Unimplemented command!".to_string())
                }
            }
            [Token { rule: Rule::ducky_command, .. }, other @ ..] => match other {
                [Token { rule: Rule::ducky_op_flash, .. }, Token { rule: Rule::EOI, .. }] => Ok(Self::FlashFirmware(None)),
                [Token { rule: Rule::ducky_op_flash, .. }, Token { rule: Rule::firmware_url, value}, Token { rule: Rule::EOI, .. }] => Ok(Self::FlashFirmware(Some(value.to_string()))),
                _ => Err("Unimplemented command!".to_string())
            }
            [Token { rule: Rule::help_command, .. }, Token { rule: Rule::EOI, .. }] => {
                Ok(Self::Help(HelpCommand::All))
            }
            [Token { rule: Rule::help_command, .. }, Token { rule: Rule::help_op_session, .. }, Token { rule: Rule::EOI, .. }] => {
                Ok(Self::Help(HelpCommand::Session))
            }
            _ => Err("Unimplemented command!".to_string())
        }
    }

    fn object_to_uuid(value: &String, session_store: &SessionStore, alias_store: &HashMap<String, u128>) -> Option<u128> {
        match Uuid::parse_str(&value) {
            Ok(uuid) => {
                let uuid = uuid.as_u128();
                if session_store.sessions.contains_key(&uuid) {
                    Some(uuid)
                } else {
                    None
                }
            }
            Err(_) => {
                alias_store.get(value).map(|a| *a)
            }
        }
    }

    pub async fn execute(self, session_store: &mut SessionStore, alias_store: &mut HashMap<String, u128>, active_session: &mut Option<u128>) -> Result<(), String> {
        use prettytable as pt;
        debug!("EXECUTING: {:?}", self);
        match self {
            Command::SessionShow(session) => {
                let sessions = if let Some(session) = session {
                    let alias = alias_store.iter().find_map(|(key, value)| if *value == session { Some(key.to_string()) } else { None });
                    vec![(session_store.sessions.get(&session).unwrap(), alias)]
                } else {
                    session_store.sessions.values().map(|session| {
                        let alias = alias_store.iter().find_map(|(key, value)| if *value == session.uuid { Some(key.to_string()) } else { None });
                        (session, alias)
                    }).collect()
                };
                let rows = sessions.iter().map(|(session, alias)|
                    pt::Row::new(vec![
                        pt::Cell::new(Uuid::from_u128(session.uuid).to_string().as_str()),
                        pt::Cell::new(session.data.user.to_string().as_str()),
                        pt::Cell::new(session.data.directory.to_string().as_str()),
                        pt::Cell::new(format!("{}s", SystemTime::now().duration_since(session.last_seen).unwrap().as_secs_f32()).as_str()),
                        pt::Cell::new(alias.clone().unwrap_or("Undefined".to_string()).as_str()),
                    ])
                ).collect();
                let mut table = pt::Table::init(rows);
                table.set_titles(pt::Row::new(vec![
                    pt::Cell::new("UUID"),
                    pt::Cell::new("User"),
                    pt::Cell::new("Directory"),
                    pt::Cell::new("Last Seen"),
                    pt::Cell::new("Alias"),
                ]));
                let format = pt::format::FormatBuilder::new()
                    .column_separator('│')
                    .borders('│')
                    .separators(&[pt::format::LinePosition::Top], pt::format::LineSeparator::new('─', '┬', '┬', '┬'))
                    .separators(&[pt::format::LinePosition::Bottom], pt::format::LineSeparator::new('─', '┴', '┴', '┴'))
                    .separators(&[pt::format::LinePosition::Title], pt::format::LineSeparator::new('─', '┼', '┼', '┼'))
                    .padding(1, 1)
                    .indent(3)
                    .build();
                table.set_format(format);
                table.printstd();
            }
            Command::SessionCreateAlias { session, alias } => {
                // Remove old_alias if one exists
                if let Some(old_alias) = alias_store.iter().find_map(|(key, value)| if *value == session { Some(key.to_string()) } else { None }) {
                    alias_store.remove(&old_alias);
                    println!("Replaced alias {old_alias} with {alias} for {}", Uuid::from_u128(session));
                } else {
                    println!("Created alias {alias} for {}", Uuid::from_u128(session));
                }
                alias_store.insert(alias, session);
            }
            Command::SessionOpen(session) => {
                *active_session = Some(session)
            }
            Command::SessionRemove(session) => {
                if let Some(session) = session_store.sessions.remove(&session) {
                    println!("Removed session: {}", Uuid::from_u128(session.uuid));
                }
            }
            Command::Help(command) => {
                match command {
                    HelpCommand::All => {}
                    HelpCommand::Session => {}
                }
            }
            Command::FlashFirmware(url) => {
                let disks = sysinfo::Disks::new_with_refreshed_list();
                let disks = disks.into_iter()
                    .filter(|disk| disk.is_removable())
                    .filter(|disk| disk.file_system() == "vfat")
                    .filter(|disk| {
                        let to_execute = format!("find -L /dev/disk/by-label -inum $(stat -c %i {}) -print", disk.name().to_string_lossy());
                        let output = std::process::Command::new("/bin/bash")
                            .arg("-c")
                            .arg(to_execute)
                            .output()
                            .expect("failed to start process!");
                        let label = std::str::from_utf8(&output.stdout).unwrap().trim().replace("/dev/disk/by-label/", "");
                        label == "RPI-RP2"
                    }).collect::<Vec<_>>();
                if disks.is_empty() {
                    return Err("No disk to flash found, searched for disk label: \"RPI-RP2\"!".to_string())
                } else {
                    info!("Flashing {} disk(s): {}", disks.len(), disks.iter().map(|disk| disk.name().to_string_lossy()).collect::<Vec<_>>().join(", "));
                }
                let firmware = if let Some(url) = url {
                    info!("Using custom firmware: {url}");
                    reqwest::get(url).await.map_err(|err| err.to_string())?.bytes().await.map_err(|err| err.to_string())?
                } else {
                    let latest_url_github = reqwest::get("https://github.com/adafruit/circuitpython/releases/latest").await.map_err(|err| err.to_string())?.url().to_string();
                    let latest_version = latest_url_github.replace("https://github.com/adafruit/circuitpython/releases/tag/", "");
                    info!("Using latest version {latest_version} of circuit python for rp-pico");
                    let latest_url = format!("https://downloads.circuitpython.org/bin/raspberry_pi_pico/en_US/adafruit-circuitpython-raspberry_pi_pico-en_US-{latest_version}.uf2");
                    reqwest::get(latest_url).await.map_err(|err| err.to_string())?.bytes().await.map_err(|err| err.to_string())?
                };
                info!("Copying {} bytes to disk", firmware.len());
                for disk in disks {
                    let path = disk.mount_point().join("firmware.uf2");
                    info!("Copying to {}", path.to_string_lossy());
                    std::fs::write(path, firmware.clone()).map_err(|err| err.to_string())?;
                }
                info!("Successfully flashed devices!")
            }
        }
        Ok(())
    }
}