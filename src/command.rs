use std::collections::HashMap;
use log::info;
use pest::iterators::{Pair, Pairs};
use uuid::{Error, Uuid};
use crate::parser::Rule;
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
            [Token { rule: Rule::session_command, .. }, Token { rule: Rule::object, value }, Token { rule: Rule::op_open, .. }, Token { rule: Rule::EOI, .. }] => {
                let uuid = Self::object_to_uuid(&value, &alias_store).ok_or(format!("Invalid/Unknown uuid or alias: {value:?}"))?;
                Ok(Self::SessionOpen(uuid))
            }
            [Token { rule: Rule::session_command, .. }, Token { rule: Rule::object, value }, Token { rule: Rule::op_show, .. }, Token { rule: Rule::EOI, .. }] => {
                let uuid = Self::object_to_uuid(&value, &alias_store).ok_or(format!("Invalid/Unknown uuid or alias: {value:?}"))?;
                Ok(Self::SessionShow(Some(uuid)))
            }
            [Token { rule: Rule::session_command, .. }, Token { rule: Rule::object, value: object }, Token { rule: Rule::op_alias, .. }, Token { rule: Rule::alias, value: alias }, Token { rule: Rule::EOI, .. }] => {
                let uuid = Self::object_to_uuid(&object, &alias_store).ok_or(format!("Invalid/Unknown uuid or alias: {object:?}"))?;
                Ok(Self::SessionCreateAlias {
                    session: uuid,
                    alias: alias.to_string(),
                })
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

    fn object_to_uuid(value: &String, alias_store: &HashMap<String, u128>) -> Option<u128> {
        match Uuid::parse_str(&value) {
            Ok(uuid) => {
                Some(uuid.as_u128())
            }
            Err(_) => {
                alias_store.get(value).map(|a| *a)
            }
        }
    }

    pub fn execute(&self, session_store: &mut SessionStore, alias_store: &mut HashMap<String, u128>) -> Result<(), String> {
        info!("EXECUTING: {:?}", self);
        match self {
            Command::SessionShow(session) => {}
            Command::SessionCreateAlias { session, alias } => {}
            Command::SessionOpen(session) => {}
            Command::Help(command) => {}
        }
        Ok(())
    }
}