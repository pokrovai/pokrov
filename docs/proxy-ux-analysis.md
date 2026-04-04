# Proxy UX Analysis: Client Configuration Convenience for Pokrov.AI

**Date:** 2026-04-05  
**Method:** Council analysis (glm-5-turbo + MiniMax-M2.7 consensus)  
**Scope:** Proxy/routing UX only — sanitization, security, MCP excluded  

---

## Current State Assessment

| Criterion                   | Score    | Comment                                                                                     |
|-----------------------------|----------|---------------------------------------------------------------------------------------------|
| **DX (setup convenience)**  | **5/10** | Triple manual config sync (client ↔ Pokrov ↔ upstream)                                      |
| **Proxy transparency**      | **4/10** | OpenAI-compatible works, but no `/v1/models` + `pokrov` metadata injection into responses   |
| **Multi-provider support**  | **3/10** | OpenAI-compatible upstream only. Anthropic/Google **architecturally impossible**            |
| **Model discovery**         | **0/10** | `/v1/models` endpoint completely absent                                                     |
| **OpenRouter-like UX**      | **2/10** | No aliasing, prefix-routing, or fallback                                                    |

---

## Key Architectural Blockers

### 1. Hardcoded upstream path (`upstream.rs:172-174`)

```rust
fn build_endpoint(base_url: &str) -> String {
    format!("{}{}", base_url.trim_end_matches('/'), "/chat/completions")
}
```

All requests go to `/chat/completions` — Anthropic (`/v1/messages`), Google Gemini, Bedrock **break architecturally**.

### 2. No `/v1/models` endpoint

Clients cannot discover available models. Data already exists in `ProviderRouteTable.routes` (`routing.rs:29-31`) — 
a `BTreeMap<String, RouteRecord>`. Implementation is straightforward export.

### 3. No model aliasing

Exact match required between `model` in request and `routes[].model` in config. Cannot map `openai/gpt-4o` → `gpt-4o`.
Это очень важно, так как например GLM-5, может быть у Z.AI или другого провайдера

### 4. "Triple synchronization" problem

For 10 models, user must manually maintain:
1. 10 route entries in Pokrov YAML
2. 10 model entries in client config (OpenCode, Codex, etc.)
3. Real model names at upstream providers

---

## Detailed Findings

### OpenCode Compatibility

OpenCode configures multiple providers pointing to a single proxy:

```yaml
providers:
  openai:
    base_url: http://localhost:8080/v1  # → Pokrov
    api_key: pokrov-gateway-key
    models: [gpt-4o, gpt-4o-mini, o3]
  anthropic:
    base_url: http://localhost:8080/v1  # → same Pokrov
    api_key: pokrov-gateway-key
    models: [claude-sonnet-4-20250514]
```

**What works:** Pokrov handles multiple models on a single endpoint — `ProviderRouteTable.resolve(model)` does 
BTreeMap lookup. Architecture is correct at runtime.

**What breaks:**
- No auto-discovery → manual model list sync required
- No provider prefix routing → `anthropic/claude-sonnet` doesn't work
- Anthropic upstream **impossible** — all requests go to `/chat/completions`

### Codex Agent Compatibility

Codex uses `/v1/responses` — Pokrov supports this endpoint. Limitations:
- Responses payload is **converted to chat-completions** upstream — loses Responses-specific features (tools, function_calling in Responses format)
- Only text input/output mapping is supported

### Drop-in Transparency Issues

| Issue                          | Location                                                                       | Impact                                                                   |
|--------------------------------|--------------------------------------------------------------------------------|--------------------------------------------------------------------------|
| No `/v1/models`                | `app.rs:302-319` — route missing                                               | Clients can't discover models; OpenAI SDK may fail on connectivity check |
| `pokrov` metadata in responses | `handler/mod.rs:316-326` — injects `request_id` + `pokrov` into every response | Violates response "purity"; strict parsers may reject unexpected fields  |
| Hardcoded upstream path        | `upstream.rs:172-174`                                                          | All non-OpenAI providers broken                                          |

### Comparison with Analogues

**OpenRouter provides:**
- Unified OpenAI-compatible endpoint for all providers
- `/v1/models` listing all available models
- Automatic routing by model name (`openai/gpt-4o`, `anthropic/claude-3.5-sonnet`)
- Provider-specific format transformation
- Fallback routing
- Single API key for everything

