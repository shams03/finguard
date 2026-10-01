# ADR 7: Why low confidence triggers escalation

Status: accepted (MVP)

## Context
Low confidence means the system is unsure what is being asked, not that the other model knows better.

## Decision
Below the per-risk-class threshold we escalate: get a clarification/second semantic read, then **re-run policy on a new proposal**. The second model never auto-approves.

## Consequences
More latency on ambiguous requests; much safer failure mode than 'trust whichever model is more confident'.
