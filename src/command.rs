use crate::parser::Rule;
use crate::payload;
use crate::storage::SessionStore;
use log::{debug, info, warn};
use pest::iterators::{Pair, Pairs};
use rayon::prelude::*;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::str::FromStr;
use std::time::SystemTime;
use uuid::Uuid;

static HELP_SESSION_STRING: &'static str = r#"session ( show | sh )
session ( <alias> | <uuid> ) ( show | sh )
session ( <alias> | <uuid> ) ( alias | > ) <alias>
session ( <alias> | <uuid> ) ( open | op | . )
session ( <alias> | <uuid> ) ( forget | x )"#;

static HELP_DUCKY_STRING: &'static str = r#"ducky flash <server_address>"#;

static HELP_STRING: &'static str = r#"( help | ? )
( help | ? ) session
( exit | quit | q )"#;

#[derive(Debug)]
pub enum Command {
    SessionShow(Option<u128>),
    SessionCreateAlias { session: u128, alias: String },
    SessionOpen(u128),
    SessionRemove(u128),
    FlashFirmware(String),
    Help(HelpCommand),
    Quit,
}

#[derive(Debug)]
pub enum HelpCommand {
    All,
    Session,
    Ducky,
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
    pub fn from_pairs(
        pairs: Pairs<Rule>,
        session_store: &SessionStore,
        alias_store: &HashMap<String, u128>,
    ) -> Result<Self, String> {
        let rules = pairs.flatten().map(Token::from).collect::<Vec<_>>();
        let rules = rules.iter().collect::<Vec<_>>();
        match rules.as_slice() {
            [Token {
                rule: Rule::session_command,
                ..
            }, Token {
                rule: Rule::session_op_show,
                ..
            }, Token {
                rule: Rule::EOI, ..
            }] => Ok(Self::SessionShow(None)),
            [Token {
                rule: Rule::session_command,
                ..
            }, Token {
                rule: Rule::object,
                value,
            }, other @ ..] => {
                let uuid = Self::object_to_uuid(value, session_store, alias_store)
                    .ok_or(format!("Invalid/Unknown uuid or alias: {value:?}"))?;
                match other {
                    [Token {
                        rule: Rule::session_op_show,
                        ..
                    }, Token {
                        rule: Rule::EOI, ..
                    }] => Ok(Self::SessionShow(Some(uuid))),
                    [Token {
                        rule: Rule::session_op_open,
                        ..
                    }, Token {
                        rule: Rule::EOI, ..
                    }] => Ok(Self::SessionOpen(uuid)),
                    [Token {
                        rule: Rule::session_op_remove,
                        ..
                    }, Token {
                        rule: Rule::EOI, ..
                    }] => Ok(Self::SessionRemove(uuid)),
                    [Token {
                        rule: Rule::session_op_alias,
                        ..
                    }, Token {
                        rule: Rule::alias,
                        value: alias,
                    }, Token {
                        rule: Rule::EOI, ..
                    }] => Ok(Self::SessionCreateAlias {
                        session: uuid,
                        alias: alias.to_string(),
                    }),
                    _ => Err("Unimplemented command!".to_string()),
                }
            }
            [Token {
                rule: Rule::ducky_command,
                ..
            }, other @ ..] => match other {
                [Token {
                    rule: Rule::ducky_op_flash,
                    ..
                }, Token {
                    rule: Rule::server_addr,
                    value,
                }, Token {
                    rule: Rule::EOI, ..
                }] => Ok(Self::FlashFirmware(value.to_string())),
                _ => Err("Unimplemented command!".to_string()),
            },
            [Token {
                rule: Rule::help_command,
                ..
            }, Token {
                rule: Rule::EOI, ..
            }] => Ok(Self::Help(HelpCommand::All)),
            [Token {
                rule: Rule::help_command,
                ..
            }, Token {
                rule: Rule::help_op_session,
                ..
            }, Token {
                rule: Rule::EOI, ..
            }] => Ok(Self::Help(HelpCommand::Session)),
            [Token {
                rule: Rule::help_command,
                ..
            }, Token {
                rule: Rule::help_op_ducky,
                ..
            }, Token {
                rule: Rule::EOI, ..
            }] => Ok(Self::Help(HelpCommand::Ducky)),
            [Token {
                rule: Rule::exit_command,
                ..
            }, Token {
                rule: Rule::EOI, ..
            }] => Ok(Self::Quit),
            _ => Err("Unimplemented command!".to_string()),
        }
    }

    fn object_to_uuid(
        value: &String,
        session_store: &SessionStore,
        alias_store: &HashMap<String, u128>,
    ) -> Option<u128> {
        match Uuid::parse_str(&value) {
            Ok(uuid) => {
                let uuid = uuid.as_u128();
                if session_store.sessions.contains_key(&uuid) {
                    Some(uuid)
                } else {
                    None
                }
            }
            Err(_) => alias_store.get(value).map(|a| *a),
        }
    }

