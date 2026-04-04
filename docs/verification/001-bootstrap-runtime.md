# Bootstrap Runtime Verification (001)

## Scope

- Runtime startup from valid YAML config.
- Probe behavior contract for `/health` and `/ready`.
- Request correlation via `x-request-id` and `request_id`.
- Graceful shutdown transition to not-ready before process exit.

## Baseline Environment

- Single-node Linux/amd64 or Darwin/arm64.
- At least 2 vCPU and 4 GiB RAM.
- Local loopback networking without external load balancer.

## Runtime Start

```bash
export POKROV_API_KEY='dev-bootstrap-key'
cargo run -p pokrov-runtime -- --config ./config/pokrov.example.yaml
```

## Probe Contract Checks

```bash
curl -sS http://127.0.0.1:8080/health
curl -sS -i http://127.0.0.1:8080/ready
```

Pass criteria:

- `/health` returns `200`.
- `/ready` returns `200` after initialization.
- Both responses expose `x-request-id` header and body-level `request_id`.

## Startup-Pending / Draining Checks

Pass criteria:

- During startup-pending state, `/ready` returns `503`.
- After shutdown signal and during draining, `/ready` returns `503`.
- Existing in-flight requests are allowed to finish within `drain_timeout_ms`.

## Invalid Config Check

```bash
cargo run -p pokrov-runtime -- --config ./config/pokrov.invalid.yaml
```

Pass criteria:

- Startup fails with config validation error.
- Runtime never reaches ready state for invalid config.

## Container Smoke Check

```bash
docker build -t pokrov-bootstrap:latest .
docker run --rm \
  -p 8080:8080 \
  -e POKROV_API_KEY='dev-bootstrap-key' \
  -v "$(pwd)/config/pokrov.example.yaml:/app/config/pokrov.yaml:ro" \
  pokrov-bootstrap:latest \
  --config /app/config/pokrov.yaml
```

Pass criteria:

- `/health=200` and `/ready=200` before stop.
- After `docker stop`, runtime transitions through not-ready before exit.

## Required Test Commands

```bash
cargo test --test integration bootstrap_acceptance_contract
cargo test --test integration readiness_shutdown_flow
cargo test --test integration startup_config_flow
cargo test --test integration request_id_logging_flow
cargo test --test performance probes_respond_within_bootstrap_smoke_budget
```

