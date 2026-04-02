# Pokrov.AI — спецификация v1 для реализации

**Документ:** Implementation Specification v1  
**Продукт:** Pokrov.AI  
**Статус:** Готово к передаче в разработку  
**Версия:** 1.0  
**Дата:** 30.03.2026  
**Язык реализации:** Rust  
**Целевой тип поставки:** self-hosted open-source сервис

---

# 1. Назначение документа

Этот документ описывает первую реализуемую версию Pokrov.AI на уровне, достаточном для передачи в разработку инженерной команде или ИИ coding-агенту.

Документ фиксирует:

- границы продукта v1;
    
- целевые сценарии;
    
- функциональные и нефункциональные требования;
    
- API-контракты;
    
- внутреннюю архитектуру;
    
- модели данных;
    
- правила обработки;
    
- требования к безопасности;
    
- тестовые и приемочные критерии;
    
- последовательность реализации.
    

Документ должен использоваться как основной source of truth для продуктового
поведения v1. Конституция проекта определяет инженерные правила, quality gates и
процесс изменения этого поведения, но не заменяет сам PRD как описание продукта.

---

# 2. Краткое описание продукта

## 2.1. Что такое Pokrov.AI

Pokrov.AI — это доверенный proxy-слой для взаимодействия **coding/AI-агентов** с:

- внешними или внутренними **LLM**;
    
- внешними или внутренними **MCP tools / MCP servers**.
    

Pokrov.AI стоит между агентом и внешним исполнителем и делает следующее:

1. принимает запрос агента;
    
2. определяет тип трафика и применимый профиль политики;
    
3. санитизирует контекст и payload;
    
4. валидирует tool access и tool arguments;
    
5. при необходимости блокирует запрос или редактирует его;
    
6. проксирует безопасную версию upstream;
    
7. при необходимости санитизирует ответ;
    
8. пишет аудит по метаданным.
    

## 2.2. Ключевая ценность v1

Первая версия продукта решает одну узкую задачу:

**сделать безопасным взаимодействие `agent ↔ LLM ↔ MCP tools` без встраивания кастомных guardrails в каждого агента.**

## 2.3. Что не является целью v1

Первая версия не является:

- полноценным AI gateway platform;
    
- полноценной AI security platform;
    
- DLP-системой общего назначения;
    
- системой IAM / RBAC;
    
- системой registry/governance для всех агентных протоколов;
    
- универсальным observability-продуктом.
    

---

# 3. Scope v1

## 3.1. Что входит в v1

### LLM path

- OpenAI-compatible endpoint для запросов агентов;
    
- проксирование к upstream LLM providers;
    
- sanitization входного prompt/context;
    
- optional sanitization output response;
    
- policy-based allow / redact / mask / replace / block.
    

### MCP path

- mediation layer / proxy перед approved MCP servers;
    
- allowlist MCP servers;
    
- allowlist MCP tools;
    
- валидация аргументов tool calls;
    
- sanitization tool outputs;
    
- optional sanitization MCP resources / prompt templates на минимальном уровне.
    

### Sanitization core

- regex detection для secrets;
    
- regex detection для базовых PII;
    
- custom rules для corporate markers;
    
- transformation engine;
    
- policy profiles;
    
- dry-run mode;
    
- explain summary.
    

### Operational core

- API key authentication;
    
- rate limiting;
    
- metadata-only audit;
    
- structured logs;
    
- Prometheus metrics;
    
- health / readiness endpoints.
    

## 3.2. Что исключено из v1

- A2A proxy;
    
- MCP registry как самостоятельный control plane;
    
- RBAC / role model;
    
- delegated authorization;
    
- heavy ML NER;
    
- phonetic matching;
    
- response caching;
    
- SIEM export;
    
- web UI / admin panel;
    
- multi-tenant billing/control plane.
    

---

# 4. Целевые пользователи и сценарии

## 4.1. Основной ICP

