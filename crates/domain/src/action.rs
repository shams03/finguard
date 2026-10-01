//! Typed actions. The critical layers never see `tool_name: String` plus a JSON
//! blob: model output is converted into one of these enums immediately, and an
//! unknown tool name is rejected at that boundary.

use crate::{
    actor::Permission,
    ids::{AccountId, CardId, RecipientId, UserId},
    money::Money,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fmt, str::FromStr};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RiskClass {
    ReadOnly,
    Reversible,
    MoneyMovement,
    HighImpact,
}

impl RiskClass {
    pub fn as_str(self) -> &'static str {
        match self {
            RiskClass::ReadOnly => "READ_ONLY",
            RiskClass::Reversible => "REVERSIBLE",
            RiskClass::MoneyMovement => "MONEY_MOVEMENT",
            RiskClass::HighImpact => "HIGH_IMPACT",
        }
    }
}

impl FromStr for RiskClass {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        Ok(match s {
            "READ_ONLY" => RiskClass::ReadOnly,
            "REVERSIBLE" => RiskClass::Reversible,
            "MONEY_MOVEMENT" => RiskClass::MoneyMovement,
            "HIGH_IMPACT" => RiskClass::HighImpact,
            _ => return Err(()),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActionKind {
    ReadBalance,
    FreezeCard,
    TransferFunds,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("unknown action: {0}")]
pub struct UnknownAction(pub String);

impl ActionKind {
    pub const ALL: [ActionKind; 3] = [
        ActionKind::ReadBalance,
        ActionKind::FreezeCard,
        ActionKind::TransferFunds,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            ActionKind::ReadBalance => "READ_BALANCE",
            ActionKind::FreezeCard => "FREEZE_CARD",
            ActionKind::TransferFunds => "TRANSFER_FUNDS",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            ActionKind::ReadBalance => "Read Balance",
            ActionKind::FreezeCard => "Freeze Card",
            ActionKind::TransferFunds => "Transfer Funds",
        }
    }
    pub fn risk_class(self) -> RiskClass {
        tool_spec(self).risk_class
    }
}

impl FromStr for ActionKind {
    type Err = UnknownAction;
    /// The allow-list: anything that is not a registered tool is rejected.
    fn from_str(s: &str) -> Result<Self, UnknownAction> {
        ActionKind::ALL
            .into_iter()
            .find(|k| k.as_str() == s)
            .ok_or_else(|| UnknownAction(s.chars().take(64).collect()))
    }
}

impl fmt::Display for ActionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Registry entry: everything policy needs to know about a tool.
#[derive(Debug, Clone, Copy)]
pub struct ToolSpec {
    pub kind: ActionKind,
    pub risk_class: RiskClass,
    pub required_permission: Permission,
    /// Always demand explicit user confirmation before executing.
    pub requires_confirmation: bool,
    /// Whether an AI risk assessment (Jev) is part of policy for this tool.
    pub requires_jev: bool,
    /// Name of the deterministic executor function (documentation + traces).
    pub executor: &'static str,
    /// Human description of the argument schema.
    pub argument_schema: &'static str,
}

static REGISTRY: [ToolSpec; 3] = [
    ToolSpec {
        kind: ActionKind::ReadBalance,
        risk_class: RiskClass::ReadOnly,
        required_permission: Permission::ReadAccount,
        requires_confirmation: false,
        requires_jev: false,
        executor: "executor::read_balance",
        argument_schema: "{ account_id }",
    },
    ToolSpec {
        kind: ActionKind::FreezeCard,
        risk_class: RiskClass::Reversible,
        required_permission: Permission::FreezeCard,
        requires_confirmation: true,
        requires_jev: false,
        executor: "executor::freeze_card",
        argument_schema: "{ card_id }",
    },
    ToolSpec {
        kind: ActionKind::TransferFunds,
        risk_class: RiskClass::MoneyMovement,
        required_permission: Permission::TransferFunds,
        requires_confirmation: true,
        requires_jev: true,
        executor: "executor::transfer_funds",
        argument_schema: "{ from_account, recipient_id, amount{amount_minor,currency} }",
    },
];

pub fn tool_spec(kind: ActionKind) -> &'static ToolSpec {
    REGISTRY
        .iter()
        .find(|s| s.kind == kind)
        .expect("every ActionKind is registered (enforced by test)")
}

pub fn tool_registry() -> &'static [ToolSpec] {
    &REGISTRY
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingField {
    Amount,
    Recipient,
}

impl MissingField {
    pub fn as_str(self) -> &'static str {
        match self {
            MissingField::Amount => "amount",
            MissingField::Recipient => "recipient",
        }
    }
}

