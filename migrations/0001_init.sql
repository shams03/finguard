-- FinGuard schema. Plain PostgreSQL only (no Supabase-specific features), so the
-- same migration runs on Docker Postgres locally and on Supabase in production.
--
-- Money is BIGINT minor units (PHP centavos). Financial history tables are
-- append-only, enforced by triggers, not by application discipline.

------------------------------------------------------------------------------
-- Identity (synthetic demo user)
------------------------------------------------------------------------------
CREATE TABLE users (
    id                          UUID PRIMARY KEY,
    display_name                TEXT        NOT NULL,
    primary_currency            TEXT        NOT NULL CHECK (primary_currency IN ('PHP')),
    typical_transfer_min_minor  BIGINT      NOT NULL CHECK (typical_transfer_min_minor >= 0),
    typical_transfer_max_minor  BIGINT      NOT NULL CHECK (typical_transfer_max_minor >= typical_transfer_min_minor),
    normal_location             TEXT        NOT NULL,
    created_at                  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE devices (
    id         UUID PRIMARY KEY,
    user_id    UUID        NOT NULL REFERENCES users(id),
    label      TEXT        NOT NULL,
    trusted    BOOLEAN     NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Stand-in for a real identity provider session (see docs/SECURITY.md).
CREATE TABLE demo_sessions (
    id               UUID PRIMARY KEY,
    user_id          UUID        NOT NULL REFERENCES users(id),
    device_id        UUID        NOT NULL REFERENCES devices(id),
    current_location TEXT        NOT NULL,
    permissions      TEXT[]      NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

------------------------------------------------------------------------------
-- Accounts, cards, recipients
------------------------------------------------------------------------------
CREATE TABLE accounts (
    id            UUID PRIMARY KEY,
    owner_user_id UUID REFERENCES users(id),
    name          TEXT        NOT NULL,
    kind          TEXT        NOT NULL CHECK (kind IN ('CUSTOMER', 'SETTLEMENT')),
    currency      TEXT        NOT NULL CHECK (currency IN ('PHP')),
    -- Materialised balance, maintained in the same transaction as ledger entries.
    balance_minor BIGINT      NOT NULL,
    status        TEXT        NOT NULL DEFAULT 'ACTIVE' CHECK (status IN ('ACTIVE', 'CLOSED')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Last line of defence against overspending: even a bug in Rust cannot
    -- push a customer account negative.
    CONSTRAINT customer_balance_non_negative
        CHECK (kind = 'SETTLEMENT' OR (owner_user_id IS NOT NULL AND balance_minor >= 0))
);

CREATE TABLE cards (
    id         UUID PRIMARY KEY,
    user_id    UUID        NOT NULL REFERENCES users(id),
    account_id UUID        NOT NULL REFERENCES accounts(id),
    last4      TEXT        NOT NULL CHECK (last4 ~ '^[0-9]{4}$'),
    state      TEXT        NOT NULL CHECK (state IN ('ACTIVE', 'FROZEN')),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE recipients (
    id             UUID PRIMARY KEY,
    user_id        UUID        NOT NULL REFERENCES users(id),
    display_name   TEXT        NOT NULL,
    account_masked TEXT        NOT NULL,
    blocked        BOOLEAN     NOT NULL DEFAULT FALSE,
    -- Recipient "age" is derived from this (e.g. 240 days for Alex).
    created_at     TIMESTAMPTZ NOT NULL
);

-- User-facing history (display + velocity + "usual amount" evidence). The
-- ledger below is the source of truth for balances.
CREATE TABLE transactions (
    id                    UUID PRIMARY KEY,
    account_id            UUID        NOT NULL REFERENCES accounts(id),
    kind                  TEXT        NOT NULL,
    category              TEXT        NOT NULL,
    description           TEXT        NOT NULL,
    amount_minor          BIGINT      NOT NULL, -- signed: negative = money out
    currency              TEXT        NOT NULL CHECK (currency IN ('PHP')),
    recipient_id          UUID REFERENCES recipients(id),
    ledger_transaction_id UUID,
    occurred_at           TIMESTAMPTZ NOT NULL
);
CREATE INDEX transactions_account_time ON transactions (account_id, occurred_at DESC);
CREATE INDEX transactions_recipient_time ON transactions (recipient_id, occurred_at DESC)
    WHERE recipient_id IS NOT NULL;

------------------------------------------------------------------------------
-- Actions: one row per proposed action, with an explicit status
------------------------------------------------------------------------------
CREATE TABLE actions (
    id                          UUID PRIMARY KEY,
    trace_id                    UUID        NOT NULL,
    user_id                     UUID        NOT NULL REFERENCES users(id),
    session_id                  UUID        NOT NULL,
    parent_action_id            UUID REFERENCES actions(id),
    kind                        TEXT        NOT NULL CHECK (kind IN ('READ_BALANCE', 'FREEZE_CARD', 'TRANSFER_FUNDS')),
    risk_class                  TEXT        NOT NULL CHECK (risk_class IN ('READ_ONLY', 'REVERSIBLE', 'MONEY_MOVEMENT', 'HIGH_IMPACT')),
    status                      TEXT        NOT NULL CHECK (status IN (
                                    'PROPOSED', 'VALIDATING', 'RISK_EVALUATING', 'AWAITING_CLARIFICATION',
                                    'AWAITING_CONFIRMATION', 'MANUAL_REVIEW', 'APPROVED', 'EXECUTING',
                                    'EXECUTED', 'BLOCKED', 'CANCELLED', 'FAILED', 'EXPIRED')),
    user_text                   TEXT        NOT NULL, -- scrubbed summary, not raw
    account_id                  UUID REFERENCES accounts(id),
    card_id                     UUID REFERENCES cards(id),
    recipient_id                UUID REFERENCES recipients(id),
    amount_minor                BIGINT,
    currency                    TEXT CHECK (currency IN ('PHP')),
    missing_fields              TEXT[]      NOT NULL DEFAULT '{}',
    fingerprint                 TEXT,       -- NULL while the draft is incomplete
    policy_version              TEXT        NOT NULL,
    policy_decision             TEXT,
    decision_reasons            JSONB       NOT NULL DEFAULT '[]',
    hard_policy                 JSONB,
    clarification_question      TEXT,
    clarification_candidate_minor BIGINT,
    result                      JSONB,
    ledger_transaction_id       UUID,
    created_at                  TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    updated_at                  TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    completed_at                TIMESTAMPTZ
);
CREATE INDEX actions_user_time ON actions (user_id, created_at DESC);
CREATE INDEX actions_trace ON actions (trace_id);

-- A confirmation is bound to one action and one fingerprint, expires, and is
-- single-use. There is no global "confirmed = true".
CREATE TABLE confirmations (
    id                 UUID PRIMARY KEY,
    action_id          UUID        NOT NULL UNIQUE REFERENCES actions(id),
    user_id            UUID        NOT NULL REFERENCES users(id),
    action_fingerprint TEXT        NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    expires_at         TIMESTAMPTZ NOT NULL,
    used_at            TIMESTAMPTZ
);

CREATE TABLE idempotency_records (
    id                  UUID PRIMARY KEY,
    user_id             UUID        NOT NULL REFERENCES users(id),
    scope               TEXT        NOT NULL, -- e.g. 'command', 'confirm:<action_id>'
    idempotency_key     TEXT        NOT NULL,
    request_fingerprint TEXT        NOT NULL,
    action_id           UUID REFERENCES actions(id),
    status              TEXT        NOT NULL CHECK (status IN ('IN_PROGRESS', 'COMPLETED')),
    response            JSONB       NOT NULL DEFAULT '{}',
    created_at          TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT idempotency_unique UNIQUE (user_id, scope, idempotency_key)
);

------------------------------------------------------------------------------
-- Ledger: append-only, double-entry inspired
------------------------------------------------------------------------------
CREATE TABLE ledger_transactions (
    id          UUID PRIMARY KEY,
    action_id   UUID REFERENCES actions(id),
    kind        TEXT        NOT NULL,
    description TEXT        NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);

CREATE TABLE ledger_entries (
    id                    BIGSERIAL PRIMARY KEY,
    ledger_transaction_id UUID        NOT NULL REFERENCES ledger_transactions(id),
    account_id            UUID        NOT NULL REFERENCES accounts(id),
    amount_minor          BIGINT      NOT NULL CHECK (amount_minor <> 0),
    currency              TEXT        NOT NULL CHECK (currency IN ('PHP')),
    created_at            TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX ledger_entries_tx ON ledger_entries (ledger_transaction_id);
CREATE INDEX ledger_entries_account ON ledger_entries (account_id);

------------------------------------------------------------------------------
-- Audit + model evaluation records (append-only)
------------------------------------------------------------------------------
CREATE TABLE audit_events (
    seq         BIGSERIAL PRIMARY KEY, -- total order
    event_id    UUID        NOT NULL UNIQUE,
    trace_id    UUID        NOT NULL,
    action_id   UUID REFERENCES actions(id),
    event_type  TEXT        NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    metadata    JSONB       NOT NULL DEFAULT '{}'
);
CREATE INDEX audit_trace ON audit_events (trace_id, seq);
CREATE INDEX audit_action ON audit_events (action_id, seq);

CREATE TABLE jev_evaluations (
    id                   UUID PRIMARY KEY,
    action_id            UUID        NOT NULL REFERENCES actions(id),
    trace_id             UUID        NOT NULL,
    question_set_version TEXT        NOT NULL,
    provider             TEXT        NOT NULL,
    provider_status      TEXT        NOT NULL, -- OK | RATE_LIMITED | TIMEOUT | ...
    model                TEXT,
    latency_ms           BIGINT,
    questions            JSONB,
    answers              JSONB,
    assessment           JSONB,
    overall_confidence   DOUBLE PRECISION,
    input_tokens         BIGINT,
    output_tokens        BIGINT,
    credits_charged      BIGINT,
    credits_remaining    BIGINT,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX jev_eval_action ON jev_evaluations (action_id);

CREATE TABLE groq_evaluations (
    id            UUID PRIMARY KEY,
    action_id     UUID REFERENCES actions(id),
    trace_id      UUID        NOT NULL,
    purpose       TEXT        NOT NULL, -- INTENT_PARSING | ESCALATION_ANALYSIS
    schema_version TEXT       NOT NULL,
    provider_status TEXT      NOT NULL,
    model         TEXT,
    latency_ms    BIGINT,
    result        JSONB,      -- structured result only; no prompts, no secrets
    input_tokens  BIGINT,
    output_tokens BIGINT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX groq_eval_trace ON groq_evaluations (trace_id);

------------------------------------------------------------------------------
-- Append-only enforcement
------------------------------------------------------------------------------
CREATE FUNCTION forbid_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION '% is append-only: % is not allowed', TG_TABLE_NAME, TG_OP
        USING ERRCODE = 'restrict_violation';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER ledger_transactions_append_only BEFORE UPDATE OR DELETE ON ledger_transactions
    FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER ledger_entries_append_only BEFORE UPDATE OR DELETE ON ledger_entries
    FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER audit_events_append_only BEFORE UPDATE OR DELETE ON audit_events
    FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER jev_evaluations_append_only BEFORE UPDATE OR DELETE ON jev_evaluations
    FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER groq_evaluations_append_only BEFORE UPDATE OR DELETE ON groq_evaluations
    FOR EACH ROW EXECUTE FUNCTION forbid_mutation();

-- Every ledger transaction must balance to zero, checked at COMMIT.
CREATE FUNCTION check_ledger_balanced() RETURNS trigger AS $$
DECLARE
    total BIGINT;
BEGIN
    SELECT COALESCE(SUM(amount_minor), 0) INTO total
      FROM ledger_entries WHERE ledger_transaction_id = NEW.ledger_transaction_id;
    IF total <> 0 THEN
        RAISE EXCEPTION 'ledger transaction % does not balance (sum=%)', NEW.ledger_transaction_id, total
            USING ERRCODE = 'check_violation';
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE CONSTRAINT TRIGGER ledger_entries_balanced
    AFTER INSERT ON ledger_entries
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION check_ledger_balanced();