1. AI platform teams, строящие coding agents;
    
2. internal developer platform teams;
    
3. security/AppSec stakeholders, которым нужна единая точка снижения риска.
    

## 4.2. Основные сценарии использования

### UC-01. Санитизация prompt/context перед LLM

Агент отправляет messages и retrieved context в модель. Pokrov удаляет или маскирует секреты, PII и внутренние маркеры перед отправкой upstream.

### UC-02. Санитизация ответа модели

Ответ модели проверяется на утечки чувствительных данных перед возвратом агенту.

### UC-03. Контроль вызова MCP tool

Pokrov проверяет, что:

- MCP server разрешен;
    
- tool разрешен;
    
- аргументы вызова не нарушают политику.
    

### UC-04. Санитизация MCP tool output

Ответ инструмента проверяется на sensitive data и опасные инструкции до передачи агенту.

### UC-05. Dry-run внедрение

Команда может прогнать агентный трафик через Pokrov без enforcement и увидеть, что было бы замаскировано или заблокировано.

---

# 5. Термины и определения

|Термин|Значение|
|---|---|
|Agent|приложение или runtime, которое вызывает LLM и/или MCP tools|
|LLM request|запрос к модели: messages, prompt, context, tool-derived context|
|MCP server|внешний или внутренний сервер, предоставляющий tools/resources/prompts по MCP|
|Tool call|вызов конкретного MCP tool|
|Tool output|результат выполнения инструмента|
|Policy profile|набор правил, определяющий поведение sanitization и blocking|
|Detection|обнаруженный фрагмент sensitive content|
|Transformation|действие над обнаруженным фрагментом или запросом|
|Dry-run|режим оценки без фактического применения блокировок/редакции|
|Audit event|запись о факте обработки без сырого sensitive payload|

---

# 6. Product goals и non-goals

## 6.1. Goals

1. Дать platform teams единый reusable interaction proxy.
    
2. Снизить риск утечки sensitive context при работе coding agents.
    
3. Снизить риск unsafe interaction с MCP tools.
    
4. Дать безопасный путь внедрения через dry-run.
    
5. Обеспечить metadata-only audit.
    

## 6.2. Non-goals

1. Не строить полноценный enterprise AI gateway.
    
2. Не строить full governance suite.
    
3. Не строить IAM/RBAC систему.
    
4. Не закрывать все формы agentic security.
    
5. Не решать все классы structured data sanitization вне agent interaction.
    

---

# 7. Функциональные требования

## 7.1. LLM Proxy

### FR-LLM-01. Прием запросов

Сервис должен принимать LLM-запросы по HTTP(S) в формате, совместимом с OpenAI Chat Completions API.

### FR-LLM-02. Нормализация запроса

Сервис должен извлекать:

- messages;
    
- system/user/assistant content;
    
- tool-derived context, если он включен в payload;
    
- model identifier;
    
- stream flag;
    
- metadata fields, необходимые для аудита.
    

### FR-LLM-03. Sanitization входа

Сервис должен прогонять входной payload через sanitization pipeline до передачи upstream.

### FR-LLM-04. Действия по политике

Для каждого срабатывания или совокупности срабатываний сервис должен поддерживать действия:

- `allow`
    
- `mask`
    
- `replace`
    
- `redact`
    
- `block`
    

### FR-LLM-05. Проксирование

После обработки сервис должен проксировать разрешенный payload к настроенному upstream provider.

### FR-LLM-06. Sanitization ответа

Сервис должен уметь прогонять model response через output sanitization, если это включено политикой.

### FR-LLM-07. Поддержка стриминга

Сервис должен поддерживать streaming response mode в scope OpenAI-compatible SSE для базового сценария.

### FR-LLM-08. Multi-provider routing

Сервис должен поддерживать routing по `model` к настроенному upstream provider.

---

## 7.2. MCP Proxy / Mediation Layer

