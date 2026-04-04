# Data Model: BYOK Passthrough Auth

## UpstreamAuthMode

- **Purpose**: Определяет источник upstream credentials для LLM/MCP вызовов.
- **Values**:
  - `static`
  - `passthrough`
- **Validation rules**:
  - значение обязано быть одним из двух перечисленных;
  - default mode сохраняет backward-compatible `static` behavior.

## ClientIdentity

- **Purpose**: Канонический идентификатор клиента/тенанта для policy/rate-limit isolation.
- **Fields**:
  - `id: String`
  - `source: "gateway_auth" | "header" | "ingress_context"`
  - `profile_hint: Option<String>`
- **Validation rules**:
  - `id` non-empty;
  - источник identity должен быть детерминированно выбран по приоритету;
  - отсутствие identity в обязательном режиме приводит к block path.

## GatewayAuthContext

- **Purpose**: Контекст проверки доступа клиента к Pokrov.
- **Fields**:
  - `authenticated: bool`
  - `auth_subject: Option<String>`
  - `auth_mechanism: "api_key" | "jwt" | "mtls" | "external_gateway"`
  - `failure_reason: Option<String>`
- **Validation rules**:
  - при `authenticated=false` запрос не уходит upstream;
  - `failure_reason` в metadata-only формате и без секретов.

## UpstreamCredentialSource

- **Purpose**: Отражает откуда взяты provider credentials для текущего запроса.
- **Fields**:
  - `mode: UpstreamAuthMode`
  - `provider_id: String`
  - `credential_present: bool`
  - `credential_source: "config" | "request"`
- **Validation rules**:
  - `mode=static` -> `credential_source=config`;
  - `mode=passthrough` -> `credential_source=request`;
  - отсутствие credentials в активном режиме приводит к structured auth error.

## IdentityPolicyBinding

- **Purpose**: Привязка client identity к policy profile.
- **Fields**:
  - `client_identity: String`
  - `policy_profile_id: String`
  - `binding_source: "config" | "default"`
- **Validation rules**:
  - для каждой identity должен существовать детерминированный итоговый профиль;
  - fallback к default profile допустим только если явный binding отсутствует.

## IdentityRateLimitBinding

- **Purpose**: Привязка client identity к rate-limit профилю и окну учета.
- **Fields**:
  - `client_identity: String`
  - `rate_limit_profile_id: String`
  - `window_key: String`
- **Validation rules**:
  - rate-limit counters изолированы по `window_key`, включающему identity;
  - события превышения лимита одного клиента не влияют на другой `window_key`.

## AuthDecision

- **Purpose**: Итог решения для auth-пути до policy/upstream вызова.
- **Fields**:
  - `allowed: bool`
  - `stage: "gateway_auth" | "upstream_credential_resolution"`
  - `code: "authorized" | "gateway_unauthorized" | "upstream_credential_missing" | "upstream_credential_invalid"`
  - `request_id: String`
- **Validation rules**:
  - `allowed=false` всегда возвращает metadata-only error envelope;
  - `code=authorized` допустим только при `allowed=true`.

## AuthAuditSummary

- **Purpose**: Metadata-only аудит auth-path результата.
- **Fields**:
  - `request_id: String`
  - `client_identity: Option<String>`
  - `auth_mode: UpstreamAuthMode`
  - `gateway_auth_result: "pass" | "fail"`
  - `upstream_credential_result: "pass" | "fail"`
  - `final_decision: "allowed" | "blocked"`
- **Validation rules**:
  - поле не содержит raw headers/tokens/keys;
  - используется фиксированный набор enum-значений для deterministic analytics.

## AuthMetricEvent

- **Purpose**: Набор low-cardinality метрик для auth outcomes.
- **Fields**:
  - `route: String`
  - `path_class: "llm" | "mcp" | "runtime"`
  - `auth_mode: UpstreamAuthMode`
  - `decision: "allowed" | "blocked"`
  - `reason: "gateway_auth" | "upstream_credentials" | "policy" | "rate_limit"`
- **Validation rules**:
  - labels ограничены allowlist набором;
  - identity и credentials никогда не используются как metric labels.
