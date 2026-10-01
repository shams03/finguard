use finguard_domain::RiskClass;

/// Minimum Jev confidence for a class of action before we stop trusting the
/// assessment alone and escalate for extra handling.
///
/// These are **demo parameters, not validated financial thresholds**. The
/// defaults were chosen from a tiny live study against Jev (2026-10-01, see
/// `docs/research/calibration.md`): normal transfers scored 0.76–0.88 weakest-link
/// confidence, ambiguous requests 0.26–0.34, suspicious ones 0.52–0.64. An
/// initial guess of 0.90 for money movement would have escalated every normal
/// transfer. In production they must be calibrated on representative historical
/// evaluation data with asymmetric error costs (a wrongly-allowed transfer costs
/// far more than a wrongly-escalated one).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConfidenceThresholds {
    pub default: f64,
    pub read_only: Option<f64>,
    pub reversible: Option<f64>,
    pub money_movement: Option<f64>,
    pub high_impact: Option<f64>,
}

impl ConfidenceThresholds {
    pub fn for_class(&self, class: RiskClass) -> f64 {
        match class {
            RiskClass::ReadOnly => self.read_only,
            RiskClass::Reversible => self.reversible,
            RiskClass::MoneyMovement => self.money_movement,
            RiskClass::HighImpact => self.high_impact,
        }
        .unwrap_or(self.default)
    }
}

impl Default for ConfidenceThresholds {
    fn default() -> Self {
        Self {
            default: 0.65,
            read_only: Some(0.50),
            reversible: Some(0.60),
            money_movement: Some(0.70),
            high_impact: Some(0.80),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_per_class_and_fallback() {
        let t = ConfidenceThresholds::default();
        assert_eq!(t.for_class(RiskClass::ReadOnly), 0.50);
        assert_eq!(t.for_class(RiskClass::MoneyMovement), 0.70);
        assert!(
            t.for_class(RiskClass::ReadOnly) < t.for_class(RiskClass::Reversible)
                && t.for_class(RiskClass::Reversible) < t.for_class(RiskClass::MoneyMovement)
                && t.for_class(RiskClass::MoneyMovement) < t.for_class(RiskClass::HighImpact),
            "riskier classes need higher confidence"
        );
        let only_default = ConfidenceThresholds {
            default: 0.65,
            read_only: None,
            reversible: None,
            money_movement: None,
            high_impact: None,
        };
        assert_eq!(only_default.for_class(RiskClass::HighImpact), 0.65);
    }
}