    pub async fn execute(
        self,
        session_store: &mut SessionStore,
        alias_store: &mut HashMap<String, u128>,
        active_session: &mut Option<u128>,
    ) -> Result<bool, String> {
        use prettytable as pt;
        debug!("EXECUTING: {:?}", self);
        match self {
            Command::SessionShow(session) => {
                let sessions = if let Some(session) = session {
                    let alias = alias_store.iter().find_map(|(key, value)| {
                        if *value == session {
                            Some(key.to_string())
                        } else {
                            None
                        }
                    });
                    vec![(session_store.sessions.get(&session).unwrap(), alias)]
                } else {
                    session_store
                        .sessions
                        .values()
                        .map(|session| {
                            let alias = alias_store.iter().find_map(|(key, value)| {
                                if *value == session.uuid {
                                    Some(key.to_string())
                                } else {
                                    None
                                }
                            });
                            (session, alias)
                        })
                        .collect()
                };
                let rows = sessions
                    .iter()
                    .map(|(session, alias)| {
                        pt::Row::new(vec![
                            pt::Cell::new(Uuid::from_u128(session.uuid).to_string().as_str()),
                            pt::Cell::new(session.data.user.to_string().as_str()),
                            pt::Cell::new(session.data.directory.to_string().as_str()),
                            pt::Cell::new(
                                format!(
                                    "{}s",
                                    SystemTime::now()
                                        .duration_since(session.last_seen)
                                        .unwrap()
                                        .as_secs_f32()
                                )
                                .as_str(),
                            ),
                            pt::Cell::new(
                                alias.clone().unwrap_or("Undefined".to_string()).as_str(),
                            ),
                        ])
                    })
                    .collect();
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
                    .separators(
                        &[pt::format::LinePosition::Top],
                        pt::format::LineSeparator::new('─', '┬', '┬', '┬'),
                    )
                    .separators(
                        &[pt::format::LinePosition::Bottom],
                        pt::format::LineSeparator::new('─', '┴', '┴', '┴'),
                    )
                    .separators(
                        &[pt::format::LinePosition::Title],
                        pt::format::LineSeparator::new('─', '┼', '┼', '┼'),
                    )
                    .padding(1, 1)
                    .indent(3)
                    .build();
                table.set_format(format);
                table.printstd();
            }
            Command::SessionCreateAlias { session, alias } => {
                // Remove old_alias if one exists
                if let Some(old_alias) = alias_store.iter().find_map(|(key, value)| {
                    if *value == session {
                        Some(key.to_string())
                    } else {
                        None
                    }
                }) {
                    alias_store.remove(&old_alias);
                    println!(
                        "Replaced alias {old_alias} with {alias} for {}",
                        Uuid::from_u128(session)
                    );
                } else {
                    println!("Created alias {alias} for {}", Uuid::from_u128(session));
                }
                alias_store.insert(alias, session);
            }
            Command::SessionOpen(session) => *active_session = Some(session),
            Command::SessionRemove(session) => {
                if let Some(session) = session_store.sessions.remove(&session) {
                    println!("Removed session: {}", Uuid::from_u128(session.uuid));
                }
            }
            Command::Help(command) => match command {
                HelpCommand::All => {
                    println!("{HELP_SESSION_STRING}");
                    println!("{HELP_DUCKY_STRING}");
                    println!("{HELP_STRING}");
                }
                HelpCommand::Session => {
                    println!("{HELP_SESSION_STRING}");
                }
                HelpCommand::Ducky => {
                    println!("{HELP_DUCKY_STRING}");
                }
            },
            Command::FlashFirmware(address) => {
                let disks = sysinfo::Disks::new_with_refreshed_list();
                let disks = disks
                    .into_iter()
                    .filter(|disk| disk.is_removable())
                    .filter(|disk| disk.file_system() == "vfat")
                    .filter(|disk| {
                        let to_execute = format!(
                            "find -L /dev/disk/by-label -inum $(stat -c %i {}) -print",
                            disk.name().to_string_lossy()
                        );
                        let output = std::process::Command::new("/bin/bash")
                            .arg("-c")
                            .arg(to_execute)
                            .output()
                            .expect("failed to start process!");
                        let label = std::str::from_utf8(&output.stdout)
                            .unwrap()
                            .trim()
                            .replace("/dev/disk/by-label/", "");
                        label == "CIRCUITPY"
                    })
                    .collect::<Vec<_>>();
                if disks.is_empty() {
                    return Err(
                        "No disk to flash found, searched for disk label: \"RPI-RP2\"!".to_string(),
                    );
                } else {
                    info!(
                        "Flashing {} disk(s): {}",
                        disks.len(),
                        disks
                            .iter()
                            .map(|disk| disk.name().to_string_lossy())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                let addr = Ipv4Addr::from_str(address.as_str()).map_err(|err| err.to_string())?;
                disks
                    .par_iter()
                    .for_each(|disk| match payload::flash_disk(disk, addr, "4132") {
                        Ok(()) => {
                            info!("Sucessfully flashed disk {}", disk.name().to_string_lossy())
                        }
                        Err(err) => warn!(
                            "Couldn't flash disk {}: {err}",
                            disk.name().to_string_lossy()
                        ),
                    })
            }
            Command::Quit => {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
