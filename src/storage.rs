use std::cell::Cell;
use std::collections::HashMap;
use std::sync::{Arc, LockResult, RwLock};
use serde::{Deserialize, Serialize};
use crate::session::{Command, Session, SessionData};

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct SessionStore {
    pub sessions: HashMap<u128, RwLock<Session>>,
    pub session_lock: HashMap<u128, RwLock<Option<String>>>,
    pub commands: HashMap<u128, RwLock<Vec<Command>>>,
}

impl SessionStore {
    pub fn create_session(&mut self, session: Session) {
        let uuid = session.uuid;
        self.sessions.insert(uuid, RwLock::new(session));
        self.session_lock.insert(uuid, RwLock::new(None));
        self.commands.insert(uuid, RwLock::new(Vec::new()));
    }

    pub fn start_command(&self, uuid: u128, input: String) -> Result<(), String> {
        let current = self.session_lock.get(&uuid).ok_or("No session for uuid!".to_string())?;
        let mut current = current.write().ok().ok_or("Cannot acquire session lock!".to_string())?;
        if let Some(command) = current.take() {
            Err(format!("A command is already pending: {command:?}!").to_string())
        } else {
            *current = Some(input);
            Ok(())
        }
    }
    
    pub fn get_pending_command(&self, uuid: u128) -> Result<String, String> {
        let current = self.session_lock.get(&uuid).ok_or("No session for uuid!".to_string())?;
        let mut current = current.write().ok().ok_or("Cannot acquire session lock!".to_string())?;
        let command = current.take().ok_or("Already empty!".to_string())?;
        Ok(command)
    }
    
    fn clear_pending_command(&self, uuid: u128) -> Result<String, String> {
        let current = self.session_lock.get(&uuid).ok_or("No session for uuid!".to_string())?;
        let mut current = current.write().ok().ok_or("Cannot acquire session lock!".to_string())?;
        let command = current.take().ok_or("Already empty!".to_string())?;
        *current = None;
        Ok(command)
    }

    pub fn resolve_command(&self, uuid: u128, output: String) -> Result<Command, String> {
        let input = self.clear_pending_command(uuid)?;
        let command = Command {
            timestamp: std::time::SystemTime::now(),
            input,
            output,
        };
        Ok(command)
    }

    pub fn insert_command(&self, uuid: u128, command: Command) -> Result<(), String> {
        let commands = self.commands.get(&uuid).ok_or("No session for uuid!".to_string())?;
        let mut commands = commands.write().ok().ok_or("Cannot acquire commands lock!".to_string())?;
        commands.push(command);
        Ok(())
    }
    
    pub fn update_session_data(&self, uuid: u128, data: SessionData) -> Result<(), String> {
        let session = self.sessions.get(&uuid).ok_or("No session for uuid!".to_string())?;
        let mut session = session.write().ok().ok_or("Cannot acquire session!".to_string())?;
        session.data = data;
        Ok(())
    }
}