import type {
  AccountResp, ActionListItem, ActionView, ApiErrorBody, CardView, HealthResp,
  RecipientView, TraceView, TransactionView,
} from "./types";

const BASE = import.meta.env.VITE_API_URL ?? "";

export class ApiError extends Error {
  constructor(public body: ApiErrorBody, public status: number) {
    super(body.message);
  }
}

async function request<T>(path: string, init?: RequestInit & { idempotencyKey?: string }): Promise<T> {
  const headers: Record<string, string> = { "content-type": "application/json" };
  if (init?.idempotencyKey) headers["idempotency-key"] = init.idempotencyKey;
  let res: Response;
  try {
    res = await fetch(`${BASE}${path}`, { ...init, headers });
  } catch {
    throw new ApiError({ code: "NETWORK", message: "Cannot reach the FinGuard backend. Is it running on :8080?", trace_id: "-" }, 0);
  }
  const text = await res.text();
  const data = text ? JSON.parse(text) : {};
  if (!res.ok) {
    throw new ApiError(data.error ?? { code: "INTERNAL_ERROR", message: res.statusText, trace_id: "-" }, res.status);
  }
  return data as T;
}

const post = <T,>(path: string, body?: unknown, idempotencyKey?: string) =>
  request<T>(path, { method: "POST", body: body === undefined ? undefined : JSON.stringify(body), idempotencyKey });

export const newKey = () => crypto.randomUUID();

export const api = {
  health: () => request<HealthResp>("/health"),
  account: () => request<AccountResp>("/api/v1/account"),
  cards: () => request<{ cards: CardView[] }>("/api/v1/cards").then((r) => r.cards),
  recipients: () => request<{ recipients: RecipientView[] }>("/api/v1/recipients").then((r) => r.recipients),
  addRecipient: (name: string) => post<{ recipients: RecipientView[] }>("/api/v1/recipients", { name }).then((r) => r.recipients),
  transactions: () => request<{ transactions: TransactionView[] }>("/api/v1/transactions").then((r) => r.transactions),
  actions: () => request<{ actions: ActionListItem[] }>("/api/v1/actions").then((r) => r.actions),
  command: (text: string, key: string) => post<ActionView>("/api/v1/agent/commands", { text }, key),
  action: (id: string) => request<ActionView>(`/api/v1/actions/${id}`),
  confirm: (id: string, confirmationId: string, key: string) =>
    post<ActionView>(`/api/v1/actions/${id}/confirm`, { confirmation_id: confirmationId }, key),
  cancel: (id: string) => post<ActionView>(`/api/v1/actions/${id}/cancel`),
  clarify: (id: string, body: { accept_suggestion?: boolean; amount?: string; text?: string }, key: string) =>
    post<ActionView>(`/api/v1/actions/${id}/clarify`, body, key),
  trace: (id: string) => request<TraceView>(`/api/v1/actions/${id}/trace`),
  reset: () => post<unknown>("/api/v1/demo/reset"),
  scenario: (name: string) => post<unknown>(`/api/v1/demo/scenarios/${name}`),
  failure: (provider: "jev" | "groq", mode: string) => post<unknown>(`/api/v1/demo/failures/${provider}/${mode}`),
};