### FR-MCP-01. Поддержка проксирования

Сервис должен уметь принимать MCP-вызовы от клиента и передавать их approved upstream MCP server.

### FR-MCP-02. Allowlist по server

Сервис должен проверять, что целевой MCP server включен в allowlist.

### FR-MCP-03. Allowlist по tool

Сервис должен проверять, что вызываемый tool разрешен для использования.

### FR-MCP-04. Валидация аргументов

Сервис должен валидировать аргументы tool call:

- по схеме, если схема есть;
    
- по policy rules;
    
- по ограничениям на запрещенные паттерны.
    

### FR-MCP-05. Sanitization tool outputs

Сервис должен сканировать и редактировать tool output перед возвратом агенту.

### FR-MCP-06. Блокировка tool calls

Если MCP tool call нарушает allowlist или policy, сервис должен вернуть структурированную ошибку вместо выполнения.

### FR-MCP-07. Sanitization ресурсов и prompt templates

Если MCP flow содержит ресурсы или prompt templates в текстовом виде, сервис должен иметь возможность прогнать их через sanitization engine.

---

## 7.3. Detection Engine

### FR-DET-01. Secrets detection

Система должна находить как минимум:

- API keys;
    
- bearer tokens;
    
- private keys;
    
- passwords / credential-like patterns.
    

### FR-DET-02. Basic PII detection

Система должна находить как минимум:

- email;
    
- phone;
    
- credit card numbers;
    
- IBAN;
    
- базовые numeric identifiers по правилам.
    

### FR-DET-03. Corporate markers detection

Система должна находить:

- internal URLs;
    
- project codes;
    
- customer-specific identifiers;
    
- custom configured corporate patterns.
    

### FR-DET-04. Custom rules

Система должна поддерживать пользовательские regex-правила.

### FR-DET-05. Deterministic behavior

Одинаковый payload и одинаковая конфигурация должны приводить к одинаковому набору detections.

### FR-DET-06. Overlap resolution

При пересечении detections система должна применять deterministic merge policy.

---

## 7.4. Transformation Engine

### FR-TR-01. Placeholder replacement

Система должна уметь заменять detection на placeholder.

### FR-TR-02. Partial masking

Система должна уметь частично маскировать detection.

### FR-TR-03. Segment removal

Система должна уметь удалять сегмент или поле.

### FR-TR-04. Request blocking

Система должна уметь блокировать запрос целиком.

### FR-TR-05. Output validity preservation

После transformation payload должен оставаться структурно валидным относительно исходного протокола.

---

## 7.5. Policy Engine

### FR-POL-01. Policy profiles

Система должна поддерживать минимум три профиля:

- `minimal`
    
- `strict`
    
- `custom`
    

### FR-POL-02. Policy binding

Политика должна выбираться:

- по API key;
    
- по endpoint;
    
- по default profile.
    

### FR-POL-03. Rule enable/disable

Система должна позволять включать и выключать отдельные правила.

### FR-POL-04. Dry-run

Система должна поддерживать dry-run на уровне policy execution.

### FR-POL-05. Rule priorities

Система должна поддерживать приоритеты правил.

---

## 7.6. Audit и Explainability

### FR-AUD-01. Metadata-only audit

Система должна сохранять только метаданные обработки без сырого payload по умолчанию.

### FR-AUD-02. Audit payload

Audit event должен включать:

- request id;
    
- timestamp;
    
- flow type;
    
- profile id;
    
- rule hit counts;
    
- action taken;
    
- upstream target id;
    
- tool/server id при наличии.
    

### FR-AUD-03. Explain mode

Система должна возвращать summary, какие правила сработали и какие действия были применены.

---

## 7.7. Authentication и Rate Limiting

### FR-AUTH-01. API key authentication

Система должна поддерживать аутентификацию по API key.

### FR-AUTH-02. Rate limiting

Система должна поддерживать rate limiting:

