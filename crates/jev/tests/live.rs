//! Live Jev smoke test. Ignored by default; run with `make test-live`.
//! Skips with a clear message when no key is configured, so CI without
//! secrets stays green.
use finguard_domain::*;
use finguard_jev::{JevConfig, JevDecisionProvider};

#[tokio::test]
#[ignore = "live provider call: needs JEVSTATION_API_KEY"]
async fn live_jev_returns_a_valid_assessment() {
    let Ok(key) = std::env::var("JEVSTATION_API_KEY") else {
        eprintln!("SKIPPED: JEVSTATION_API_KEY not set");
        return;
    };
    let endpoint = std::env::var("JEVSTATION_BASE_URL")
        .unwrap_or_else(|_| "https://jevstation.com/api/v1/systemone".into());
    let p = JevDecisionProvider::new(JevConfig::new(key, endpoint)).unwrap();
    let ctx = RiskContext {
        action_type: ActionKind::TransferFunds,
        risk_class: RiskClass::MoneyMovement,
        user_request: "Send 2000 pesos to Alex".into(),
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
    };
    let a = p.assess(&ctx).await.expect("live Jev call failed");
    println!("{:#?}", a.provider_metadata);
    println!(
        "intent={:?}\ncontext={:?}",
        a.intent_alignment, a.context_sufficient
    );
    println!(
        "anomaly={:?}\nrecipient={:?}",
        a.behavioral_anomaly
            .as_ref()
            .map(|s| (s.score, s.confidence, &s.probabilities)),
        a.recipient_risk
            .as_ref()
            .map(|s| (s.score, s.confidence, &s.probabilities))
    );
    println!("overall_confidence = {:.2}", a.overall_confidence);
    assert!((0.0..=1.0).contains(&a.overall_confidence));
    assert!(a.behavioral_anomaly.is_some());
}
