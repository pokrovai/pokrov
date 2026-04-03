# Data Model: Hardening Release

## RateLimitPolicy

- **Purpose**: Конфигурация ограничений по API key и token-like бюджету для
  LLM/MCP path.
- **Fields**:
  - `profile_id: String`
  - `requests_per_minute: u32`
  - `token_units_per_minute: u32`
  - `burst_multiplier: f32`
  - `enforcement_mode: "enforce" | "dry_run"`
- **Relationships**:
  - привязывается к `ApiKeyBinding` из security config;
  - используется `RateLimitState` для вычисления текущего допуска.
- **Validation rules**:
  - `requests_per_minute > 0`;
  - `token_units_per_minute > 0`;
  - `burst_multiplier >= 1.0` и `<= 5.0`;
  - режим `dry_run` не блокирует запрос, но сохраняет telemetry событий.

## RateLimitState

- **Purpose**: Runtime-состояние budget counters для пары `(api_key_id, window)`.
- **Fields**:
  - `api_key_id: String`
  - `window_kind: "requests" | "token_units"`
  - `window_started_at: Instant`
  - `consumed: u32`
  - `remaining: u32`
- **Relationships**:
  - обновляется `RateLimitEvaluator` на каждом request.
- **Validation rules**:
  - `consumed + remaining == effective_limit`;
  - значения не могут быть отрицательными;
  - stale windows переинициализируются до проверки лимита.

## RateLimitDecision

- **Purpose**: Deterministic результат проверки лимита для текущего запроса.
- **Fields**:
  - `allowed: bool`
  - `reason: "within_budget" | "request_budget_exhausted" | "token_budget_exhausted"`
  - `retry_after_ms: u64`
  - `limit: u32`
  - `remaining: u32`
  - `reset_at: DateTime`
- **Validation rules**:
  - при `allowed=false` возвращается `retry_after_ms > 0`;
  - при `allowed=true` `reason` должен быть `within_budget`;
  - решение не содержит raw payload и token text fragments.

## TokenUnitEstimate

- **Purpose**: Безопасная оценка token-like объема до upstream вызова.
- **Fields**:
  - `request_id: String`
  - `path: "llm" | "mcp"`
  - `estimated_units: u32`
  - `observed_units: Option<u32>`
  - `estimation_version: String`
- **Validation rules**:
  - `estimated_units >= 1` для непустого payload;
  - `observed_units`, если присутствует, используется только для telemetry;
  - расчет опирается на санитизированные данные, не на raw payload.

## MetricSeriesDefinition

- **Purpose**: Контракт обязательных telemetry series для hardening stage.
- **Fields**:
  - `name: String`
  - `kind: "counter" | "histogram" | "gauge"`
  - `labels: Vec<String>`
  - `description: String`
- **Relationships**:
  - агрегируется в `MetricsCatalog`;
  - проверяется `ReleaseEvidence` как обязательное покрытие.
- **Validation rules**:
  - labels только low-cardinality (`route`, `path_class`, `decision`, `provider`, `status`);
  - отсутствие series из mandatory списка считается release blocker.

## LogSafetyPolicy

- **Purpose**: Allowlist полей structured logs/audit для исключения leakage.
- **Fields**:
  - `allowed_fields: Vec<String>`
  - `forbidden_patterns: Vec<String>`
  - `enforcement_mode: "drop_field" | "drop_event"`
- **Validation rules**:
  - `allowed_fields` не включает payload bodies/tool arguments/model outputs;
  - `forbidden_patterns` покрывает secret-like markers;
  - любое нарушение фиксируется metadata-only событием.

## LogEventEnvelope

- **Purpose**: Унифицированный безопасный формат runtime log событий.
- **Fields**:
  - `timestamp: DateTime`
  - `level: "INFO" | "WARN" | "ERROR"`
  - `request_id: String`
  - `component: String`
  - `event_type: String`
  - `metadata: Map<String, Primitive>`
- **Validation rules**:
  - `metadata` содержит только allowlisted ключи;
  - строковые значения проходят leak-safety check;
  - raw payload, secret values и large free-form text запрещены.

## ReleaseEvidence

- **Purpose**: Итоговый пакет доказательств release readiness.
- **Fields**:
  - `release_id: String`
  - `generated_at: DateTime`
  - `git_commit: String`
  - `performance: PerformanceEvidence`
  - `security: SecurityEvidence`
  - `operational: OperationalEvidence`
  - `gate_status: "pass" | "fail"`
- **Relationships**:
  - агрегирует результаты test/probe workflows;
  - входит в `DeploymentPackageManifest`.
- **Validation rules**:
  - `gate_status=pass` только если все обязательные блоки помечены `pass`;
  - evidence не включает raw prompt/tool/model samples.

## PerformanceEvidence

- **Purpose**: Метрики производительности для acceptance критериев SC-001/SC-002.
- **Fields**:
  - `scenario_id: String`
  - `p50_ms: f64`
  - `p95_ms: f64`
  - `p99_ms: f64`
  - `throughput_rps: f64`
  - `startup_seconds: f64`
  - `runs: u8`
- **Validation rules**:
  - `runs >= 3`;
  - `p95_ms <= 50`, `p99_ms <= 100`, `throughput_rps >= 500` для baseline pass.

## SecurityEvidence

- **Purpose**: Результаты abuse/auth/log-safety проверок.
- **Fields**:
  - `invalid_auth_check: "pass" | "fail"`
  - `rate_limit_abuse_check: "pass" | "fail"`
  - `log_leak_check: "pass" | "fail"`
  - `secret_handling_check: "pass" | "fail"`
  - `notes: Vec<String>`
- **Validation rules**:
  - любой `fail` переводит `ReleaseEvidence.gate_status` в `fail`;
  - `notes` не содержат sample sensitive payload.

## OperationalEvidence

- **Purpose**: Доказательство operability во время pilot deployment.
- **Fields**:
  - `metrics_coverage_percent: u8`
  - `readiness_degradation_behavior: "pass" | "fail"`
  - `graceful_shutdown_behavior: "pass" | "fail"`
  - `rate_limit_observability: "pass" | "fail"`
- **Validation rules**:
  - `metrics_coverage_percent` должен быть `100` для pass;
  - readiness/graceful-shutdown checks обязательны.

## DeploymentPackageManifest

- **Purpose**: Описание self-hosted release package для pilot rollout.
- **Fields**:
  - `image: String`
  - `config_templates: Vec<String>`
  - `verification_checklist_path: String`
  - `evidence_path: String`
  - `checksums: Vec<ArtifactChecksum>`
- **Validation rules**:
  - все перечисленные артефакты должны существовать;
  - `evidence_path` указывает на валидный `ReleaseEvidence` документ.

## ArtifactChecksum

- **Purpose**: Контроль целостности release artifacts.
- **Fields**:
  - `path: String`
  - `sha256: String`
- **Validation rules**:
  - `sha256` должен быть 64-символьной hex строкой;
  - путь должен относиться к текущему release bundle.
