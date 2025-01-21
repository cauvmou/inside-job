use crate::session::{Command, Session, SessionData};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;

#[derive(Debug, Deserialize, Serialize)]
pub enum LockState {
    ToSend(String),
    ToReceive(String),
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct SessionStore {
    pub sessions: HashMap<u128, Session>,
    pub session_lock: HashMap<u128, Option<LockState>>,
    pub commands: HashMap<u128, Vec<Command>>,
}

impl SessionStore {
    pub fn create_session(&mut self, session: Session) {
        let uuid = session.uuid;
        self.sessions.insert(uuid, session);
        self.session_lock.insert(uuid, None);
        self.commands.insert(uuid, Vec::new());
    }

    pub fn start_command(&mut self, uuid: u128, input: String) -> Result<(), String> {
        let current = self
            .session_lock
            .get_mut(&uuid)
            .ok_or("No session for uuid!".to_string())?;
        if let Some(lock_state) = current.take() {
            Err(format!("A command is already pending: {lock_state:?}!").to_string())
        } else {
            *current = Some(LockState::ToSend(input));
            Ok(())
        }
    }

    pub fn get_pending_command(&self, uuid: u128) -> Result<&String, String> {
        let current = self
            .session_lock
            .get(&uuid)
            .ok_or("No session for uuid!".to_string())?;
        if let Some(lock_state) = current {
            match lock_state {
                LockState::ToSend(command) => Ok(command),
                LockState::ToReceive(_) => Err("Command has not been printed yet!".to_string()),
            }
        } else {
            Err("Cannot acquire session lock!".to_string())
        }
    }

    fn clear_pending_command(&mut self, uuid: u128) -> Result<String, String> {
        let session_lock = self
            .session_lock
            .get_mut(&uuid)
            .ok_or("No session for uuid!".to_string())?;
        let lock_state = session_lock.take().ok_or("Already empty!".to_string())?;
        *session_lock = None;
        match lock_state {
            LockState::ToSend(command) | LockState::ToReceive(command) => Ok(command),
        }
    }

    pub fn resolve_command(&mut self, uuid: u128, output: String) -> Result<Command, String> {
        let input = self.clear_pending_command(uuid)?;
        let session_lock = self
            .session_lock
            .get_mut(&uuid)
            .ok_or("No session for uuid!".to_string())?;
        *session_lock = Some(LockState::ToReceive(output.clone()));
        let command = Command {
            timestamp: SystemTime::now(),
            input,
            output,
        };
        Ok(command)
    }

    pub fn insert_command(&mut self, uuid: u128, command: Command) -> Result<(), String> {
        let commands = self
            .commands
            .get_mut(&uuid)
            .ok_or("No session for uuid!".to_string())?;
        commands.push(command);
        Ok(())
    }

    pub fn update_session_data(&mut self, uuid: u128, data: SessionData) -> Result<(), String> {
        let session = self
            .sessions
            .get_mut(&uuid)
            .ok_or("No session for uuid!".to_string())?;
        session.data = data;
        Ok(())
    }

    pub fn seen(&mut self, uuid: u128) -> Result<(), String> {
        let session = self
            .sessions
            .get_mut(&uuid)
            .ok_or("No session for uuid!".to_string())?;
        session.last_seen = SystemTime::now();
        Ok(())
    }
}
