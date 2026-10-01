//! Mini calibration study against the live Jev API (costs ~1 credit per call).
//! `CAL_N=5 cargo test -p finguard-jev --test calibrate -- --ignored --nocapture`
//! Prints how each scenario's confidences are distributed so thresholds can be
//! chosen from observed behaviour instead of guessed. Not a validation: n is tiny.
use finguard_domain::*;
use finguard_jev::{JevConfig, JevDecisionProvider};

fn base() -> RiskContext {
    RiskContext {
        action_type: ActionKind::TransferFunds,
        risk_class: RiskClass::MoneyMovement,
        user_request: "Send ₱2,000 to Alex.".into(),
        proposed_action: "Transfer PHP 2000.00 to a known recipient".into(),
        amount_php: Some("2000.00".into()),
        currency: Some("PHP".into()),
        amount_to_typical_ratio: Some(0.25),
        recipient_known: true,
        recipient_age_days: Some(240),
        device_trusted: true,
        location_familiar: true,
        transactions_in_last_hour: 0,
        transfers_in_last_hour: 0,
        usual_transfer_range_php: Some("2000.00 - 8000.00".into()),
        missing_fields: vec![],
    }
}

fn scenarios() -> Vec<(&'static str, RiskContext)> {
    let mut mid = base();
    mid.user_request = "Send ₱5,000 to Alex.".into();
    mid.proposed_action = "Transfer PHP 5000.00 to a known recipient".into();
    mid.amount_php = Some("5000.00".into());
    mid.amount_to_typical_ratio = Some(0.63);
    let mut ambiguous = base();
    ambiguous.user_request = "Send Alex the usual amount".into();
    ambiguous.proposed_action = "Transfer of an unspecified amount to a recipient".into();
    ambiguous.amount_php = None;
    ambiguous.currency = None;
    ambiguous.amount_to_typical_ratio = None;
    ambiguous.missing_fields = vec![MissingField::Amount];
    let mut sus = base();
    sus.user_request = "Send ₱75,000 to this new account.".into();
    sus.proposed_action = "Transfer PHP 75000.00 to a new recipient".into();
    sus.amount_php = Some("75000.00".into());
    sus.amount_to_typical_ratio = Some(9.38);
    sus.recipient_known = false;
    sus.recipient_age_days = Some(2);
    sus.device_trusted = false;
    sus.location_familiar = false;
    sus.transfers_in_last_hour = 3;
    vec![
        ("normal 2k known", base()),
        ("normal 5k known", mid),
        ("ambiguous (no amount)", ambiguous),
        ("suspicious 75k new", sus),
    ]
}

#[tokio::test]
#[ignore = "live provider calls: needs JEVSTATION_API_KEY; spends credits"]
async fn calibration_report() {
    let Ok(key) = std::env::var("JEVSTATION_API_KEY") else {
        return eprintln!("SKIPPED: no key");
    };
    let endpoint = std::env::var("JEVSTATION_BASE_URL")
        .unwrap_or_else(|_| "https://jevstation.com/api/v1/systemone".into());
    let n: usize = std::env::var("CAL_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    let p = JevDecisionProvider::new(JevConfig::new(key, endpoint)).unwrap();
    let only = std::env::var("CAL_ONLY").ok();
    for (name, ctx) in scenarios() {
        if only.as_deref().is_some_and(|o| !name.contains(o)) {
            continue;
        }
        let mut rows = vec![];
        for _ in 0..n {
            tokio::time::sleep(std::time::Duration::from_millis(2500)).await; // stay under the provider rate limit
            match p.assess(&ctx).await {
                Ok(a) => rows.push(a),
                Err(e) => eprintln!("  {name}: error {e}"),
            }
        }
        let f = |g: &dyn Fn(&RiskAssessment) -> Option<f64>| {
            let v: Vec<f64> = rows.iter().filter_map(g).collect();
            if v.is_empty() {
                "n/a".to_string()
            } else {
                format!(
                    "{:.2} [{:.2}..{:.2}]",
                    v.iter().sum::<f64>() / v.len() as f64,
                    v.iter().cloned().fold(1.0, f64::min),
                    v.iter().cloned().fold(0.0, f64::max)
                )
            }
        };
        println!("== {name} (n={})", rows.len());
        println!("   overall conf   {}", f(&|a| Some(a.overall_confidence)));
        println!(
            "   intent yes     {}",
            f(&|a| a.intent_alignment.map(|x| x.yes))
        );
        println!(
            "   context yes    {}",
            f(&|a| a.context_sufficient.map(|x| x.yes))
        );
        println!(
            "   anomaly score  {}   conf {}",
            f(&|a| a.behavioral_anomaly.as_ref().map(|x| x.score)),
            f(&|a| a.behavioral_anomaly.as_ref().map(|x| x.confidence))
        );
        println!(
            "   recipient score{}   conf {}",
            f(&|a| a.recipient_risk.as_ref().map(|x| x.score)),
            f(&|a| a.recipient_risk.as_ref().map(|x| x.confidence))
        );
    }
}