/// A fully specified, executable action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProposedAction {
    ReadBalance {
        account_id: AccountId,
    },
    FreezeCard {
        card_id: CardId,
    },
    TransferFunds {
        from_account: AccountId,
        recipient_id: RecipientId,
        amount: Money,
    },
}

impl ProposedAction {
    pub fn kind(&self) -> ActionKind {
        match self {
            ProposedAction::ReadBalance { .. } => ActionKind::ReadBalance,
            ProposedAction::FreezeCard { .. } => ActionKind::FreezeCard,
            ProposedAction::TransferFunds { .. } => ActionKind::TransferFunds,
        }
    }
    pub fn risk_class(&self) -> RiskClass {
        self.kind().risk_class()
    }

    /// Stable fingerprint bound to actor + every parameter that matters.
    /// Fields are written in a fixed order with a version tag, so it does not
    /// depend on JSON key order. SHA-256 hex.
    pub fn fingerprint(&self, actor: UserId) -> String {
        let (account, card, recipient, amount, currency) = match self {
            ProposedAction::ReadBalance { account_id } => (
                account_id.to_string(),
                "-".to_owned(),
                "-".to_owned(),
                "-".to_owned(),
                "-".to_owned(),
            ),
            ProposedAction::FreezeCard { card_id } => (
                "-".to_owned(),
                card_id.to_string(),
                "-".to_owned(),
                "-".to_owned(),
                "-".to_owned(),
            ),
            ProposedAction::TransferFunds {
                from_account,
                recipient_id,
                amount,
            } => (
                from_account.to_string(),
                "-".to_owned(),
                recipient_id.to_string(),
                amount.amount_minor.to_string(),
                amount.currency.code().to_owned(),
            ),
        };
        let canonical = format!(
            "finguard-fp-v1\nactor={actor}\nkind={}\naccount={account}\ncard={card}\nrecipient={recipient}\namount={amount}\ncurrency={currency}",
            self.kind()
        );
        hex::encode(Sha256::digest(canonical.as_bytes()))
    }
}

/// An action whose parameters may still be incomplete (e.g. "send Alex the
/// usual amount"). It can be risk-assessed, but never executed: `complete()`
/// is the only way to obtain a `ProposedAction`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActionDraft {
    ReadBalance {
        account_id: AccountId,
    },
    FreezeCard {
        card_id: CardId,
    },
    TransferFunds {
        from_account: AccountId,
        recipient_id: Option<RecipientId>,
        amount: Option<Money>,
    },
}