- по API key;
    
- по request count;
    
- по token-like estimate для LLM path, если доступно.
    

---

## 7.8. Operations

### FR-OPS-01. Health endpoint

Система должна иметь `/health`.

### FR-OPS-02. Readiness endpoint

Система должна иметь `/ready`.

### FR-OPS-03. Structured logging

Система должна писать JSON-логи.

### FR-OPS-04. Prometheus metrics

Система должна экспонировать метрики Prometheus.

### FR-OPS-05. Graceful shutdown

Система должна корректно завершать активные запросы при остановке.

---

# 8. Нефункциональные требования

## 8.1. Производительность

|ID|Требование|Цель|
|---|---|---|
|NFR-PERF-01|P95 latency overhead|не более 50 мс|
|NFR-PERF-02|P99 latency overhead|не более 100 мс|
|NFR-PERF-03|Throughput|не менее 500 RPS на baseline-инстансе для типового payload|
|NFR-PERF-04|Startup time|не более 5 сек|

## 8.2. Безопасность

|ID|Требование|
|---|---|
|NFR-SEC-01|raw sensitive payload не должен попадать в лог по умолчанию|
|NFR-SEC-02|конфигурация секретов должна идти через env vars / secret mounts|
|NFR-SEC-03|продукт должен поддерживать zero-copy и минимизацию удержания sensitive data в памяти там, где practically возможно|
|NFR-SEC-04|аудит должен быть metadata-only by default|

## 8.3. Надежность

|ID|Требование|
|---|---|
|NFR-REL-01|graceful shutdown обязателен|
|NFR-REL-02|readiness должен отражать доступность конфигурации и upstream routing layer|
|NFR-REL-03|при ошибке upstream должен формироваться предсказуемый ответ|

## 8.4. Развертывание

|ID|Требование|
|---|---|
|NFR-DEP-01|self-hosted deployment — обязательный путь v1|
|NFR-DEP-02|продукт должен поставляться как контейнер|
|NFR-DEP-03|продукт должен запускаться без внешней control plane инфраструктуры|

## 8.5. Объяснимость

|ID|Требование|
|---|---|
|NFR-EXP-01|для каждого policy decision должен быть доступен rule summary|
|NFR-EXP-02|dry-run должен быть пригоден для настройки false positives|

---

# 9. API-спецификация v1

## 9.1. Общие принципы

- Все endpoint'ы должны принимать и возвращать JSON, если не указано иное.
    
- Все ответы должны содержать `request_id`.
    
- Все ошибки должны быть структурированными.
    

## 9.2. LLM Proxy API

### POST `/v1/llm/chat/completions`

#### Назначение

OpenAI-compatible proxy endpoint для agent requests.

#### Request (пример)

```json
{
  "model": "gpt-4o",
  "messages": [
    {"role": "system", "content": "You are a coding assistant"},
    {"role": "user", "content": "Вот токен sk-123... и код, помоги"}
  ],
  "stream": false,
  "metadata": {
    "agent_id": "cursor-agent-01",
    "profile": "strict"
  }
}
```

#### Response (пример)

```json
{
  "id": "chatcmpl-...",
  "object": "chat.completion",
  "created": 1774820000,
  "model": "gpt-4o",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "..."
      },
      "finish_reason": "stop"
    }
  ],
  "request_id": "req_...",
  "pokrov": {
    "profile": "strict",
    "sanitized": true,
    "rule_hits": 2,
    "action": "redact"
  }
}
```

#### Возможные коды ошибок

- `400` invalid request
    
- `401` invalid api key
    
- `403` blocked by policy
    
- `429` rate limited
    
- `502` upstream error
    
- `503` upstream unavailable
    

---

## 9.3. Evaluate API

### POST `/v1/evaluate`

#### Назначение

Dry-run / explain endpoint для оценки payload без реального проксирования.

#### Request