**CLIProxyAPI:**
- Lightweight proxy for OpenAI-compatible API
- Multiple providers through single endpoint
- Full OpenAI protocol passthrough

---

## Improvement Roadmap

### P0 — MVP Proxy (~10–16 hours total)

| #  | Improvement                                                                     | Files                              | Time  | DX Impact                                       |
|----|---------------------------------------------------------------------------------|------------------------------------|-------|-------------------------------------------------|
| 1  | **`GET /v1/models`** endpoint                                                   | New `handlers/models.rs`, `app.rs` | 2–3 h | Critical — discovery, data already in memory    |
| 2  | **Provider-specific upstream paths** (`upstream_path` field in provider config) | `model.rs`, `upstream.rs`          | 3–5 h | Critical — architectural blocker for non-OpenAI |
| 3  | **Model aliasing** (`aliases` field in route config)                            | `model.rs`, `routing.rs`           | 4–6 h | High — name flexibility                         |
| 4  | **Optional `pokrov` metadata suppression**                                      | `handler/mod.rs`, config           | 1–2 h | Medium — for strict clients                     |

**Target config after P0:**

```yaml
llm:
  providers:
    - id: openai
      base_url: https://api.openai.com/v1
      upstream_path: /chat/completions    # NEW: configurable per-provider
      auth: { api_key: env:OPENAI_API_KEY }
      timeout_ms: 30000
      enabled: true
    - id: anthropic
      base_url: https://api.anthropic.com/v1
      upstream_path: /messages             # NEW: Anthropic endpoint
      auth: { api_key: env:ANTHROPIC_API_KEY }
      timeout_ms: 30000
      enabled: true
  routes:
    - model: gpt-4o-mini
      provider_id: openai
      aliases: [openai/gpt-4o-mini]       # NEW: alternative names
      output_sanitization: true
      enabled: true
    - model: claude-sonnet-4-20250514
      provider_id: anthropic
      aliases: [anthropic/claude-sonnet-4, claude-sonnet]
      output_sanitization: true
      enabled: true
```

### P1 — Full Multi-provider (~19–28 hours)

| # | Improvement                                              | Time   | 
|---|----------------------------------------------------------|--------|
| 5 | Wildcard/prefix routing (`anthropic/*`)                  | 4–6 h  |
| 6 | Anthropic request/response transformer                   | 8–12 h |
| 7 | Fallback routing (backup provider on failure)            | 3–5 h  |
| 8 | Config simplification ("proxy all models from provider") | 3–5 h  |

### P2 — Production Polish

| #  | Improvement                                                     |
|----|-----------------------------------------------------------------|
| 9  | `/v1/responses` passthrough without chat-completions conversion |
| 10 | Google Gemini transformer                                       |
| 11 | Rate limiting per provider/model                                |

---

## Projected Scores After Improvements

| Criterion              | Current  | After P0  | After P1  | OpenRouter   |
|------------------------|----------|-----------|-----------|--------------|
| DX (setup convenience) | 5/10     | **7/10**  | **8/10**  | 9/10         |
| Proxy transparency     | 4/10     | **7/10**  | **8/10**  | 9/10         |
| Multi-provider support | 3/10     | **5/10**  | **8/10**  | 9/10         |
| Model discovery        | 0/10     | **9/10**  | **9/10**  | 10/10        |
| OpenRouter-like UX     | 2/10     | **5/10**  | **7/10**  | 10/10        |

---

## Conclusion

Pokrov is currently a **security proxy for OpenAI-compatible upstream**, not a universal AI gateway. 
To become a convenient proxy (OpenRouter-style), 4 P0 improvements (~10–16 h) would radically improve DX:

1. `/v1/models` — solves discovery, cheap to implement (data already in memory)
2. Provider-specific upstream paths — removes architectural blocker for non-OpenAI
3. Model aliasing — removes fragility of exact name matching
4. Optional metadata suppression — for strict clients

After P0, Pokrov becomes a practical drop-in proxy for OpenAI-compatible providers. 
P1 (transformers, wildcard routing) adds full multi-provider experience at OpenRouter level.

**Remaining uncertainties:**
- Exact Anthropic `/v1/messages` ↔ OpenAI `/v1/chat/completions` mapping completeness (tool calls, function calling, vision) needs separate research
- OpenCode behavior on 404 from `/v1/models` — fallback to hardcoded list or error?

---

*Council: 2/2 councillors responded (alpha: glm-5-turbo, beta: MiniMax-M2.7) — full consensus on all findings and priorities*
