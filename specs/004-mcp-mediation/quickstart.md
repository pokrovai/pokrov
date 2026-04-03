# Quickstart: MCP Mediation

## Preconditions

- Rust stable `1.85+`
- Valid Pokrov YAML config with `security.api_keys`, `sanitization.*`, and `mcp.*`
- Approved MCP server reachable from runtime network
- `curl` and `jq` installed

## Example Config Fragment

```yaml
security:
  api_keys:
    - key: env:POKROV_API_KEY
      profile: strict

sanitization:
  enabled: true
  default_profile: strict
  profiles:
    strict:
      mode_default: enforce
      categories:
        secrets: block
        pii: redact
        corporate_markers: mask
      mask_visible_suffix: 4

mcp:
  defaults:
    profile_id: strict
    upstream_timeout_ms: 10000
    output_sanitization: true
  servers:
    - id: repo-tools
      endpoint: http://repo-tools.internal
      enabled: true
      allowed_tools:
        - read_file
        - grep
      blocked_tools:
        - write_file
      tools:
        read_file:
          enabled: true
          output_sanitization: true
          argument_constraints:
            required_keys: [path]
            forbidden_keys: [command]
            allowed_path_prefixes: [src/, docs/]
            max_string_length: 512
        grep:
          enabled: true
          output_sanitization: true
```

## Local Run

```bash
export POKROV_API_KEY='mcp-proxy-test-key'
cargo run -p pokrov-runtime -- --config ./config/pokrov.example.yaml
```

Probe checks:

```bash
curl -sS http://127.0.0.1:8080/health
curl -sS -i http://127.0.0.1:8080/ready
```

## Allowed Tool Call Verification

```bash
curl -sS -X POST http://127.0.0.1:8080/v1/mcp/tool-call \
  -H "Authorization: Bearer $POKROV_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "server": "repo-tools",
    "tool": "read_file",
    "arguments": {"path": "src/lib.rs"},
    "metadata": {"agent_id": "pilot-agent", "profile": "strict"}
  }' | jq
```

Validate:

- HTTP `200`
- `allowed=true`
- `pokrov.profile`, `pokrov.action`, `pokrov.rule_hits` присутствуют
- Tool output возвращается после output sanitization

## Blocked Server/Tool Verification

```bash
curl -sS -X POST http://127.0.0.1:8080/v1/mcp/tool-call \
  -H "Authorization: Bearer $POKROV_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "server": "repo-tools",
    "tool": "write_file",
    "arguments": {"path": "src/lib.rs", "content": "secret"}
  }' | jq
```

Validate:

- HTTP `403`
- `allowed=false`
- `error.code=tool_call_blocked`
- No upstream execution occurs

## Unsupported Variant Verification

```bash
curl -sS -X POST http://127.0.0.1:8080/v1/mcp/tool-call \
  -H "Authorization: Bearer $POKROV_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "server": "repo-tools",
    "tool": "read_file",
    "arguments": {"path": "src/lib.rs"},
    "metadata": {"profile": "strict", "transport": "sse"}
  }' | jq
```

Validate:

- HTTP `422`
- `allowed=false`
- `error.code=unsupported_variant`

## Invalid Arguments Verification

```bash
curl -sS -X POST http://127.0.0.1:8080/v1/mcp/tool-call \
  -H "Authorization: Bearer $POKROV_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "server": "repo-tools",
    "tool": "read_file",
    "arguments": {"command": "cat /etc/passwd"}
  }' | jq
```

Validate:

- HTTP `422` (или `403`, если policy задает block-наследование)
- `error.code=argument_validation_failed` или `tool_call_blocked`
- `error.details` не содержит raw argument values

## Sanitized Output Verification

Сконфигурировать upstream tool, чтобы он возвращал строку с secret-like фрагментом
(например, `token sk-test-123456`), затем повторить allowed вызов.

Validate:

- В ответе нет исходного secret-like фрагмента
- `pokrov.sanitized=true`
- JSON структура `result.content` не повреждена

## Upstream Unavailability Verification

Временно указать недоступный `mcp.servers[].endpoint` и повторить запрос.

Validate:

- HTTP `503`
- `error.code=upstream_unavailable`
- Response содержит только metadata-only поля (`request_id`, safe error summary)

## Automated Verification

```bash
cargo check --workspace
cargo test --workspace
```

## Expected Evidence

- Contract tests подтверждают endpoint/request/response schema для MCP mediation.
- Integration tests покрывают allow path, blocked path, argument-invalid,
  sanitized-output и upstream-unavailable path.
- Security tests подтверждают invalid API key и отсутствие raw arguments/output
  в logs/audit/error details.
- Performance checks подтверждают MCP overhead budget (`p95 <= 50ms`, `p99 <= 100ms`).