```json
{
  "flow_type": "llm_input",
  "profile": "strict",
  "payload": {
    "messages": [
      {"role": "user", "content": "пароль: secret123"}
    ]
  }
}
```

#### Response

```json
{
  "request_id": "req_...",
  "mode": "dry_run",
  "profile": "strict",
  "result": {
    "action": "redact",
    "rule_hits": [
      {
        "rule_id": "secret.password.pattern",
        "category": "secret",
        "count": 1
      }
    ],
    "summary": {
      "detections": 1,
      "blocked": false,
      "transformed": true
    }
  }
}
```

---

## 9.4. MCP Proxy API

### Базовый принцип

В v1 допускается реализация MCP mediation layer как:

- HTTP facade поверх approved MCP server interactions;
    
- или нативный MCP-aware proxy для выбранного набора transport patterns.
    

Требование к реализации v1:

- покрыть не все возможные MCP transports,
    
- а практический subset, достаточный для пилотного сценария.
    

### Минимальный HTTP control endpoint

#### POST `/v1/mcp/tool-call`

#### Request

```json
{
  "server": "repo-tools",
  "tool": "read_file",
  "arguments": {
    "path": "src/config/secrets.ts"
  },
  "metadata": {
    "agent_id": "cursor-agent-01",
    "profile": "strict"
  }
}
```

#### Response

```json
{
  "request_id": "req_...",
  "allowed": true,
  "sanitized": true,
  "result": {
    "content": "export const TOKEN = \"[API_KEY]\""
  },
  "pokrov": {
    "profile": "strict",
    "rule_hits": 1,
    "action": "replace"
  }
}
```

#### Block response

```json
{
  "request_id": "req_...",
  "allowed": false,
  "error": {
    "code": "tool_call_blocked",
    "message": "Tool call blocked by policy",
    "details": {
      "server": "repo-tools",
      "tool": "write_file"
    }
  }
}
```

---

## 9.5. Health API

### GET `/health`

Ответ: `200 OK` если процесс жив.

### GET `/ready`

Ответ: `200 OK` если:

- конфиг загружен;
    
- policy engine готов;
    
- upstream routing layer готов принимать трафик.
    

---

# 10. Модели данных

## 10.1. Detection

```rust
struct Detection {
    rule_id: String,
    category: DetectionCategory,
    start: usize,
    end: usize,
    confidence: Option<f32>,
    action: TransformAction,
    metadata: BTreeMap<String, String>,
}
```

## 10.2. DetectionCategory

```rust
enum DetectionCategory {
    Secret,
    Pii,
    Corporate,
    ToolRisk,
}
```

## 10.3. TransformAction

```rust
enum TransformAction {
    Allow,
    Mask,
    Replace,
    Redact,
    Block,
}
```

## 10.4. PolicyProfile

```rust
struct PolicyProfile {
    id: String,
    rules_enabled: Vec<String>,
    default_action: TransformAction,
    output_sanitization: bool,
    dry_run_allowed: bool,
    tool_policy: ToolPolicy,
}
```

## 10.5. ToolPolicy

```rust
struct ToolPolicy {
    allowed_servers: Vec<String>,
    allowed_tools: Vec<String>,
    blocked_tools: Vec<String>,
    validate_arguments: bool,
}
```

## 10.6. AuditEvent

```rust
struct AuditEvent {
    request_id: String,
    timestamp: String,
    flow_type: FlowType,
    profile_id: String,
    action: TransformAction,
    rule_hits_count: usize,
    upstream_target: Option<String>,
    server_id: Option<String>,
    tool_id: Option<String>,
    agent_id: Option<String>,
}
```

---

# 11. Архитектура реализации

## 11.1. Модули

### `pokrov-api`

Отвечает за:

- HTTP server;
    
- маршрутизацию;
    
- auth middleware;
    
- rate limit middleware;
    
- handlers.
    

### `pokrov-core`

Отвечает за:

- detection rules;
    
