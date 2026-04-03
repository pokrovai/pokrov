# Quickstart: Sanitization Core

## Preconditions

- Rust stable `1.85+`
- Валидный YAML config с policy profiles `minimal`, `strict`, `custom`
- Запущенный runtime с evaluate endpoint
- Тестовый API key из `env:`/`file:` reference
- `jq` для проверки metadata-only response полей

## Example Config Fragment

```yaml
sanitization:
  enabled: true
  default_profile: strict
  profiles:
    minimal:
      categories:
        secrets: mask
        pii: allow
        corporate_markers: allow
    strict:
      categories:
        secrets: block
        pii: redact
        corporate_markers: mask
    custom:
      categories:
        secrets: redact
        pii: mask
        corporate_markers: mask
      custom_rules:
        - id: custom_project_codename
          category: corporate_markers
          pattern: "(?i)project\s+andromeda"
          action: redact
```

## Local Run

1. Запустить сервис:

```bash
export POKROV_API_KEY='sanitization-test-key'
cargo run -p pokrov-runtime -- --config ./config/pokrov.example.yaml
```

2. Проверить probes:

```bash
curl -sS http://127.0.0.1:8080/health
curl -sS -i http://127.0.0.1:8080/ready
```

## Evaluate Flow Verification

1. Выполнить enforce запрос с profile `strict`:

```bash
curl -sS -X POST http://127.0.0.1:8080/v1/sanitize/evaluate \
  -H "Authorization: Bearer $POKROV_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "profile_id": "strict",
    "mode": "enforce",
    "payload": {
      "messages": [
        {"role": "user", "content": "my card is 4111 1111 1111 1111 and token sk-test-abc"}
      ]
    }
  }' | jq
```

2. Проверить response:
- присутствуют `request_id`, `final_action`, `explain`, `audit`;
- при `final_action=block` поле `sanitized_payload` отсутствует;
- при неблокирующем действии структура `payload` сохраняется.

## Dry-Run Parity Verification

1. Повторить тот же payload в `mode=dry_run`:

```bash
curl -sS -X POST http://127.0.0.1:8080/v1/sanitize/evaluate \
  -H "Authorization: Bearer $POKROV_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "profile_id": "strict",
    "mode": "dry_run",
    "payload": {
      "messages": [
        {"role": "user", "content": "my card is 4111 1111 1111 1111 and token sk-test-abc"}
      ]
    }
  }' | jq
```

2. Подтвердить parity:
- `final_action` и category hit counts совпадают с enforce режимом;
- `executed=false`;
- отсутствуют upstream side effects.

## Metadata-Only Safety Verification

1. Проверить audit/explain payload:

```bash
curl -sS -X POST http://127.0.0.1:8080/v1/sanitize/evaluate \
  -H "Authorization: Bearer $POKROV_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "profile_id": "custom",
    "mode": "enforce",
    "payload": {
      "messages": [
        {"role": "user", "content": "Project Andromeda, email: user@example.com"}
      ]
    }
  }' > /tmp/sanitize-result.json
```

2. Убедиться, что в `/tmp/sanitize-result.json` нет raw fragments из исходного
   текста в секциях `audit` и `explain`; разрешены только counts/category/action
   metadata.

3. Проверить structured logs:
- есть `request_id`, `profile_id`, `final_action`, `rule_hits_total`;
- нет raw prompt, raw match fragments и secret-like строк.

## Performance Verification (Baseline)

1. Запустить baseline набор evaluate запросов (warm-up + 3 измеряемых серии).
2. Зафиксировать p50/p95/p99 latency только для sanitize/evaluate overhead.
3. Признать pass, если выполняются:
- p95 <= 50 ms
- p99 <= 100 ms
- детерминированность результата для одинакового input/config = 100%

## Expected Evidence

- Unit tests покрывают detection rules, overlap resolution и transform actions.
- Integration tests покрывают allow/mask/replace/redact/block + dry-run parity.
- Security checks подтверждают metadata-only audit/log/explain outputs.
- Performance checks подтверждают целевой latency overhead v1.

## Final Verification Path

Run the dedicated sanitization suites before acceptance:

```bash
cargo test --test contract sanitization_evaluate_contract
cargo test --test integration sanitization_evaluate_flow sanitization_transform_flow sanitization_audit_explain_flow
cargo test --test security sanitization_metadata_leakage
cargo test --test performance sanitization_evaluate_latency
```
