use std::collections::HashMap;
use std::time::SystemTime;
use log::info;
use pest::iterators::{Pair, Pairs};
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
    Help(HelpCommand)
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
            [Token { rule: Rule::session_command, .. }, Token { rule: Rule::op_show, .. }, Token { rule: Rule::EOI, .. }] => {
                Ok(Self::SessionShow(None))
            }
            [Token { rule: Rule::session_command, .. }, Token { rule: Rule::object, value }, other @ ..] => {
                let uuid = Self::object_to_uuid(&value, &session_store, &alias_store).ok_or(format!("Invalid/Unknown uuid or alias: {value:?}"))?;
                match other {
                    [Token { rule: Rule::op_show, .. }, Token { rule: Rule::EOI, .. }] => Ok(Self::SessionShow(Some(uuid))),
                    [Token { rule: Rule::op_open, .. }, Token { rule: Rule::EOI, .. }] => Ok(Self::SessionOpen(uuid)),
                    [Token { rule: Rule::op_alias, .. }, Token { rule: Rule::alias, value: alias }, Token { rule: Rule::EOI, .. }] => Ok(Self::SessionCreateAlias { session: uuid, alias: alias.to_string()}),
                    _ => Err("Unimplemented command!".to_string())
                }
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

    pub fn execute(self, session_store: &mut SessionStore, alias_store: &mut HashMap<String, u128>, active_session: &mut Option<u128>) -> Result<(), String> {
        info!("EXECUTING: {:?}", self);
        match self {
            Command::SessionShow(session) => {
                for session in session_store.sessions.values() {
                    println!("{uuid} | {user:<48} | {dir:<48} | {time:.2}s", uuid=Uuid::from_u128(session.uuid), user=session.data.user, dir=session.data.directory, time=SystemTime::now().duration_since(session.last_seen).unwrap().as_secs_f32());
                }
            }
            Command::SessionCreateAlias { session, alias } => {
                alias_store.insert(alias, session);
            }
            Command::SessionOpen(session) => {
                *active_session = Some(session)
            }
            Command::Help(command) => {}
        }
        Ok(())
    }
}