- transformation logic;
    
- policy application;
    
- explain summaries.
    

### `pokrov-proxy-llm`

Отвечает за:

- request normalization;
    
- upstream model routing;
    
- OpenAI-compatible contracts;
    
- response handling.
    

### `pokrov-proxy-mcp`

Отвечает за:

- MCP mediation/proxy logic;
    
- tool/server allowlist;
    
- argument validation;
    
- tool output handling.
    

### `pokrov-config`

Отвечает за:

- загрузку конфигурации;
    
- валидацию конфигурации;
    
- hot reload (если включен в v1.1; в v1 можно ограничиться fast reload через restart или базовым watcher).
    

### `pokrov-metrics`

Отвечает за:

- Prometheus metrics.
    

### `pokrov-runtime`

Отвечает за:

- startup/shutdown;
    
- readiness;
    
- runtime lifecycle.
    

## 11.2. Поток обработки LLM request

1. Request приходит на `/v1/llm/chat/completions`.
    
2. API layer делает auth и rate limit check.
    
3. Handler нормализует payload.
    
4. Policy selector определяет profile.
    
5. Detection engine ищет secrets / PII / corporate markers.
    
6. Transformation engine применяет действия.
    
7. Если action=`block`, возвращается policy error.
    
8. Иначе payload проксируется upstream.
    
9. При включенной output sanitization ответ также прогоняется через detection/transformation.
    
10. Генерируется audit event.
    
11. Возвращается итоговый response.
    

## 11.3. Поток обработки MCP tool call

1. Request приходит на MCP mediation endpoint.
    
2. API layer делает auth и rate limit check.
    
3. Проверяется server allowlist.
    
4. Проверяется tool allowlist/blocklist.
    
5. Аргументы валидируются.
    
6. При нарушении политики возвращается block error.
    
7. Иначе запрос отправляется upstream MCP server.
    
8. Tool output проходит sanitization.
    
9. Генерируется audit event.
    
10. Возвращается safe result.
    

---

# 12. Конфигурация v1

## 12.1. Общая конфигурация

Формат: YAML.

### Пример структуры

```yaml
server:
  host: 0.0.0.0
  port: 8080

security:
  api_keys:
    - key: env:POKROV_API_KEY
      profile: strict

policies:
  profiles:
    minimal:
      output_sanitization: false
      rules:
        - pii.email
        - secret.api_key
    strict:
      output_sanitization: true
      rules:
        - pii.email
        - pii.phone
        - secret.api_key
        - corporate.internal_url

mcp:
  servers:
    - id: repo-tools
      endpoint: http://repo-tools.internal
      allowed_tools:
        - read_file
        - grep
        - list_dir
      blocked_tools:
        - write_file
        - run_shell

llm:
  providers:
    - id: openai
      base_url: https://api.openai.com/v1
      models:
        - gpt-4o
        - gpt-4.1
```

## 12.2. Требования к конфигу

- конфиг должен валидироваться при старте;
    
- при невалидном конфиге сервис не должен переходить в ready;
    
- secrets не должны храниться в конфиге в открытом виде, кроме ссылок на env/secret mounts.
    

---

# 13. Безопасность

## 13.1. Обязательные принципы

1. Не хранить raw sensitive payload по умолчанию.
    
2. Не писать raw sensitive payload в логи.
    
3. Поддерживать explicit block path для опасных запросов.
    
4. Делать sanitization до отправки upstream.
    
5. Делать sanitization и на ответе, если профиль требует.
    

## 13.2. Аутентификация

В v1 достаточно:

- API key authentication.
    

## 13.3. Авторизация

В v1 полноценная модель ролей не требуется. Вместо нее используются:

- policy profiles;
    
- MCP allowlist / blocklist;
    
- binding профиля к API key.
    

---

# 14. Метрики и наблюдаемость

## 14.1. Обязательные метрики

- requests_total
    
