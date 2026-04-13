# Pokrov Runtime Configuration Reference

Complete reference for all YAML configuration fields in `pokrov.example.yaml`.

## Table of Contents

- [server](#server)
- [logging](#logging)
- [shutdown](#shutdown)
- [security](#security)
- [auth](#auth)
- [identity](#identity)
- [rate_limit](#rate_limit)
- [sanitization](#sanitization)
- [ner](#ner)
  - [Multi-model execution](#multi-model-execution)
  - [Merge strategies](#merge-strategies)
  - [Timeout scaling](#timeout-scaling)
  - [Model fields](#model-fields)
  - [NER profile fields](#ner-profile-fields)
- [llm](#llm)
- [mcp](#mcp)
- [response_envelope](#response_envelope)
- [Evaluate API Request Fields](#evaluate-api-request-fields)
- [Policy Actions](#policy-actions)
- [Evaluation Modes](#evaluation-modes)
- [Detection Categories](#detection-categories)

---

## server

```yaml
server:
  host: 0.0.0.0
  port: 8080
  tls:
    enabled: false
    cert_file: null
    key_file: null
    client_ca_file: null
    require_client_cert: false
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `host` | `string` | _required_ | Bind address. Use `0.0.0.0` for all interfaces. |
| `port` | `u16` | _required_ | Listen port. |
| `tls.enabled` | `bool` | `false` | Enable TLS (HTTPS). Requires `cert_file` and `key_file`. |
| `tls.cert_file` | `string?` | `null` | Path to TLS certificate (PEM). Required when `tls.enabled: true`. |
| `tls.key_file` | `string?` | `null` | Path to TLS private key (PEM). Required when `tls.enabled: true`. |
| `tls.client_ca_file` | `string?` | `null` | Path to client CA bundle for mTLS. Required when `tls.require_client_cert: true`. |
| `tls.require_client_cert` | `bool` | `false` | Require client certificate for mTLS authentication. |

---

## logging

```yaml
logging:
  level: info
  format: json
  component: runtime
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `level` | `enum` | `info` | Log level: `trace`, `debug`, `info`, `warn`, `error`. |
| `format` | `enum` | `json` | Log format. Only `json` is supported in v1. |
| `component` | `string` | `runtime` | Component name included in all log entries. |

---

## shutdown

```yaml
shutdown:
  drain_timeout_ms: 5000
  grace_period_ms: 10000
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `drain_timeout_ms` | `u64` | _required_ | Max time (ms) to wait for in-flight requests to complete during graceful shutdown. |
| `grace_period_ms` | `u64` | _required_ | Total graceful shutdown window (ms). Must be >= `drain_timeout_ms`. After this, connections are force-closed. |

---

## security

```yaml
security:
  fail_on_unresolved_api_keys: false
  fail_on_unresolved_provider_keys: false
  api_keys:
    - key: env:POKROV_API_KEY
      profile: strict
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `fail_on_unresolved_api_keys` | `bool` | `false` | If `true`, runtime fails to start when any API key reference cannot be resolved. |
| `fail_on_unresolved_provider_keys` | `bool` | `false` | If `true`, runtime fails to start when any provider API key reference cannot be resolved. |
| `api_keys` | `array` | `[]` | Gateway API key bindings. Each key maps to a sanitization profile. |
| `api_keys[].key` | `string` | _required_ | Secret reference: `env:VAR_NAME` (from environment) or `file:/path` (from file). |
| `api_keys[].profile` | `string` | _required_ | Sanitization profile to bind this key to. Must be one of `minimal`, `strict`, `custom`. |

---

## auth

```yaml
auth:
  upstream_auth_mode: static
  allow_single_bearer_passthrough: false
  gateway_auth_mode: api_key
  internal_mtls:
    identity_header: x-pokrov-client-cert-subject
    require_header: true
  mesh:
    identity_header: x-forwarded-client-cert
    required_spiffe_trust_domain: null
    require_header: true
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `upstream_auth_mode` | `enum` | `static` | How upstream provider credentials are handled. `static` = Pokrov uses config credentials. `passthrough` = client-supplied bearer token forwarded to provider. |
| `allow_single_bearer_passthrough` | `bool` | `false` | Allow a single `Authorization: Bearer ...` header to serve as both gateway auth and upstream provider credential. Only valid in `passthrough` mode. |
| `gateway_auth_mode` | `enum` | `api_key` | Gateway authentication mode. `api_key` = validate against `security.api_keys`. `internal_mtls` = validate via client TLS certificates. `mesh_mtls` = validate via mesh identity header. |
| `internal_mtls.identity_header` | `string` | `x-pokrov-client-cert-subject` | Header name for passing client certificate subject in internal mTLS mode. |
| `internal_mtls.require_header` | `bool` | `true` | Whether the identity header must be present in internal mTLS mode. |
| `mesh.identity_header` | `string` | `x-forwarded-client-cert` | Header name for mesh identity (e.g., Istio `x-forwarded-client-cert`). |
| `mesh.required_spiffe_trust_domain` | `string?` | `null` | Optional SPIFFE trust domain to validate mesh identity. |
| `mesh.require_header` | `bool` | `true` | Whether the mesh identity header must be present. |

### Upstream TLS trust via environment

For outbound TLS to upstream LLM providers, Pokrov supports `SSL_CERT_FILE`.
Use this when upstream certificates are issued by a private/corporate CA that
is not present in default trust roots.

```bash
export SSL_CERT_FILE=/etc/pokrov/certs/corp-root-ca.pem
```

- Value must point to a PEM file.
- File may contain multiple certificates (`BEGIN CERTIFICATE` blocks).
- Typical failure without trusted CA: `invalid peer certificate: UnknownIssuer`.

---

## identity

```yaml
identity:
  resolution_order:
    - gateway_auth_subject
    - x_pokrov_client_id
    - ingress_identity
  profile_bindings: {}
  rate_limit_bindings: {}
  required_for_policy: false
  required_for_rate_limit: false
  fallback_policy_profile: strict
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `resolution_order` | `array` | `[gateway_auth_subject, x_pokrov_client_id, ingress_identity]` | Order of identity sources for profile/rate-limit resolution. |
| `profile_bindings` | `map` | `{}` | Map identity values to sanitization profiles. Key = identity value, value = profile name. |
| `rate_limit_bindings` | `map` | `{}` | Map identity values to rate-limit profiles. Key = identity value, value = profile name. |
| `required_for_policy` | `bool` | `false` | Whether a resolved identity is required for policy evaluation. |
| `required_for_rate_limit` | `bool` | `false` | Whether a resolved identity is required for rate limiting. |
| `fallback_policy_profile` | `string?` | `null` | Default profile used when no identity-based binding matches. |

Identity sources in `resolution_order`:

| Source | Description |
|--------|-------------|
| `gateway_auth_subject` | Subject from gateway API key authentication. |
| `x_pokrov_client_id` | Value from `X-Pokrov-Client-Id` header. |
| `ingress_identity` | Value from ingress identity header (mesh/proxy). |

---

## rate_limit

```yaml
rate_limit:
  enabled: true
  default_profile: strict
  profiles:
    strict:
      requests_per_minute: 120
      token_units_per_minute: 24000
      burst_multiplier: 1.5
      enforcement_mode: enforce
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | `bool` | `false` | Enable rate limiting. |
| `default_profile` | `string` | `strict` | Default rate-limit profile for unbound clients. |
| `profiles` | `map` | `{}` | Named rate-limit profiles. |
| `profiles.<name>.requests_per_minute` | `u32` | _required_ | Maximum requests per minute. |
| `profiles.<name>.token_units_per_minute` | `u32` | _required_ | Maximum token units per minute (estimated from payload size). |
| `profiles.<name>.burst_multiplier` | `f32` | `1.0` | Burst allowance multiplier. `1.5` allows 50% over the limit in short bursts. |
| `profiles.<name>.enforcement_mode` | `enum` | `enforce` | `enforce` = reject excess requests. `dry_run` = log only, do not reject. |

---

## sanitization

```yaml
sanitization:
  enabled: true
  default_profile: strict
  profiles:
    minimal:
      mode_default: enforce
      categories:
        secrets: mask
        pii: allow
        corporate_markers: allow
        custom: null
      mask_visible_suffix: 4
      max_hits_per_request: 4096
      ner_enabled: false
      custom_rules: []
      deterministic_recognizers: []
      allow_empty_matches: false
    strict:
      mode_default: enforce
      categories:
        secrets: block
        pii: redact
        corporate_markers: mask
        custom: null
      mask_visible_suffix: 4
      max_hits_per_request: 4096
      ner_enabled: true
      custom_rules: []
      deterministic_recognizers: []
      allow_empty_matches: false
    custom:
      mode_default: dry_run
      categories:
        secrets: redact
        pii: mask
        corporate_markers: mask
        custom: null
      mask_visible_suffix: 4
      max_hits_per_request: 4096
      ner_enabled: false
      custom_rules: []
      deterministic_recognizers: []
      allow_empty_matches: false
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | `bool` | `true` | Enable sanitization engine. |
| `default_profile` | `string` | `strict` | Default profile used when request does not specify one. |
| `profiles` | _object_ | see below | Three fixed profiles: `minimal`, `strict`, `custom`. |

### Profile fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `mode_default` | `enum` | `enforce` | Default evaluation mode: `enforce` or `dry_run`. |
| `categories.secrets` | `enum` | profile-dependent | Action for `secrets` detection category. |
| `categories.pii` | `enum` | profile-dependent | Action for `pii` detection category. |
| `categories.corporate_markers` | `enum` | profile-dependent | Action for `corporate_markers` detection category. |
| `categories.custom` | `enum?` | `null` | Action for `custom` detection category. Defaults to `corporate_markers` action if not set. |
| `mask_visible_suffix` | `u8` | `4` | Number of trailing characters visible when action is `mask`. Max `8`. |
| `max_hits_per_request` | `u32` | `4096` | Maximum detection hits per request. |
| `ner_enabled` | `bool` | profile-dependent | Enable NER-based entity detection for this profile. Requires `ner.enabled: true` at top level. |
| `allow_empty_matches` | `bool` | `false` | Allow custom rules that can match empty strings. |
| `custom_rules` | `array` | `[]` | User-defined regex rules. |
| `deterministic_recognizers` | `array` | `[]` | Deterministic recognizers with validators and context. |

### Built-in profile defaults

| Profile | secrets | pii | corporate_markers | ner_enabled |
|---------|---------|-----|-------------------|-------------|
| `minimal` | `mask` | `allow` | `allow` | `false` |
| `strict` | `block` | `redact` | `mask` | `true` |
| `custom` | `redact` | `mask` | `mask` | `false` |

### custom_rules

```yaml
custom_rules:
  - id: custom.project_andromeda
    category: corporate_markers
    pattern: "(?i)project\\s+andromeda"
    action: redact
    priority: 900
    replacement: null
    enabled: true
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `id` | `string` | _required_ | Unique rule identifier. Prefixed with `custom.` in internal rule IDs. |
| `category` | `enum` | _required_ | Detection category: `secrets`, `pii`, `corporate_markers`, `custom`. |
| `pattern` | `string` | _required_ | Regex pattern. Applied to string leaves in JSON payload. |
| `action` | `enum` | _required_ | Policy action: `allow`, `mask`, `replace`, `redact`, `block`. |
| `priority` | `u16` | `100` | Rule priority. Higher wins when rules overlap. |
| `replacement` | `string?` | `null` | Replacement template for `replace` action. Required when `action: replace`. Supports `{match}` placeholder. |
| `enabled` | `bool` | `true` | Whether the rule is active. |

### deterministic_recognizers

```yaml
deterministic_recognizers:
  - id: payment_card
    category: secrets
    action: block
    family_priority: 600
    enabled: true
    patterns:
      - id: pan
        expression: "\\b\\d(?:[ -]?\\d){12,15}\\b"
        base_score: 200
        normalization: alnum_lowercase
        validator:
          kind: luhn
    allowlist_exact: ["4111 1111 1111 1111"]
    denylist_exact: ["9999 0000 0000 0000"]
    context:
      positive_terms: ["card", "payment", "cvv"]
      negative_terms: ["example", "demo"]
      score_boost: 10
      score_penalty: 10
      window: 32
      suppress_on_negative: false
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `id` | `string` | _required_ | Unique recognizer ID. Must be unique across all recognizers in the profile. |
| `category` | `enum` | _required_ | Detection category for all rules in this recognizer. |
| `action` | `enum` | _required_ | Default action for all rules. |
| `family_priority` | `u16` | `100` | Base priority for the entire recognizer family. Added to pattern `base_score`. |
| `enabled` | `bool` | `true` | Whether the recognizer is active. |
| `patterns` | `array` | `[]` | Regex patterns with optional validators. |
| `denylist_exact` | `array` | `[]` | Exact-match deny list. Matches are anchored to full value (automatic `\A...\z`). Priority = `family_priority + 1000`. |
| `allowlist_exact` | `array` | `[]` | Exact-match allow list. Deny-listed values in the allow list are suppressed. |
| `context` | `object?` | `null` | Context-aware scoring. Adjusts priority based on nearby terms. |

#### Pattern fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `id` | `string` | _required_ | Unique pattern ID within the recognizer. |
| `expression` | `string` | _required_ | Regex pattern. Must be a valid Rust regex. |
| `base_score` | `u16` | `100` | Additional score added to `family_priority`. Final priority = `family_priority + base_score`. |
| `normalization` | `enum` | `preserve` | Normalization before validation: `preserve`, `lowercase`, `alnum_lowercase`. |
| `validator` | `object?` | `null` | Optional validator applied after normalization. |

#### Validator kinds

| Kind | Description |
|------|-------------|
| `luhn` | Luhn algorithm for payment card number validation. |

#### Context fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `positive_terms` | `array` | `[]` | Terms that increase priority when found within `window` characters. |
| `negative_terms` | `array` | `[]` | Terms that decrease priority when found within `window` characters. |
| `score_boost` | `i16` | `10` | Priority increase per positive term found. |
| `score_penalty` | `i16` | `10` | Priority decrease per negative term found. |
| `window` | `u8` | `32` | Character window size for context scanning around the match. |
| `suppress_on_negative` | `bool` | `false` | If `true`, completely suppress the match when any negative term is found in the window. |

---

## ner

```yaml
ner:
  enabled: true
  default_language: ""
  execution: auto              # auto | sequential | parallel
  merge_strategy: union        # union | highest_score
  skip_llm_tools_and_system: true
  skip_fields: []
  strip_values: []
  exclude_entity_patterns: []
  models:
    - language: en
      model_path: "./models/bert-base-NER/model.onnx"
      tokenizer_path: "./models/bert-base-NER/tokenizer.json"
      priority: 100
    - language: ru
      model_path: "./models/ner-rubert-tiny-news/model.onnx"
      tokenizer_path: "./models/ner-rubert-tiny-news/tokenizer.json"
      priority: 100
  fallback_language: en
  timeout_ms: 80
  confidence_threshold: 0.7
  max_seq_length: 512
  profiles:
    strict:
      fail_mode: fail_closed
      entity_types: [person, organization]
    minimal:
      fail_mode: fail_open
      entity_types: [person]
```

Requires the `ner` feature flag: `cargo run -p pokrov-runtime --features ner`.

### Multi-model execution

The `execution` field controls how loaded NER models are invoked when processing a batch of texts.

| Mode | Behavior | Latency | CPU |
|------|----------|---------|-----|
| `auto` | Select exactly one model per text by detected language (or `default_language`). Fallback to `fallback_language` on mismatch. | Single model time | 1x |
| `sequential` | Run **every** loaded model on each text, one after another. Collects all hits, then merges. | Sum of all model times | 1x |
| `parallel` | Run **every** loaded model on each text concurrently via `std::thread::scope`. Each thread locks a different model's ONNX session (zero contention). | Max single model time | Nx (N = number of models) |

Default is `auto` — fully backward compatible with previous behavior.

### Merge strategies

When `execution` is `sequential` or `parallel`, multiple models may produce overlapping entity spans for the same text. The `merge_strategy` field controls how these overlaps are resolved.

| Strategy | Behavior |
|----------|----------|
| `union` | Collect all non-overlapping spans. When two models detect the exact same byte range, keep the one with the higher confidence score. Spans with different ranges are all kept. |
| `highest_score` | Greedy non-overlapping selection: sort all hits by confidence (descending), then iterate and keep a hit only if it does not overlap any already-selected hit. Overlap is defined as `(a.start < b.end) && (b.start < a.end)`. |

Default is `union`.

### Timeout scaling

The adapter automatically adjusts the effective timeout based on the execution mode:

- `auto` / `parallel`: `timeout_ms × ceil(texts / 32)` — scales only by batch chunk count.
- `sequential`: `timeout_ms × ceil(texts / 32) × num_models` — additionally scales by model count since models run one after another.

### Field reference

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | `bool` | `true` | Global NER on/off switch. |
| `default_language` | `string` | `""` | When non-empty, all texts are processed with this language model and auto-detection is skipped entirely. |
| `execution` | `enum` | `auto` | Multi-model execution mode: `auto`, `sequential`, `parallel`. See [Multi-model execution](#multi-model-execution). |
| `merge_strategy` | `enum` | `union` | Strategy for merging overlapping hits from multiple models: `union`, `highest_score`. Only applies when `execution` is not `auto`. See [Merge strategies](#merge-strategies). |
| `models` | `array` | EN + RU defaults | ONNX model bindings for NER inference. |
| `fallback_language` | `string` | `en` | Language used when auto-detection fails (auto mode) or when no model matches. |
| `timeout_ms` | `u64` | `80` | Timeout (ms) per batch chunk for NER inference. See [Timeout scaling](#timeout-scaling). |
| `confidence_threshold` | `f32` | `0.7` | Minimum confidence score for entity detection (0.0-1.0). |
| `max_seq_length` | `usize` | `512` | Maximum token sequence length. Longer texts are truncated. |
| `skip_llm_tools_and_system` | `bool` | `true` | Skip NER over LLM `tools` and `system` message content to avoid large-schema/system-prompt inference timeouts. |
| `skip_fields` | `array` | `[]` | List of regex patterns matched against each JSON pointer segment; matching paths are skipped by NER. Example: `["^__"]` skips `__typename`, `__id`, etc. |
| `strip_values` | `array` | `[]` | List of regex patterns matched against text content; matched substrings are replaced with spaces before NER inference so the rest of the text is still processed. Example: `['"__typename"\\s*:\\s*"[^"]*"']` strips GraphQL type discriminators. |
| `exclude_entity_patterns` | `array` | `[]` | List of regex patterns; NER hits whose recognized text matches are discarded. Example: `["^_E_"]` skips GraphQL entity type markers. |
| `profiles` | `map` | `{}` | Per-profile NER configuration. Key = profile name. |

### Model fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `language` | `string` | _required_ | Language tag (e.g., `en`, `ru`). Used for auto-detection based on `effective_language` in requests. |
| `model_path` | `string` | _required_ | Path to ONNX model file. |
| `tokenizer_path` | `string` | _required_ | Path to HuggingFace tokenizer JSON file. |
| `priority` | `u16` | `100` | Model priority when multiple models match the same language. Higher wins. |

### NER profile fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `fail_mode` | `enum` | `fail_open` | `fail_open` = skip NER hits on inference error, continue processing. `fail_closed` = block the entire request on inference error. |
| `entity_types` | `array` | `[person, organization]` | Entity types to detect: `person`, `organization`. Controls which entities are returned by NER for this profile. |

**Note:** NER is only active for profiles that have `sanitization.profiles.<name>.ner_enabled: true`. The `ner.profiles.<name>.entity_types` controls which entity types are detected, while `ner_enabled` controls whether NER runs at all for that profile.

---

## llm

```yaml
llm:
  providers:
    - id: openai
      base_url: https://api.openai.com/v1
      profile_id: strict
      upstream_path: /chat/completions
      auth:
        api_key: env:OPENAI_API_KEY
      timeout_ms: 30000
      retry_budget: 1
      enabled: true
  routes:
    - model: gpt-4o-mini
      provider_id: openai
      aliases: [openai/gpt-4o-mini]
      output_sanitization: true
      enabled: true
  defaults:
    profile_id: strict
    output_sanitization: true
    stream_sanitization_max_buffer_bytes: 1048576
```

### Provider fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `id` | `string` | _required_ | Unique provider identifier. Referenced by routes. |
| `base_url` | `string` | _required_ | Provider API base URL. |
| `profile_id` | `string?` | `null` | Optional provider-level fallback profile for LLM sanitization. Must be one of `minimal`, `strict`, `custom`. |
| `upstream_path` | `string?` | `null` | Override upstream endpoint path. Must start with `/`. Normalized to remove trailing slashes and double slashes. |
| `auth.api_key` | `string` | `""` | Optional provider API key reference: `env:VAR_NAME` or `file:/path`. Leave empty for local/no-auth upstreams; Pokrov skips the upstream `Authorization` header when no key is configured. |
| `timeout_ms` | `u64` | `30000` | Upstream request timeout (ms). |
| `retry_budget` | `u8` | `0` | Number of retry attempts on upstream failure (5xx/network). `0` = no retries. |
| `enabled` | `bool` | _required_ | Whether the provider is active. Disabled providers are skipped during route resolution. |

### Route fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `model` | `string` | _required_ | Canonical model ID exposed to clients. |
| `provider_id` | `string` | _required_ | Provider to route requests to. Must reference a valid `providers[].id`. |
| `aliases` | `array` | `[]` | Alternative model IDs that map to this route. Normalized to lowercase for comparison. Max 128 chars per alias. |
| `output_sanitization` | `bool?` | `null` | Override default output sanitization for this route. `null` = use `llm.defaults.output_sanitization`. |
| `enabled` | `bool` | `true` | Whether the route is active. Only one enabled route per canonical model ID. |

### LLM defaults

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `profile_id` | `string` | _required_ | Default sanitization profile for LLM input/output. |
| `output_sanitization` | `bool` | `true` | Whether to sanitize LLM responses. |
| `stream_sanitization_max_buffer_bytes` | `usize` | `1048576` | Max buffer size (bytes) for SSE stream sanitization. Range: `1024..=16777216` (1KB-16MB). |

### LLM profile resolution precedence

Effective profile is selected in this order:

1. Request `metadata.profile` (when valid).
2. `llm.providers[].profile_id` for the resolved provider.
3. Gateway/API key profile binding.
4. `llm.defaults.profile_id`.

---

## mcp

```yaml
mcp:
  defaults:
    profile_id: strict
    upstream_timeout_ms: 10000
    output_sanitization: true
  servers:
    - id: repo-tools
      endpoint: http://repo-tools.internal
      enabled: true
      allowed_tools: [read_file, grep]
      blocked_tools: [write_file]
      tools:
        read_file:
          enabled: true
          output_sanitization: true
          argument_constraints:
            required_keys: [path]
            forbidden_keys: [command]
            allowed_path_prefixes: [src/, docs/]
            max_string_length: 512
```

### MCP defaults

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `profile_id` | `string` | _required_ | Default sanitization profile for MCP tool calls and outputs. |
| `upstream_timeout_ms` | `u64` | `10000` | Upstream MCP server timeout (ms). |
| `output_sanitization` | `bool` | `true` | Whether to sanitize tool call responses. |

### Server fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `id` | `string` | _required_ | Unique server identifier. Must be unique across all servers. |
| `endpoint` | `string` | _required_ | MCP server URL. |
| `enabled` | `bool` | `true` | Whether the server is active. |
| `allowed_tools` | `array` | `[]` | Tool ID allowlist. Must contain at least one tool when server is enabled. |
| `blocked_tools` | `array` | `[]` | Tool ID blocklist. A tool cannot appear in both `allowed_tools` and `blocked_tools`. |
| `tools` | `map` | `{}` | Per-tool policy overrides. Key = tool ID. |

### Tool policy fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | `bool` | `true` | Whether the tool is accessible. |
| `argument_schema` | `value?` | `null` | Optional JSON Schema for argument validation. |
| `argument_constraints` | `object` | see below | Argument-level constraints. |
| `output_sanitization` | `bool?` | `null` | Override default output sanitization for this tool. |

### Argument constraint fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `max_depth` | `u8?` | `null` | Maximum nesting depth for JSON arguments. |
| `max_string_length` | `usize?` | `null` | Maximum length for string argument values. |
| `required_keys` | `array` | `[]` | Argument keys that must be present. |
| `forbidden_keys` | `array` | `[]` | Argument keys that must not be present. |
| `allowed_path_prefixes` | `array` | `[]` | Allowed path prefixes for `path`-type arguments. |

---

## response_envelope

```yaml
response_envelope:
  pokrov_metadata:
    mode: enabled
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `pokrov_metadata.mode` | `enum` | `enabled` | `enabled` = include `pokrov` metadata block in LLM responses. `suppressed` = omit metadata for strict client compatibility. |

---

## Evaluate API Request Fields

Fields for `POST /v1/sanitize/evaluate`:

```json
{
  "profile_id": "strict",
  "mode": "enforce",
  "path_class": "direct",
  "effective_language": "en",
  "payload": { "key": "value" }
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `profile_id` | `string` | server default | Sanitization profile to use. |
| `mode` | `enum` | profile default | Evaluation mode: `enforce` or `dry_run`. |
| `path_class` | `enum` | `direct` | Request origin class for audit: `direct`, `llm`, `mcp`. |
| `effective_language` | `string?` | `en` | Language hint for NER auto-detection. Used to select the appropriate NER model. |
| `payload` | `value` | _required_ | JSON payload to sanitize. Recursively scanned for sensitive data. |

---

## Policy Actions

Actions applied to detected sensitive content, ordered by strictness:

| Action | Strictness | Description |
|--------|-----------|-------------|
| `allow` | 0 | No action. Match is recorded but content passes through. |
| `mask` | 1 | Partial masking. Keep first `mask_visible_suffix` characters, replace the rest with `*`. |
| `replace` | 2 | Custom replacement. Replace match with `replacement` template. Requires `replacement` field. |
| `redact` | 3 | Full redaction. Replace entire match with `[REDACTED]`. |
| `block` | 4 | Block the entire request. No content is returned. |

When multiple rules overlap, the strictest action wins.

---

## Evaluation Modes

| Mode | Description |
|------|-------------|
| `enforce` | Apply sanitization. Masked/redacted content is returned in `sanitized_payload`. Blocked requests return error. |
| `dry_run` | Run full detection pipeline but do not modify the payload. Results show what would have been sanitized. Useful for testing policies. |

---

## Detection Categories

| Category | Description | Typical Action |
|----------|-------------|----------------|
| `secrets` | API keys, tokens, credentials, payment card numbers | `block` / `redact` |
| `pii` | Personal information: names, emails, phone numbers | `redact` / `mask` |
| `corporate_markers` | Project codenames, internal service names | `mask` / `redact` |
| `custom` | User-defined patterns | configurable |

NER entity mapping:

| NER Entity | Detection Category |
|-----------|-------------------|
| `person` | `pii` |
| `organization` | `corporate_markers` |
