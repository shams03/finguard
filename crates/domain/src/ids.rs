//! Newtype identifiers. A `CardId` can never be passed where an `AccountId` is
//! expected. Display form is prefixed (`act_<hex>`) which is what the API and
//! UI show; parsing accepts the prefixed form or a bare UUID.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt, str::FromStr};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum IdError {
    #[error("invalid {kind} id")]
    Invalid { kind: &'static str },
    #[error("invalid idempotency key: {0}")]
    BadIdempotencyKey(&'static str),
}

macro_rules! uuid_id {
    ($name:ident, $prefix:literal, $kind:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub Uuid);

        impl $name {
            pub const PREFIX: &'static str = $prefix;
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }
            pub fn as_uuid(&self) -> Uuid {
                self.0
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
        impl From<Uuid> for $name {
            fn from(u: Uuid) -> Self {
                Self(u)
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}_{}", $prefix, self.0.simple())
            }
        }
        impl FromStr for $name {
            type Err = IdError;
            fn from_str(s: &str) -> Result<Self, IdError> {
                let raw = s.strip_prefix(concat!($prefix, "_")).unwrap_or(s);
                Uuid::parse_str(raw)
                    .map(Self)
                    .map_err(|_| IdError::Invalid { kind: $kind })
            }
        }
        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.collect_str(self)
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                s.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

uuid_id!(UserId, "usr", "user");
uuid_id!(SessionId, "ses", "session");
uuid_id!(DeviceId, "dev", "device");
uuid_id!(AccountId, "acc", "account");
uuid_id!(CardId, "crd", "card");
uuid_id!(RecipientId, "rcp", "recipient");
uuid_id!(ActionId, "act", "action");
uuid_id!(TraceId, "tr", "trace");
uuid_id!(ConfirmationId, "cnf", "confirmation");
uuid_id!(LedgerTransactionId, "ltx", "ledger transaction");

/// Client supplied key making a state-changing request safe to retry.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    pub fn new(raw: &str) -> Result<Self, IdError> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(IdError::BadIdempotencyKey("empty"));
        }
        if raw.len() > 128 {
            return Err(IdError::BadIdempotencyKey("longer than 128 characters"));
        }
        if !raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
        {
            return Err(IdError::BadIdempotencyKey("unsupported characters"));
        }
        Ok(Self(raw.to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IdempotencyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_roundtrip_prefixed_and_bare() {
        let id = ActionId::new();
        let s = id.to_string();
        assert!(s.starts_with("act_"));
        assert_eq!(s.parse::<ActionId>().unwrap(), id);
        assert_eq!(id.as_uuid().to_string().parse::<ActionId>().unwrap(), id);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(serde_json::from_str::<ActionId>(&json).unwrap(), id);
    }

    #[test]
    fn id_rejects_garbage() {
        assert!("not-a-uuid".parse::<CardId>().is_err());
    }

    #[test]
    fn idempotency_key_validation() {
        assert!(IdempotencyKey::new("abc123").is_ok());
        assert!(IdempotencyKey::new("").is_err());
        assert!(IdempotencyKey::new("has space").is_err());
        assert!(IdempotencyKey::new(&"x".repeat(129)).is_err());
    }
}
