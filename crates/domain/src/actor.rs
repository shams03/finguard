//! Who is asking. In this sandbox the actor is a fixed synthetic demo user;
//! production would populate `ActorContext` from a real identity provider.

use crate::ids::{DeviceId, SessionId, UserId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Permission {
    ReadAccount,
    FreezeCard,
    TransferFunds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorContext {
    pub user_id: UserId,
    pub session_id: SessionId,
    pub permissions: Vec<Permission>,
    pub device_id: DeviceId,
}

impl ActorContext {
    pub fn has(&self, p: Permission) -> bool {
        self.permissions.contains(&p)
    }
}
