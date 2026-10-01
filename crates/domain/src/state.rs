//! The action lifecycle as an explicit state machine. Illegal transitions are
//! errors; there is no boolean soup like `confirmed && !executed`.

use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActionStatus {
    Proposed,
    Validating,
    RiskEvaluating,
    AwaitingClarification,
    AwaitingConfirmation,
    ManualReview,
    Approved,
    Executing,
    Executed,
    Blocked,
    Cancelled,
    Failed,
    Expired,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("illegal transition {from} -> {to}")]
pub struct IllegalTransition {
    pub from: ActionStatus,
    pub to: ActionStatus,
}

impl ActionStatus {
    pub fn as_str(self) -> &'static str {
        use ActionStatus::*;
        match self {
            Proposed => "PROPOSED",
            Validating => "VALIDATING",
            RiskEvaluating => "RISK_EVALUATING",
            AwaitingClarification => "AWAITING_CLARIFICATION",
            AwaitingConfirmation => "AWAITING_CONFIRMATION",
            ManualReview => "MANUAL_REVIEW",
            Approved => "APPROVED",
            Executing => "EXECUTING",
            Executed => "EXECUTED",
            Blocked => "BLOCKED",
            Cancelled => "CANCELLED",
            Failed => "FAILED",
            Expired => "EXPIRED",
        }
    }

    pub fn is_terminal(self) -> bool {
        use ActionStatus::*;
        matches!(self, Executed | Blocked | Cancelled | Failed | Expired)
    }

    /// States reachable in one step.
    pub fn allowed_next(self) -> &'static [ActionStatus] {
        use ActionStatus::*;
        match self {
            Proposed => &[Validating, Failed],
            Validating => &[RiskEvaluating, Blocked, Failed],
            RiskEvaluating => &[
                AwaitingClarification,
                AwaitingConfirmation,
                ManualReview,
                Approved,
                Blocked,
                Failed,
            ],
            AwaitingClarification => &[Cancelled, Expired],
            AwaitingConfirmation => &[Approved, Cancelled, Expired, Blocked],
            // Manual review is a parking state: no funds move, a reviewer (or
            // the demo reset) can only close it out.
            ManualReview => &[Cancelled, Expired],
            Approved => &[Executing, Cancelled, Expired],
            Executing => &[Executed, Blocked, Failed],
            Executed | Blocked | Cancelled | Failed | Expired => &[],
        }
    }

    pub fn can_transition_to(self, to: ActionStatus) -> bool {
        self.allowed_next().contains(&to)
    }

    pub fn transition(self, to: ActionStatus) -> Result<ActionStatus, IllegalTransition> {
        if self.can_transition_to(to) {
            Ok(to)
        } else {
            Err(IllegalTransition { from: self, to })
        }
    }
}

impl fmt::Display for ActionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ActionStatus {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        use ActionStatus::*;
        Ok(match s {
            "PROPOSED" => Proposed,
            "VALIDATING" => Validating,
            "RISK_EVALUATING" => RiskEvaluating,
            "AWAITING_CLARIFICATION" => AwaitingClarification,
            "AWAITING_CONFIRMATION" => AwaitingConfirmation,
            "MANUAL_REVIEW" => ManualReview,
            "APPROVED" => Approved,
            "EXECUTING" => Executing,
            "EXECUTED" => Executed,
            "BLOCKED" => Blocked,
            "CANCELLED" => Cancelled,
            "FAILED" => Failed,
            "EXPIRED" => Expired,
            _ => return Err(()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ActionStatus::*;
    use super::*;

    const ALL: [ActionStatus; 13] = [
        Proposed,
        Validating,
        RiskEvaluating,
        AwaitingClarification,
        AwaitingConfirmation,
        ManualReview,
        Approved,
        Executing,
        Executed,
        Blocked,
        Cancelled,
        Failed,
        Expired,
    ];

    #[test]
    fn happy_path() {
        let mut s = Proposed;
        for next in [
            Validating,
            RiskEvaluating,
            AwaitingConfirmation,
            Approved,
            Executing,
            Executed,
        ] {
            s = s.transition(next).unwrap();
        }
        assert!(s.is_terminal());
    }

    #[test]
    fn spec_branches() {
        assert!(RiskEvaluating.can_transition_to(AwaitingClarification));
        assert!(RiskEvaluating.can_transition_to(ManualReview));
        assert!(Validating.can_transition_to(Blocked));
    }

    #[test]
    fn terminal_states_have_no_exits_and_money_cannot_skip_confirmation() {
        for s in ALL {
            if s.is_terminal() {
                assert!(s.allowed_next().is_empty(), "{s} should be terminal");
            }
        }
        // A transfer cannot jump from "needs clarification" or "manual review" to execution.
        assert!(!AwaitingClarification.can_transition_to(Approved));
        assert!(!ManualReview.can_transition_to(Approved));
        assert!(!ManualReview.can_transition_to(Executing));
        assert!(!Proposed.can_transition_to(Executing));
        assert!(!Executed.can_transition_to(Executing));
        assert_eq!(
            Executed.transition(Executing),
            Err(IllegalTransition {
                from: Executed,
                to: Executing
            })
        );
    }

    #[test]
    fn string_roundtrip() {
        for s in ALL {
            assert_eq!(s.as_str().parse::<ActionStatus>().unwrap(), s);
        }
    }
}
