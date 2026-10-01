//! Vendor JSON -> domain `RiskAssessment`. Accepts both the JevStation
//! envelope (`{code, message, data:{...}}`) and TypeSafe's bare response.

use crate::questions::*;
use finguard_domain::*;
use serde_json::Value;

fn bad(msg: impl Into<String>) -> DecisionProviderError {
    DecisionProviderError::InvalidResponse(msg.into())
}

/// Strip the optional JevStation envelope.
fn unwrap_envelope(v: &Value) -> Result<&Value, DecisionProviderError> {
    if let (Some(code), Some(data)) = (v.get("code"), v.get("data")) {
        if code.as_i64() != Some(0) {
            let msg = v
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("provider reported an error");
            return Err(bad(format!(
                "provider error envelope: {}",
                msg.chars().take(120).collect::<String>()
            )));
        }
        return Ok(data);
    }
    Ok(v)
}

fn number(v: &Value, what: &str) -> Result<f64, DecisionProviderError> {
    v.as_f64()
        .filter(|f| f.is_finite())
        .ok_or_else(|| bad(format!("{what} is not a finite number")))
}

fn parse_noul(a: &Value, id: &str) -> Result<ProbabilityAssessment, DecisionProviderError> {
    if a.get("type").and_then(Value::as_str) != Some("noul") {
        return Err(bad(format!("{id}: expected a noul answer")));
    }
    let p = number(&a["noul"], &format!("{id}.noul"))?;
    if !(0.0..=1.0).contains(&p) {
        return Err(bad(format!("{id}.noul outside 0..1")));
    }
    Ok(ProbabilityAssessment::from_yes_probability(p))
}

fn parse_score(
    a: &Value,
    id: &str,
    expected_levels: usize,
) -> Result<ScoredAssessment, DecisionProviderError> {
    if a.get("type").and_then(Value::as_str) != Some("score") {
        return Err(bad(format!("{id}: expected a score answer")));
    }
    let score = number(&a["score"], &format!("{id}.score"))?;
    let confidence = number(&a["confidence"], &format!("{id}.confidence"))?;
    if !(0.0..=1.0).contains(&confidence) {
        return Err(bad(format!("{id}.confidence outside 0..1")));
    }
    let probs = a
        .get("probabilities")
        .and_then(Value::as_object)
        .ok_or_else(|| bad(format!("{id}.probabilities missing")))?;
    let mut indexed: Vec<(usize, f64)> = Vec::with_capacity(probs.len());
    for (k, v) in probs {
        let idx: usize = k
            .parse()
            .map_err(|_| bad(format!("{id}: non-numeric level '{k}'")))?;
        indexed.push((idx, number(v, &format!("{id}.probabilities.{k}"))?));
    }
    indexed.sort_by_key(|(i, _)| *i);
    if indexed.len() != expected_levels || indexed.iter().enumerate().any(|(pos, (i, _))| pos != *i)
    {
        return Err(bad(format!(
            "{id}: expected levels 0..{}",
            expected_levels - 1
        )));
    }
    let sum: f64 = indexed.iter().map(|(_, p)| p).sum();
    if !(0.9..=1.1).contains(&sum) || indexed.iter().any(|(_, p)| *p < -1e-9 || *p > 1.0 + 1e-9) {
        return Err(bad(format!(
            "{id}: probabilities do not form a distribution"
        )));
    }
    if score < -1e-9 || score > (expected_levels - 1) as f64 + 1e-9 {
        return Err(bad(format!("{id}.score outside rubric range")));
    }
    let labels = if id == RECIPIENT_RISK {
        RECIPIENT_LEVELS
    } else {
        ANOMALY_LEVELS
    };
    Ok(ScoredAssessment {
        score,
        labels: labels.iter().map(|s| (*s).to_owned()).collect(),
        probabilities: indexed.into_iter().map(|(_, p)| p).collect(),
        confidence,
    })
}

pub struct ParseInput<'a> {
    pub body: &'a Value,
    pub requested_questions: &'a Value,
    pub provider: &'a str,
    pub latency_ms: u64,
}

