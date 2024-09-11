use std::cell::Cell;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use serde::{Deserialize, Serialize};
use crate::session::{Command, Session};

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct SessionStore {
    pub sessions: RwLock<HashMap<u128, RwLock<Session>>>,
    pub session_lock: RwLock<HashMap<u128, RwLock<Option<String>>>>,
    pub commands: RwLock<HashMap<u128, RwLock<Vec<Command>>>>,
}