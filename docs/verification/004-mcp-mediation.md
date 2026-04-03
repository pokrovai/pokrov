# Verification Runbook: 004 MCP Mediation

## Scope

This runbook validates the MCP mediation v1 path exposed via `POST /v1/mcp/tool-call`.
It covers:

- allowlisted tool execution
- deterministic block path for disallowed tools
- argument validation rejection before upstream
- output sanitization on string leaves
- predictable upstream failure mapping
- metadata-only response/logging safety
- overhead budget checks

## Local Preparation

```bash
export POKROV_API_KEY='mcp-proxy-test-key'
cargo run -p pokrov-runtime -- --config ./config/pokrov.example.yaml
```

Probe checks:

```bash
curl -sS http://127.0.0.1:8080/health
curl -sS -i http://127.0.0.1:8080/ready
```

Expected readiness checks:

- `checks.config=ok`
- `checks.policy=ok`
- `checks.llm=ok|pending` (depending on llm config)
- `checks.mcp=ok` when MCP section is valid and enabled

## Manual Scenario Verification

### Allowed Tool Path

```bash
curl -sS -X POST http://127.0.0.1:8080/v1/mcp/tool-call \
  -H "Authorization: Bearer $POKROV_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "server": "repo-tools",
    "tool": "read_file",
    "arguments": {"path": "src/lib.rs"},
    "metadata": {"profile": "strict", "transport": "http_json", "variant": "tool_call"}
  }' | jq
```

Expected:

- HTTP `200`
- `allowed=true`
- `pokrov.profile`, `pokrov.action`, `pokrov.rule_hits` present

### Blocked Tool Path

```bash
curl -sS -X POST http://127.0.0.1:8080/v1/mcp/tool-call \
  -H "Authorization: Bearer $POKROV_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "server": "repo-tools",
    "tool": "write_file",
    "arguments": {"path": "src/lib.rs", "content": "secret"},
    "metadata": {"profile": "strict"}
  }' | jq
```

Expected:

- HTTP `403`
- `allowed=false`
- `error.code=tool_call_blocked`
- `error.details.reason=tool_blocklisted`

### Invalid Arguments Path

```bash
curl -sS -X POST http://127.0.0.1:8080/v1/mcp/tool-call \
  -H "Authorization: Bearer $POKROV_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "server": "repo-tools",
    "tool": "read_file",
    "arguments": {"command": "cat /etc/passwd"},
    "metadata": {"profile": "strict"}
  }' | jq
```

Expected:

- HTTP `422`
- `allowed=false`
- `error.code=argument_validation_failed`
- response does not contain raw argument values

### Unsupported Variant Rejection

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

Expected:

- HTTP `422`
- `allowed=false`
- `error.code=unsupported_variant`

## Automated Verification Commands

```bash
cargo check --workspace
cargo test --test contract -- mcp
cargo test --test integration -- mcp_
cargo test --test security -- mcp_
cargo test --test performance -- mcp_
```

## Acceptance Checklist

- allowlisted tool path reaches upstream and returns `200`
- blocked and invalid argument paths short-circuit before upstream
- output sanitization modifies only string leaves and keeps JSON structure valid
- upstream unavailability maps to `503 upstream_unavailable`
- metadata-only error and audit semantics preserved
- performance checks satisfy p95/p99 budget targets