- requests_blocked_total
    
- sanitization_rule_hits_total
    
- tool_calls_total
    
- tool_calls_blocked_total
    
- llm_proxy_latency_ms
    
- mcp_proxy_latency_ms
    
- upstream_errors_total
    
- rate_limited_total
    

## 14.2. Логирование

Формат логов: JSON.

Обязательные поля:

- timestamp
    
- level
    
- request_id
    
- component
    
- flow_type
    
- action
    
- profile
    

Запрещено писать:

- raw prompts;
    
- raw tool outputs;
    
- secrets;
    
- сырые фрагменты detections.
    

---

# 15. Тестовые требования

## 15.1. Unit tests

Обязательны для:

- detection rules;
    
- transformation logic;
    
- policy selection;
    
- allowlist checks;
    
- argument validation;
    
- audit serialization.
    

## 15.2. Integration tests

Обязательны для:

- LLM proxy path;
    
- MCP proxy path;
    
- dry-run mode;
    
- block path;
    
- output sanitization path.
    

## 15.3. Performance tests

Обязательны для:

- p95/p99 latency overhead;
    
- typical LLM request payload;
    
- typical tool output payload.
    

## 15.4. Security tests

Обязательны для:

- отсутствие raw payload в логах;
    
- правильная блокировка запрещенных tools;
    
- правильная обработка invalid API key;
    
- корректность rate limiting.
    

---

# 16. Критерии приемки

## 16.1. Критерии готовности функционала

Функционал считается реализованным, если:

- endpoint существует;
    
- happy path покрыт интеграционным тестом;
    
- block path покрыт тестом;
    
- audit event формируется;
    
- structured logs не содержат raw payload;
    
- документация по endpoint обновлена.
    

## 16.2. Критерии готовности релиза v1

v1 готова к пилоту, если:

- реализован LLM proxy path;
    
- реализован MCP mediation path;
    
- работают allowlist и basic policy profiles;
    
- работает prompt/tool output sanitization;
    
- есть dry-run;
    
- есть metadata-only audit;
    
- есть Prometheus metrics и health endpoints;
    
- latency находится в целевом диапазоне на типовых payloads.
    

---

# 17. План реализации

## Этап 1. Базовый каркас

- workspace;
    
- API server;
    
- config loader;
    
- request id;
    
- structured logging;
    
- health/ready.
    

## Этап 2. Sanitization core

- rules engine;
    
- detection engine;
    
- transformation engine;
    
- policy profiles;
    
- dry-run.
    

## Этап 3. LLM proxy

- OpenAI-compatible handler;
    
- provider routing;
    
- input sanitization;
    
- output sanitization;
    
- audit events.
    

## Этап 4. MCP mediation layer

- server/tool allowlist;
    
- argument validation;
    
- tool output sanitization;
    
- block path;
    
- audit events.
    

## Этап 5. Hardening

- rate limiting;
    
- performance tests;
    
- logging safety validation;
    
- release packaging.
    

---

# 18. Backlog v1.1 / v2

## v1.1

- lightweight ML NER;
    
- richer MCP support;
    
- better explainability;
    
- Python SDK;
    
- TypeScript SDK;
    
- coding-agent presets.
    

## v2

- A2A support;
    
- delegated authorization;
    
- richer policy model;
    
- registry/governance components;
    
- enterprise integrations.
    

---

# 19. Итоговое решение

Реализуемая v1 Pokrov.AI — это **узкий, self-hosted, sanitization-first proxy-слой**, который делает безопасным взаимодействие **coding agents ↔ LLM ↔ MCP tools**.

Если в процессе реализации возникает конфликт между:

- расширением scope,
    
- добавлением новых протоколов,
    
- добавлением governance/control-plane функций,
    

то приоритет всегда отдается:

1. стабильности core interaction paths;
    
2. качеству sanitization;
    
3. explainability;
    
4. простоте внедрения platform teams.
