# Data Model: LLM Proxy

## LLMRequestEnvelope

- **Purpose**: Нормализованное представление OpenAI-compatible chat completions
  запроса перед policy/routing.
- **Fields**:
  - `request_id: String`
  - `model: String`
  - `messages: Vec<LLMMessage>`
  - `stream: bool`
  - `profile_hint: Option<String>`
  - `metadata_tags: BTreeMap<String, String>`
- **Validation rules**:
  - `model` не пустой;
  - `messages` содержит хотя бы одно сообщение;
  - каждый `LLMMessage.role` принадлежит allowlist (`system`, `user`, `assistant`, `tool`);
  - metadata значения ограничены по длине и не используются для raw payload dump.

## LLMMessage

- **Purpose**: Единица содержимого LLM запроса после нормализации.
- **Fields**:
  - `role: String`
  - `content: MessageContent`
  - `name: Option<String>`
- **Validation rules**:
  - текстовые сегменты допускают только UTF-8 строки;
  - структура вложенных content blocks сохраняется при sanitization.

## MessageContent

- **Purpose**: Унифицированное представление контента сообщения.
- **Variants**:
  - `Text(String)`
  - `Blocks(Vec<ContentBlock>)`
- **Validation rules**:
  - в `Blocks` каждый элемент имеет известный `type`;
  - санитизация мутирует только строковые поля, не меняя JSON shape.

## ContentBlock

- **Purpose**: Блок структурированного контента в OpenAI-compatible сообщении.
- **Fields**:
  - `block_type: String`
  - `text: Option<String>`
  - `json: Option<serde_json::Value>`
- **Validation rules**:
  - `block_type` обязателен;
  - для текстовых блоков `text` обязателен;
  - бинарные/нестроковые поля не подвергаются трансформации.

## ProviderRoute

- **Purpose**: Правило выбора upstream provider по модели.
- **Fields**:
  - `route_id: String`
  - `model: String`
  - `provider_id: String`
  - `provider_base_url: String`
  - `timeout_ms: u64`
  - `retry_budget: u8`
  - `enabled: bool`
- **Validation rules**:
  - каждая `model` имеет не более одного активного маршрута;
  - `timeout_ms > 0`;
  - `provider_base_url` валиден как HTTPS URL (или allowlisted internal HTTP).

## RouteResolution

- **Purpose**: Результат deterministic выбора provider для текущего запроса.
- **Fields**:
  - `request_id: String`
  - `model: String`
  - `provider_id: String`
  - `matched_via: "exact" | "fallback"`
- **Validation rules**:
  - при отсутствии маршрута возвращается structured error до upstream вызова;
  - resolution не зависит от порядка обхода map в runtime.

## LLMPolicyDecision

- **Purpose**: Итог input/output policy evaluation для LLM flow.
- **Fields**:
  - `profile_id: String`
  - `final_action: "allow" | "mask" | "replace" | "redact" | "block"`
  - `rule_hits_total: u32`
  - `hits_by_category: BTreeMap<String, u32>`
  - `output_sanitization_enabled: bool`
  - `blocked: bool`
- **Validation rules**:
  - `blocked=true` только при `final_action=block`;
  - `rule_hits_total == sum(hits_by_category.values())`;
  - decision не содержит raw fragments.

## SanitizedLLMPayload

- **Purpose**: Payload, безопасный для отправки upstream provider.
- **Fields**:
  - `request_id: String`
  - `model: String`
  - `messages: Vec<LLMMessage>`
  - `stream: bool`
- **Validation rules**:
  - shape совпадает с исходным OpenAI-compatible payload;
  - изменяются только string leaves;
  - при block path объект не формируется.

## UpstreamRequestContext

- **Purpose**: Контекст проксирования после sanitization и routing.
- **Fields**:
  - `request_id: String`
  - `provider_id: String`
  - `endpoint: String`
  - `timeout_ms: u64`
  - `stream: bool`
- **Validation rules**:
  - создается только для неблокируемых запросов;
  - содержит только metadata, без raw secret values.

## StreamingChunkSummary

- **Purpose**: Metadata по обработке одного SSE data event.
- **Fields**:
  - `sequence: u64`
  - `event_kind: "delta" | "done" | "error"`
  - `sanitized: bool`
  - `rule_hits: u32`
- **Validation rules**:
  - `sequence` строго возрастает;
  - summary не содержит chunk text.

## LLMResponseEnvelope

- **Purpose**: Нормализованный ответ клиенту в non-stream режиме.
- **Fields**:
  - `request_id: String`
  - `status_code: u16`
  - `body: serde_json::Value`
  - `pokrov: LLMResponseMetadata`
- **Validation rules**:
  - body соответствует OpenAI-compatible schema;
  - `pokrov` включает только metadata-only поля.

## LLMResponseMetadata

- **Purpose**: Explain summary для клиентского ответа.
- **Fields**:
  - `profile: String`
  - `sanitized_input: bool`
  - `sanitized_output: bool`
  - `action: String`
  - `rule_hits: u32`
- **Validation rules**:
  - `rule_hits >= 0`;
  - поля не содержат фрагменты prompt/response.

## LLMAuditEvent

- **Purpose**: Metadata-only аудит завершенного LLM запроса.
- **Fields**:
  - `request_id: String`
  - `profile_id: String`
  - `provider_id: Option<String>`
  - `model: String`
  - `stream: bool`
  - `final_action: String`
  - `rule_hits_total: u32`
  - `blocked: bool`
  - `upstream_status: Option<u16>`
  - `duration_ms: u64`
- **Validation rules**:
  - при `blocked=true` `provider_id` и `upstream_status` могут отсутствовать;
  - raw payload, raw response и raw detection fragments запрещены;
  - `duration_ms` заполняется для каждого запроса.

## LLMProxyError

- **Purpose**: Структурированный ответ об ошибке для LLM endpoint.
- **Fields**:
  - `request_id: String`
  - `error.code: "invalid_request" | "unauthorized" | "policy_blocked" | "model_not_routed" | "upstream_error" | "upstream_unavailable"`
  - `error.message: String`
  - `provider_id: Option<String>`
  - `retry_after_ms: Option<u64>`
- **Validation rules**:
  - `message` ограничено по длине;
  - ошибка не содержит raw payload values;
  - `retry_after_ms` используется только для retriable conditions.
