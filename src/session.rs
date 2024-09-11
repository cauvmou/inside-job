use std::cmp::Ordering;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Command {
    pub timestamp: std::time::SystemTime,
    pub input: String,
    pub output: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Status {
    Active,
    Disconnected,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SessionData {
    pub user: String,
    pub directory: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Session {
    pub uuid: u128,
    pub last_seen: std::time::SystemTime,
    pub status: Status,
    pub metadata: SessionData,
}

impl PartialOrd for Session {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.uuid.partial_cmp(&other.uuid)
    }
}

impl Ord for Session {
    fn cmp(&self, other: &Self) -> Ordering {
        self.uuid.cmp(&other.uuid)
    }
}