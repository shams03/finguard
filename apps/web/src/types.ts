// Shapes returned by the Rust API. These mirror crates/agent/src/view.rs and
// crates/persistence/src/trace.rs.

export type Status =
  | "PROPOSED" | "VALIDATING" | "RISK_EVALUATING" | "AWAITING_CLARIFICATION"
  | "AWAITING_CONFIRMATION" | "MANUAL_REVIEW" | "APPROVED" | "EXECUTING"
  | "EXECUTED" | "BLOCKED" | "CANCELLED" | "FAILED" | "EXPIRED";

export interface ApiErrorBody { code: string; message: string; trace_id: string }

export interface HardCheck {
  name: string;
  status: "PASS" | "FAIL" | "SKIPPED";
  detail: string;
}

export interface Assessment {
  intent_alignment?: { yes: number; confidence: number } | null;
  context_sufficient?: { yes: number; confidence: number } | null;
  behavioral_anomaly?: Scored | null;
  recipient_risk?: Scored | null;
  overall_confidence: number;
  provider_metadata: {
    provider: string; model: string | null; latency_ms: number; question_set_version: string;
    input_tokens: number | null; output_tokens: number | null;
    credits_charged: number | null; credits_remaining: number | null;
  };
}
export interface Scored { score: number; labels: string[]; probabilities: number[]; confidence: number }

export interface JevExplorer {
  provider: string; status: string; model: string | null; latency_ms: number | null;
  question_set_version: string; overall_confidence: number | null; threshold: number;
  confidence_sufficient: boolean | null; assessment: Assessment | { error: string; detail: string } | null;
  credits_charged: number | null; input_tokens: number | null; output_tokens: number | null; note: string;
}

export interface EscalationExplorer {
  triggered: boolean;
  reason?: string; confidence?: number | null; required_threshold?: number;
  groq?: {
    purpose: string; status: string; model: string | null; latency_ms: number | null;
    result: {
      recommended_handling?: string; clarification_question?: string | null;
      missing_information?: string[]; clarification_needed?: boolean;
    } | null;
    input_tokens: number | null; output_tokens: number | null;
  } | null;
}

export interface ActionView {
  action_id: string;
  trace_id: string;
  parent_action_id: string | null;
  user_request: string;
  proposed_action: {
    type: "READ_BALANCE" | "FREEZE_CARD" | "TRANSFER_FUNDS";
    label: string; risk_class: string;
    from_account: string | null; recipient: string | null; card: string | null;
    amount: string | null; amount_pretty: string | null; missing_fields: string[];
  };
  status: Status;
  policy_decision: string | null;
  decision_reasons: { code: string; detail: string }[];
  clarification: { question: string; suggested_amount_minor: number | null; suggested_amount: string | null } | null;
  confirmation: { confirmation_id: string; expires_at: string; used: boolean } | null;
  result: Record<string, unknown> | null;
  explorer: {
    hard_policy: { checks: HardCheck[]; result: { result: string; reason?: string } } | null;
    jev: JevExplorer | null;
    escalation: EscalationExplorer;
    policy_version: string;
    versions: Record<string, string>;
  };
  error?: ApiErrorBody;
  notice?: { title: string; message: string };
  replayed?: boolean;
  created_at: string;
  updated_at: string;
}

export interface AccountResp {
  account: { id: string; name: string; currency: string; balance_minor: number; balance: string; status: string };
  profile: {
    customer_name: string; device_label: string; device_trusted: boolean; current_location: string;
    normal_location: string; location_familiar: boolean; typical_transfer_min: string; typical_transfer_max: string;
  };
}
export interface CardView { id: string; last4: string; state: "ACTIVE" | "FROZEN" }
export interface RecipientView { id: string; display_name: string; account_masked: string; age_days: number; known: boolean; blocked: boolean }
export interface TransactionView { id: string; kind: string; category: string; description: string; amount_minor: number; amount: string; occurred_at: string }

export interface TraceEvent {
  seq: number; event_type: string; label: string; occurred_at: string; offset_ms: number;
  duration_ms: number | null; category: string; metadata: Record<string, unknown>;
}
export interface TraceStage { key: string; label: string; status: "passed" | "warning" | "blocked" | "skipped" | "executed" | "pending"; detail: string }
export interface TraceView {
  trace_id: string; action_id: string; parent_action_id: string | null; status: string; action_type: string;
  risk_class: string; started_at: string; completed_at: string | null; total_duration_ms: number;
  policy_version: string; versions: Record<string, string>; user_request: string;
  policy_decision: string | null; decision_reasons: { code: string; detail: string }[];
  stages: TraceStage[]; events: TraceEvent[];
  jev: Record<string, unknown>[]; groq: Record<string, unknown>[];
  latency: { intent_parsing_ms: number; hard_policy_ms: number; jev_assessment_ms: number; groq_escalation_ms: number; db_execution_ms: number };
  usage: { groq_input_tokens: number; groq_output_tokens: number; jev_credits_charged: number | null; jev_input_tokens: number; jev_output_tokens: number };
  confirmation: Record<string, unknown> | null;
  ledger: { ledger_transaction_id: string; entries: { account: string; amount: string; amount_minor: number }[] } | null;
  result: Record<string, unknown> | null;
  audit_answers: [string, string][];
}

export interface ActionListItem { action_id: string; trace_id: string; kind: string; status: Status; policy_decision: string | null; user_request: string; created_at: string }
export interface HealthResp { status: string; database: string; app_env: string; providers: Record<string, unknown> }
