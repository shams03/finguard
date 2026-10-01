use finguard_domain::RiskClass;

/// Minimum Jev confidence for a class of action before we stop trusting the
/// assessment alone and escalate for extra handling.
///
/// These are **demo parameters, not validated financial thresholds**. In
/// production they must be calibrated on representative historical evaluation
/// data with asymmetric error costs (a wrongly-allowed transfer costs far more
/// than a wrongly-escalated one).
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
            default: 0.85,
            read_only: Some(0.70),
            reversible: Some(0.80),
            money_movement: Some(0.90),
            high_impact: Some(0.95),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_per_class_and_fallback() {
        let t = ConfidenceThresholds::default();
        assert_eq!(t.for_class(RiskClass::ReadOnly), 0.70);
        assert_eq!(t.for_class(RiskClass::MoneyMovement), 0.90);
        let only_default = ConfidenceThresholds {
            default: 0.85,
            read_only: None,
            reversible: None,
            money_movement: None,
            high_impact: None,
        };
        assert_eq!(only_default.for_class(RiskClass::HighImpact), 0.85);
    }
}
