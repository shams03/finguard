//! Hard financial rules. They run before Jev (a hard failure costs zero AI
//! calls) and are evaluated **again** inside the execution transaction with
//! freshly locked rows, because an earlier check is only a snapshot.

use finguard_domain::{
    ActionDraft, ActorContext, CheckStatus, HardCheck, HardPolicyReason, HardPolicyReport,
    HardPolicyResult, Money, UserId, tool_spec,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardState {
    Active,
    Frozen,
}

#[derive(Debug, Clone)]
pub struct AccountFacts {
    pub owner: Option<UserId>,
    pub balance: Money,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub struct CardFacts {
    pub owner: UserId,
    pub state: CardState,
}

#[derive(Debug, Clone)]
pub struct RecipientFacts {
    pub owner: UserId,
    pub blocked: bool,
}

/// What happened when we tried to resolve the recipient.
#[derive(Debug, Clone)]
pub enum RecipientLookup {
    /// The request named no recipient (action is incomplete).
    NotRequested,
    /// A reference was given but nothing matched.
    NotFound,
    Found(RecipientFacts),
}

#[derive(Debug, Clone)]
pub struct HardFacts {
    pub account: Option<AccountFacts>,
    pub card: Option<CardFacts>,
    pub recipient: RecipientLookup,
}

#[derive(Debug, Clone, Copy)]
pub struct HardLimits {
    /// Absolute per-transfer ceiling no model or confirmation can exceed.
    pub absolute_transfer_limit: Money,
}

impl Default for HardLimits {
    fn default() -> Self {
        Self {
            absolute_transfer_limit: Money::php_major(500_000),
        }
    }
}

struct Builder {
    checks: Vec<HardCheck>,
}

impl Builder {
    fn pass(&mut self, name: &str, detail: impl Into<String>) {
        self.checks.push(HardCheck {
            name: name.into(),
            status: CheckStatus::Pass,
            detail: detail.into(),
            reason: None,
        });
    }
    fn fail(&mut self, name: &str, detail: impl Into<String>, reason: HardPolicyReason) {
        self.checks.push(HardCheck {
            name: name.into(),
            status: CheckStatus::Fail,
            detail: detail.into(),
            reason: Some(reason),
        });
    }
    fn skip(&mut self, name: &str, detail: impl Into<String>) {
        self.checks.push(HardCheck {
            name: name.into(),
            status: CheckStatus::Skipped,
            detail: detail.into(),
            reason: None,
        });
    }
    fn finish(self) -> HardPolicyReport {
        let result = self
            .checks
            .iter()
            .find_map(|c| (c.status == CheckStatus::Fail).then_some(c.reason))
            .flatten()
            .map(HardPolicyResult::Block)
            .unwrap_or(HardPolicyResult::Pass);
        HardPolicyReport {
            checks: self.checks,
            result,
        }
    }
}

/// Evaluate every applicable hard rule. Checks that cannot be evaluated yet
/// (e.g. funds for a transfer whose amount is still unknown) are `Skipped`,
/// never silently passed. A draft with skipped checks can never be executed:
/// execution requires a complete `ProposedAction` and re-runs this with all
/// facts.
pub fn evaluate(
    actor: Option<&ActorContext>,
    draft: &ActionDraft,
    facts: &HardFacts,
    limits: &HardLimits,
) -> HardPolicyReport {
    let mut b = Builder { checks: vec![] };

    // 1. authenticated actor
    let Some(actor) = actor else {
        b.fail(
            "Authenticated actor",
            "no actor context on request",
            HardPolicyReason::NotAuthenticated,
        );
        return b.finish();
    };
    b.pass("Authenticated actor", "session resolved to an actor");

    // 2. tool is allow-listed (typed enum; unknown names never get this far)
    let spec = tool_spec(draft.kind());
    b.pass(
        "Action allow-listed",
        format!("{} is a registered tool", draft.kind()),
    );

    // 3. permission
    if actor.has(spec.required_permission) {
        b.pass(
            "Permission",
            format!("actor holds {:?}", spec.required_permission),
        );
    } else {
        b.fail(
            "Permission",
            format!("actor lacks {:?}", spec.required_permission),
            HardPolicyReason::MissingPermission,
        );
    }

    match draft {
        ActionDraft::ReadBalance { .. } => account_checks(&mut b, actor, facts),
        ActionDraft::FreezeCard { .. } => match &facts.card {
            Some(c) if c.owner == actor.user_id => {
                b.pass("Card ownership", "card belongs to actor");
                b.pass(
                    "Card state valid",
                    match c.state {
                        CardState::Active => "card is active",
                        CardState::Frozen => "card already frozen; freeze is idempotent",
                    },
                );
            }
            Some(_) => b.fail(
                "Card ownership",
                "card belongs to someone else",
                HardPolicyReason::CardNotOwned,
            ),
            None => b.fail(
                "Card ownership",
                "card not found",
                HardPolicyReason::CardNotOwned,
            ),
        },
        ActionDraft::TransferFunds { amount, .. } => {
            account_checks(&mut b, actor, facts);

            match &facts.recipient {
                RecipientLookup::NotRequested => {
                    b.skip("Recipient valid", "no recipient specified yet");
                    b.skip("Recipient not blocked", "no recipient specified yet");
                }
                RecipientLookup::NotFound => {
                    b.fail(
                        "Recipient valid",
                        "recipient does not exist for this user",
                        HardPolicyReason::RecipientNotFound,
                    );
                    b.skip("Recipient not blocked", "recipient unresolved");
                }
                RecipientLookup::Found(r) => {
                    if r.owner == actor.user_id {
                        b.pass("Recipient valid", "recipient exists");
                    } else {
                        b.fail(
                            "Recipient valid",
                            "recipient not in actor's address book",
                            HardPolicyReason::RecipientNotFound,
                        );
                    }
                    if r.blocked {
                        b.fail(
                            "Recipient not blocked",
                            "recipient is blocked",
                            HardPolicyReason::RecipientBlocked,
                        );
                    } else {
                        b.pass("Recipient not blocked", "recipient is not blocked");
                    }
                }
            }

            match amount {
                None => {
                    b.skip("Positive amount", "amount not specified yet");
                    b.skip("Currency supported", "amount not specified yet");
                    b.skip("Sufficient funds", "amount not specified yet");
                    b.skip("Transfer under hard limit", "amount not specified yet");
                }
                Some(amt) => {
                    if amt.is_positive() {
                        b.pass("Positive amount", amt.pretty());
                    } else {
                        b.fail(
                            "Positive amount",
                            "amount must be greater than zero",
                            HardPolicyReason::InvalidAmount,
                        );
                    }
                    match &facts.account {
                        Some(a) if a.balance.currency != amt.currency => b.fail(
                            "Currency supported",
                            "amount currency differs from account currency",
                            HardPolicyReason::UnsupportedCurrency,
                        ),
                        _ => b.pass("Currency supported", amt.currency.code()),
                    }
                    match &facts.account {
                        Some(a) if a.balance.currency == amt.currency => {
                            if a.balance.amount_minor >= amt.amount_minor {
                                b.pass("Sufficient funds", "available balance covers the amount");
                            } else {
                                b.fail(
                                    "Sufficient funds",
                                    format!(
                                        "available {} < requested {}",
                                        a.balance.pretty(),
                                        amt.pretty()
                                    ),
                                    HardPolicyReason::InsufficientFunds,
                                );
                            }
                        }
                        Some(_) => b.skip("Sufficient funds", "currency mismatch"),
                        None => b.fail(
                            "Sufficient funds",
                            "source account not found",
                            HardPolicyReason::AccountNotOwned,
                        ),
                    }
                    if amt.currency == limits.absolute_transfer_limit.currency
                        && amt.amount_minor > limits.absolute_transfer_limit.amount_minor
                    {
                        b.fail(
                            "Transfer under hard limit",
                            format!(
                                "exceeds absolute limit {}",
                                limits.absolute_transfer_limit.pretty()
                            ),
                            HardPolicyReason::TransferLimitExceeded,
                        );
                    } else {
                        b.pass(
                            "Transfer under hard limit",
                            format!("within {}", limits.absolute_transfer_limit.pretty()),
                        );
                    }
                }
            }
        }
    }
    b.finish()
}

fn account_checks(b: &mut Builder, actor: &ActorContext, facts: &HardFacts) {
    match &facts.account {
        Some(a) if a.owner == Some(actor.user_id) => {
            b.pass("Account ownership", "account belongs to actor");
            if a.active {
                b.pass("Account active", "account is active");
            } else {
                b.fail(
                    "Account active",
                    "account is closed",
                    HardPolicyReason::AccountInactive,
                );
            }
        }
        Some(_) => b.fail(
            "Account ownership",
            "account belongs to someone else",
            HardPolicyReason::AccountNotOwned,
        ),
        None => b.fail(
            "Account ownership",
            "account not found",
            HardPolicyReason::AccountNotOwned,
        ),
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use finguard_domain::*;

    pub fn actor() -> ActorContext {
        ActorContext {
            user_id: UserId(uuid::Uuid::from_u128(1)),
            session_id: SessionId::new(),
            permissions: vec![
                Permission::ReadAccount,
                Permission::FreezeCard,
                Permission::TransferFunds,
            ],
            device_id: DeviceId::new(),
        }
    }
    pub fn facts(balance_php: i64) -> HardFacts {
        let u = UserId(uuid::Uuid::from_u128(1));
        HardFacts {
            account: Some(AccountFacts {
                owner: Some(u),
                balance: Money::php_major(balance_php),
                active: true,
            }),
            card: Some(CardFacts {
                owner: u,
                state: CardState::Active,
            }),
            recipient: RecipientLookup::Found(RecipientFacts {
                owner: u,
                blocked: false,
            }),
        }
    }
    pub fn transfer(amount: Option<Money>) -> ActionDraft {
        ActionDraft::TransferFunds {
            from_account: AccountId::new(),
            recipient_id: Some(RecipientId::new()),
            amount,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;

    fn run(draft: &ActionDraft, f: &HardFacts) -> HardPolicyReport {
        evaluate(Some(&actor()), draft, f, &HardLimits::default())
    }

    #[test]
    fn normal_transfer_passes() {
        let r = run(&transfer(Some(Money::php_major(2_000))), &facts(120_000));
        assert_eq!(r.result, HardPolicyResult::Pass);
        assert!(r.checks.iter().all(|c| c.status == CheckStatus::Pass));
    }

    #[test]
    fn insufficient_funds_blocks() {
        let r = run(&transfer(Some(Money::php_major(40_000))), &facts(20_000));
        assert_eq!(
            r.result,
            HardPolicyResult::Block(HardPolicyReason::InsufficientFunds)
        );
    }

    #[test]
    fn invalid_amounts_block() {
        for a in [0, -100] {
            let r = run(&transfer(Some(Money::php(a))), &facts(20_000));
            assert_eq!(
                r.result,
                HardPolicyResult::Block(HardPolicyReason::InvalidAmount),
                "amount {a}"
            );
        }
    }

    #[test]
    fn exactly_the_balance_is_allowed() {
        let r = run(&transfer(Some(Money::php_major(20_000))), &facts(20_000));
        assert_eq!(r.result, HardPolicyResult::Pass);
    }

    #[test]
    fn absolute_limit_blocks_even_with_funds() {
        let r = run(&transfer(Some(Money::php_major(600_000))), &facts(900_000));
        assert_eq!(
            r.result,
            HardPolicyResult::Block(HardPolicyReason::TransferLimitExceeded)
        );
    }

    #[test]
    fn unauthorized_account_blocks() {
        let mut f = facts(100_000);
        f.account.as_mut().unwrap().owner = Some(UserId(uuid::Uuid::from_u128(99)));
        let r = run(&transfer(Some(Money::php_major(10))), &f);
        assert_eq!(
            r.result,
            HardPolicyResult::Block(HardPolicyReason::AccountNotOwned)
        );
    }

    #[test]
    fn missing_actor_and_missing_permission_block() {
        let d = transfer(Some(Money::php_major(10)));
        let r = evaluate(None, &d, &facts(100), &HardLimits::default());
        assert_eq!(
            r.result,
            HardPolicyResult::Block(HardPolicyReason::NotAuthenticated)
        );
        let mut a = actor();
        a.permissions
            .retain(|p| *p != finguard_domain::Permission::TransferFunds);
        let r = evaluate(Some(&a), &d, &facts(100_000), &HardLimits::default());
        assert_eq!(
            r.result,
            HardPolicyResult::Block(HardPolicyReason::MissingPermission)
        );
    }

    #[test]
    fn blocked_or_unknown_recipient_blocks() {
        let mut f = facts(100_000);
        f.recipient = RecipientLookup::Found(RecipientFacts {
            owner: UserId(uuid::Uuid::from_u128(1)),
            blocked: true,
        });
        assert_eq!(
            run(&transfer(Some(Money::php_major(10))), &f).result,
            HardPolicyResult::Block(HardPolicyReason::RecipientBlocked)
        );
        f.recipient = RecipientLookup::NotFound;
        assert_eq!(
            run(&transfer(Some(Money::php_major(10))), &f).result,
            HardPolicyResult::Block(HardPolicyReason::RecipientNotFound)
        );
    }

    #[test]
    fn incomplete_draft_skips_rather_than_passes() {
        let r = run(&transfer(None), &facts(100_000));
        assert_eq!(r.result, HardPolicyResult::Pass);
        let skipped: Vec<_> = r
            .checks
            .iter()
            .filter(|c| c.status == CheckStatus::Skipped)
            .map(|c| c.name.as_str())
            .collect();
        assert!(skipped.contains(&"Sufficient funds"));
        assert!(skipped.contains(&"Positive amount"));
    }

    #[test]
    fn freezing_an_already_frozen_card_passes() {
        let mut f = facts(0);
        f.card.as_mut().unwrap().state = CardState::Frozen;
        let d = ActionDraft::FreezeCard {
            card_id: finguard_domain::CardId::new(),
        };
        assert_eq!(run(&d, &f).result, HardPolicyResult::Pass);
    }
}