impl ActionDraft {
    pub fn kind(&self) -> ActionKind {
        match self {
            ActionDraft::ReadBalance { .. } => ActionKind::ReadBalance,
            ActionDraft::FreezeCard { .. } => ActionKind::FreezeCard,
            ActionDraft::TransferFunds { .. } => ActionKind::TransferFunds,
        }
    }
    pub fn risk_class(&self) -> RiskClass {
        self.kind().risk_class()
    }
    pub fn missing_fields(&self) -> Vec<MissingField> {
        match self {
            ActionDraft::TransferFunds {
                recipient_id,
                amount,
                ..
            } => {
                let mut m = vec![];
                if recipient_id.is_none() {
                    m.push(MissingField::Recipient);
                }
                if amount.is_none() {
                    m.push(MissingField::Amount);
                }
                m
            }
            _ => vec![],
        }
    }
    pub fn complete(&self) -> Result<ProposedAction, Vec<MissingField>> {
        let missing = self.missing_fields();
        if !missing.is_empty() {
            return Err(missing);
        }
        Ok(match self {
            ActionDraft::ReadBalance { account_id } => ProposedAction::ReadBalance {
                account_id: *account_id,
            },
            ActionDraft::FreezeCard { card_id } => ProposedAction::FreezeCard { card_id: *card_id },
            ActionDraft::TransferFunds {
                from_account,
                recipient_id: Some(r),
                amount: Some(a),
            } => ProposedAction::TransferFunds {
                from_account: *from_account,
                recipient_id: *r,
                amount: *a,
            },
            // missing_fields() above guarantees completeness
            ActionDraft::TransferFunds { .. } => return Err(self.missing_fields()),
        })
    }
}

impl From<ProposedAction> for ActionDraft {
    fn from(a: ProposedAction) -> Self {
        match a {
            ProposedAction::ReadBalance { account_id } => ActionDraft::ReadBalance { account_id },
            ProposedAction::FreezeCard { card_id } => ActionDraft::FreezeCard { card_id },
            ProposedAction::TransferFunds {
                from_account,
                recipient_id,
                amount,
            } => ActionDraft::TransferFunds {
                from_account,
                recipient_id: Some(recipient_id),
                amount: Some(amount),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transfer(amount: i64) -> ProposedAction {
        ProposedAction::TransferFunds {
            from_account: AccountId(uuid::Uuid::from_u128(1)),
            recipient_id: RecipientId(uuid::Uuid::from_u128(2)),
            amount: Money::php(amount),
        }
    }

    #[test]
    fn risk_classification() {
        assert_eq!(ActionKind::ReadBalance.risk_class(), RiskClass::ReadOnly);
        assert_eq!(ActionKind::FreezeCard.risk_class(), RiskClass::Reversible);
        assert_eq!(
            ActionKind::TransferFunds.risk_class(),
            RiskClass::MoneyMovement
        );
        assert_eq!(transfer(1).risk_class(), RiskClass::MoneyMovement);
    }

    #[test]
    fn registry_covers_every_kind_and_transfers_need_confirmation() {
        for k in ActionKind::ALL {
            assert_eq!(tool_spec(k).kind, k);
        }
        assert!(tool_spec(ActionKind::TransferFunds).requires_confirmation);
        assert!(tool_spec(ActionKind::TransferFunds).requires_jev);
        assert!(!tool_spec(ActionKind::ReadBalance).requires_jev);
    }

    #[test]
    fn unknown_tool_is_rejected() {
        assert!("DELETE_DATABASE".parse::<ActionKind>().is_err());
        assert!("TRANSFER_FUNDS".parse::<ActionKind>().is_ok());
    }

    #[test]
    fn fingerprint_is_deterministic_and_binds_every_field() {
        let u = UserId(uuid::Uuid::from_u128(9));
        assert_eq!(transfer(100).fingerprint(u), transfer(100).fingerprint(u));
        assert_ne!(transfer(100).fingerprint(u), transfer(101).fingerprint(u));
        assert_ne!(
            transfer(100).fingerprint(u),
            transfer(100).fingerprint(UserId(uuid::Uuid::from_u128(10)))
        );
        assert_eq!(transfer(100).fingerprint(u).len(), 64);
    }

    #[test]
    fn draft_completion() {
        let d = ActionDraft::TransferFunds {
            from_account: AccountId::new(),
            recipient_id: Some(RecipientId::new()),
            amount: None,
        };
        assert_eq!(d.missing_fields(), vec![MissingField::Amount]);
        assert!(d.complete().is_err());
        let full: ActionDraft = transfer(500).into();
        assert_eq!(full.complete().unwrap(), transfer(500));
    }
}
