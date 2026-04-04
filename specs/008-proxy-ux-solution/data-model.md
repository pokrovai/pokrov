# Data Model: Proxy UX P0-P2 Improvements

## LlmProviderEndpointProfile

- **Purpose**: Конфигурация endpoint и protocol profile провайдера.
- **Fields**:
  - `provider_id: String`
  - `base_url: String`
  - `upstream_path: Option<String>`
  - `effective_upstream_path: String` (computed)
  - `transform_profile: "openai_compatible" | "anthropic" | "gemini"`
  - `enabled: bool`
- **Validation rules**:
  - `provider_id` глобально уникален;
  - `base_url` валиден как абсолютный URL;
  - `effective_upstream_path` начинается с `/`;
  - `transform_profile` обязателен для enabled provider;
  - при `upstream_path` отсутствующем используется стабильный default.

## ModelRouteRecord

- **Purpose**: Канонический маршрут модели на primary провайдер.
- **Fields**:
  - `model: String`
  - `provider_id: String`
  - `aliases: Vec<String>`
  - `wildcard_prefixes: Vec<String>`
  - `fallback_chain_id: Option<String>`
  - `output_sanitization: bool`
  - `enabled: bool`
- **Validation rules**:
  - canonical `model` non-empty;
  - `provider_id` должен ссылаться на enabled provider;
  - aliases/prefixes нормализуются в lower-case;
  - canonical/alias/wildcard keys не должны быть неоднозначны;
  - disabled route исключается из routing/discovery.

## RouteResolutionIndex

- **Purpose**: Детерминированное разрешение входного model key.
- **Fields**:
  - `exact_index: Map<normalized_key, route_id>`
  - `alias_index: Map<normalized_key, route_id>`
  - `wildcard_index: Vec<WildcardRule>` (ordered by specificity)
- **Validation rules**:
  - exact/alias коллизии запрещены;
  - wildcard priority детерминирован: длиннейший префикс, затем lexical tie-breaker;
  - resolution order фиксирован: exact -> alias -> wildcard.

## WildcardRule

- **Purpose**: Префиксное правило сопоставления модели.
- **Fields**:
  - `prefix: String`
  - `target_route_id: String`
  - `priority: u16` (derived)
- **Validation rules**:
  - `prefix` должен быть непустым и нормализованным;
  - wildcard rule не должен перекрывать более специфичное exact/alias правило неоднозначно;
  - недетерминированные tie-сценарии запрещены на startup.

## FallbackChain

- **Purpose**: Упорядоченный список резервных маршрутов для retriable отказов.
- **Fields**:
  - `chain_id: String`
  - `steps: Vec<FallbackStep>`
  - `trigger_policy: FallbackTriggerPolicy`
- **Validation rules**:
  - `steps` не пуст;
  - циклы в ссылках маршрутов запрещены;
  - fallback targets должны ссылаться на enabled routes/providers;
  - trigger policy не может включать policy/auth/validation ошибки.

## FallbackStep

- **Purpose**: Один шаг fallback-перехода.
- **Fields**:
  - `target_route_id: String`
  - `max_attempts: u8`
  - `timeout_override_ms: Option<u64>`
- **Validation rules**:
  - `max_attempts >= 1`;
  - `target_route_id` уникален в пределах цепочки;
  - timeout override не должен нарушать глобальный shutdown budget.

## ProviderTransformProfile

- **Purpose**: Правила map'инга клиентского payload в provider формат и обратно.
- **Fields**:
  - `provider_id: String`
  - `request_mapping_mode: "none" | "anthropic_messages" | "gemini_generate_content"`
  - `response_mapping_mode: "none" | "anthropic_messages" | "gemini_generate_content"`
  - `supported_features: Vec<String>`
- **Validation rules**:
  - mapping modes согласованы с provider type;
  - неподдерживаемые feature combinations приводят к deterministic contract error;
  - transform errors metadata-only.

## ResponsesPassthroughMode

- **Purpose**: Управление обработкой `/v1/responses` без деградации в chat-completions subset.
- **Fields**:
  - `enabled: bool`
  - `provider_passthrough_supported: bool`
  - `fallback_to_subset_mapping: bool`
- **Validation rules**:
  - при `enabled=true` путь предпочитает native passthrough;
  - fallback mapping допускается только если явно разрешен;
  - ошибки несовместимости возвращаются предсказуемо и без sensitive data.

## ProviderModelRateLimitProfile

- **Purpose**: Лимиты нагрузки по provider/model в дополнение к текущим лимитам.
- **Fields**:
  - `provider_id: String`
  - `model_key: String`
  - `request_budget_per_window: u32`
  - `token_budget_per_window: u32`
  - `window_ms: u64`
  - `enforcement_mode: "enforce" | "dry_run"`
- **Validation rules**:
  - `provider_id+model_key` уникальная пара;
  - budgets > 0;
  - enforcement mode не отменяет существующие policy budgets;
  - dry-run mode должен генерировать observability events.

## ModelCatalogEntry

- **Purpose**: Элемент `/v1/models` выдачи.
- **Fields**:
  - `id: String`
  - `canonical_model: String`
  - `provider_id: String`
  - `kind: "canonical" | "alias" | "wildcard"`
  - `available: bool`
- **Validation rules**:
  - каталог включает только активные и реально обслуживаемые элементы;
  - disabled providers/routes не публикуются;
  - записи формируются из deterministic routing graph snapshot.

## RoutingResolutionOutcome

- **Purpose**: Metadata-only результат разрешения модели.
- **Fields**:
  - `request_id: String`
  - `input_model_key: String`
  - `normalized_model_key: String`
  - `resolved_route_id: Option<String>`
  - `resolved_provider_id: Option<String>`
  - `resolved_via: "exact" | "alias" | "wildcard" | "fallback"`
  - `status: "resolved" | "model_not_routed" | "config_conflict" | "fallback_exhausted"`
- **Validation rules**:
  - status и resolved fields согласованы;
  - все ошибки остаются metadata-only;
  - outcome стабилен при повторе одинакового запроса.