pub fn parse_response(input: ParseInput<'_>) -> Result<RiskAssessment, DecisionProviderError> {
    let data = unwrap_envelope(input.body)?;
    let answers = data
        .get("answers")
        .and_then(Value::as_object)
        .ok_or_else(|| bad("response has no answers object"))?;
    let asked = input
        .requested_questions
        .as_object()
        .ok_or_else(|| bad("internal: question set not an object"))?;

    // Every question we asked must be answered, and with the right type.
    for id in asked.keys() {
        if !answers.contains_key(id) {
            return Err(bad(format!("missing answer for {id}")));
        }
    }
    let get = |id: &str| answers.get(id).filter(|_| asked.contains_key(id));
    let intent_alignment = get(INTENT_ALIGNMENT)
        .map(|a| parse_noul(a, INTENT_ALIGNMENT))
        .transpose()?;
    let context_sufficient = get(CONTEXT_SUFFICIENT)
        .map(|a| parse_noul(a, CONTEXT_SUFFICIENT))
        .transpose()?;
    let behavioral_anomaly = get(BEHAVIORAL_ANOMALY)
        .map(|a| parse_score(a, BEHAVIORAL_ANOMALY, ANOMALY_LEVELS.len()))
        .transpose()?;
    let recipient_risk = get(RECIPIENT_RISK)
        .map(|a| parse_score(a, RECIPIENT_RISK, RECIPIENT_LEVELS.len()))
        .transpose()?;

    let usage = data.get("usage");
    let credits = data.get("credits");
    let mut a = RiskAssessment {
        intent_alignment,
        behavioral_anomaly,
        recipient_risk,
        context_sufficient,
        overall_confidence: 0.0,
        provider_metadata: ProviderMetadata {
            provider: input.provider.to_owned(),
            model: data
                .get("model")
                .and_then(Value::as_str)
                .map(|s| s.chars().take(64).collect()),
            latency_ms: input.latency_ms,
            question_set_version: RISK_QUESTION_SET_VERSION.to_owned(),
            input_tokens: usage
                .and_then(|u| u.get("input_tokens"))
                .and_then(Value::as_u64),
            output_tokens: usage
                .and_then(|u| u.get("output_tokens"))
                .and_then(Value::as_u64),
            credits_charged: credits
                .and_then(|c| c.get("charged"))
                .and_then(Value::as_i64),
            credits_remaining: credits
                .and_then(|c| c.get("remaining"))
                .and_then(Value::as_i64),
        },
        raw: serde_json::json!({ "questions": input.requested_questions, "answers": answers }),
    };
    a.overall_confidence = a.weakest_link_confidence();
    Ok(a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn questions() -> Value {
        json!({
            "intent_alignment": {"type":"noul"},
            "behavioral_anomaly": {"type":"score"},
            "recipient_risk": {"type":"score"},
            "context_sufficient": {"type":"noul"},
        })
    }
    fn answers(anomaly_conf: f64) -> Value {
        json!({
            "intent_alignment": {"type":"noul","noul":0.98},
            "behavioral_anomaly": {"type":"score","score":0.2,"confidence":anomaly_conf,
                "legend":{"0":"a","1":"b","2":"c","3":"d","4":"e"},
                "probabilities":{"0":0.81,"1":0.14,"2":0.04,"3":0.01,"4":0.0}},
            "recipient_risk": {"type":"score","score":0.1,"confidence":0.95,
                "probabilities":{"0":0.9,"1":0.08,"2":0.02,"3":0.0,"4":0.0}},
            "context_sufficient": {"type":"noul","noul":0.97},
        })
    }
    fn parse(body: &Value) -> Result<RiskAssessment, DecisionProviderError> {
        parse_response(ParseInput {
            body,
            requested_questions: &questions(),
            provider: "jevstation",
            latency_ms: 233,
        })
    }

    #[test]
    fn jevstation_envelope_with_credits_and_usage() {
        let body = json!({"code":0,"message":"ok","data":{
            "model":"jev-1.13.0","answers":answers(0.93),
            "usage":{"input_tokens":392,"output_tokens":65},
            "credits":{"charged":1,"remaining":199},"latency_ms":400}});
        let a = parse(&body).unwrap();
        assert_eq!(a.provider_metadata.model.as_deref(), Some("jev-1.13.0"));
        assert_eq!(a.provider_metadata.credits_charged, Some(1));
        assert_eq!(
            a.provider_metadata.latency_ms, 233,
            "we report our own measured latency"
        );
        assert!((a.overall_confidence - 0.93).abs() < 1e-9);
        assert_eq!(a.behavioral_anomaly.unwrap().probabilities.len(), 5);
    }

    #[test]
    fn typesafe_bare_response() {
        let body = json!({"model":"jev-1.13.0","answers":answers(0.9),"usage":{"input_tokens":10,"output_tokens":2}});
        let a = parse(&body).unwrap();
        assert_eq!(a.provider_metadata.credits_charged, None);
        assert_eq!(a.provider_metadata.input_tokens, Some(10));
    }

    #[test]
    fn low_confidence_is_reflected() {
        let body = json!({"answers":answers(0.63)});
        assert!((parse(&body).unwrap().overall_confidence - 0.63).abs() < 1e-9);
    }

    #[test]
    fn error_envelope_is_invalid_response() {
        let body = json!({"code":-1,"message":"evaluation failed","data":null});
        assert!(matches!(
            parse(&body),
            Err(DecisionProviderError::InvalidResponse(_))
        ));
    }

    #[test]
    fn malformed_shapes_are_rejected() {
        // missing answer
        let mut a = answers(0.9);
        a.as_object_mut().unwrap().remove("recipient_risk");
        assert!(parse(&json!({"answers":a})).is_err());
        // wrong type
        let mut a = answers(0.9);
        a["intent_alignment"] = json!({"type":"score","score":1.0});
        assert!(parse(&json!({"answers":a})).is_err());
        // out of range / not a distribution
        let mut a = answers(0.9);
        a["intent_alignment"]["noul"] = json!(1.4);
        assert!(parse(&json!({"answers":a})).is_err());
        let mut a = answers(0.9);
        a["behavioral_anomaly"]["probabilities"] = json!({"0":0.1,"1":0.1,"2":0.1,"3":0.1,"4":0.1});
        assert!(parse(&json!({"answers":a})).is_err());
        // not even an object
        assert!(parse(&json!("lol")).is_err());
        assert!(parse(&json!({})).is_err());
    }
}
