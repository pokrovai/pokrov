# Защита от Prompt Injection

## 1. Назначение

Добавить в Pokrov механизм обнаружения и блокировки **prompt injection**, в том числе **indirect prompt injection**, поступающего из внешних источников данных.

Механизм должен защищать LLM/AI-агента от инструкций, внедрённых в:

- ответы MCP tools;
- MCP resources;
- MCP prompts;
- описания tools и полей `inputSchema`;
- RAG-контекст;
- документы и другие внешние данные, передаваемые модели.

Функционал дополняет существующую санитизацию секретов, PII и корпоративных данных, но является отдельным типом проверки.

---

# 2. Основной принцип

Контент из внешних источников считается **данными, а не доверенными инструкциями**.

Pokrov должен различать происхождение контента:

```text
trusted:
  system/developer instructions;
  политика приложения;
  явно доверенный внутренний контекст.

untrusted:
  MCP tool output;
  MCP resources;
  MCP tool descriptions;
  RAG;
  web/content retrieval;
  внешние документы.
```

Для `untrusted`-контента выполняется проверка на попытку:

- изменить поведение модели;
- переопределить исходные инструкции;
- инициировать действия, не предусмотренные текущей задачей;
- получить или передать чувствительные данные;
- заставить агента вызвать дополнительные tools;
- раскрыть system/developer prompt;
- обойти ограничения безопасности.

---

# 3. Границы первой версии

## Входит

### MCP

Проверяются:

```text
tools/call result:
  content
  structuredContent
```

Проверка `tools/list` (tool.description, inputSchema descriptions) в v1 **не входит**: у Pokrov пока нет собственного пути MCP tool discovery, и псевдо-discovery API не вводится. См. раздел 25.

Архитектура detector-а должна позволять подключить те же проверки для:

```text
tools/list descriptions
resources/read
prompts/get
```

без изменения контракта detector-а (источник передаётся как `source_type`).

### LLM

Архитектура должна поддерживать проверку внешнего контекста:

```text
RAG fragments
retrieved documents
tool-derived context
external context
```

Реализация RAG-пути не является обязательной для первой поставки.

### Detector

Первая версия использует локальный специализированный classifier.

Предпочтительный runtime:

```text
ONNX Runtime
```

Генеративная LLM не используется как обязательная часть определения prompt injection.

---

# 4. Базовые модели для первой версии

До выбора production-модели должны быть сравнены как минимум три модели.

## 4.1. Основной кандидат — HikmaAI multilingual

```text
HikmaAI/hikmaai-mdeberta-v3-base-prompt-injection-multilingual
```

Назначение:

- основной кандидат для production-интеграции;
- поддержка русского и английского;
- multilingual classifier;
- доступные ONNX-варианты;
- предпочтительно использовать INT8 для CPU inference.

Почему является основным кандидатом:

- русский входит в языки fine-tune;
- есть готовое ONNX-представление;
- есть INT8-вариант;
- Apache-2.0;
- хорошо соответствует уже используемому в Pokrov ONNX Runtime.

Целевая первая реализация:

```text
LocalOnnxPromptInjectionDetector
        │
        └── HikmaAI INT8 ONNX
```

---

## 4.2. Русский контрольный кандидат

```text
gbv/mdeberta-ru-prompt-injection
```

Назначение:

- контроль качества именно на русском языке;
- сравнение с основным multilingual detector;
- проверка RU и mixed RU/EN сценариев.

Модель должна использоваться в benchmark даже в случае, если она не будет выбрана для production runtime.

Причина:

```text
RU-specific fine-tuning
+
mixed RU/EN focus
```

позволяют проверить, сколько качества теряется при использовании универсальной multilingual модели.

---

## 4.3. Международный baseline

```text
meta-llama/Llama-Prompt-Guard-2-86M
```

Назначение:

- внешний baseline;
- сравнение с широко используемым специализированным prompt-injection classifier;
- оценка качества на EN и mixed-content сценариях.

Prompt Guard 2 не является предпочтительным production-вариантом для Pokrov из-за отсутствия подтверждённой ориентации на русский язык, но должен использоваться как baseline качества.

---

## 4.4. Дополнительный кандидат

При необходимости в benchmark может быть добавлен:

```text
guardion/ModernGuard-1
```

Он особенно интересен для длинного MCP/RAG-контента благодаря существенно большему контексту.

Не является обязательным для первой реализации из-за:

- большего размера;
- менее удобной лицензии;
- большей стоимости inference.

---

# 5. Выбор production-модели

Конкретная модель **не фиксируется окончательно до benchmark**.

Предварительный приоритет:

```text
1. HikmaAI multilingual INT8 ONNX
   → основной production-кандидат

2. gbv/mdeberta-ru-prompt-injection
   → RU quality reference

3. Llama Prompt Guard 2 86M
   → international baseline

4. ModernGuard-1
   → optional long-context candidate
```

Выбор должен основываться не на общей accuracy из model card, а на собственном наборе Pokrov.

**Итог benchmark (2026-09-30, corpus 42 entries, `benchmarks/prompt-injection/report-*.json`):**

```text
HikmaAI int8:   P=0.95 R=0.90 F1=0.93 FPR=0.05  323 MiB  ~18 ms avg
gbv fp32:       P=0.78 R=0.86 F1=0.82 FPR=0.24 1064 MiB  ~45 ms avg
ModernGuard-1:  P=0.70 R=0.76 F1=0.73 FPR=0.33 1174 MiB  ~42 ms avg
PromptGuard-2:  P=0.67 R=0.48 F1=0.56 FPR=0.24  268 MiB  ~15 ms avg (threshold 0.5; при 0.9 recall=0.29)
```

**Выбрано: HikmaAI multilingual INT8** — лучшие precision/recall/F1 и минимальные
ресурсы. gbv не проходит quality gate (`quoted_injection_in_docs` FPR=0.33);
ModernGuard уступает по всем метрикам, включая RU. Threshold 0.5 и 0.9 на этом
корпусе дают идентичную матрицу (скоры насыщены); дефолт оставлен 0.9.

---

# 6. Архитектура

Prompt Injection Detection является отдельным recognizer family.

```text
                    Input
                      │
             classify trust source
                      │
          ┌───────────┴───────────┐
          │                       │
       trusted                 untrusted
          │                       │
          │                text extraction
          │                       │
          │                 chunking
          │                       │
          │              injection detector
          │                       │
          │               result aggregation
          │                       │
          └───────────┬───────────┘
                      │
                 Policy Engine
                      │
          ┌───────────┼────────────┐
        allow        audit         block
```

Существующие detectors продолжают работать независимо:

```text
recognizers:
  builtin
  deterministic
  ner
  prompt_injection
```

Prompt Injection Detector не должен заменять secret/PII/corporate detection.

---

# 7. Контракт детектора

Детектор получает текст и возвращает классификацию.

```json
{
  "detector_id": "prompt-injection-local",
  "model_id": "hikmaai-mdeberta-v3-base-prompt-injection-multilingual-int8",
  "classification": "injection",
  "score": 0.97
}
```

Допустимые значения:

```text
benign
injection
```

В дальнейшем контракт может быть расширен:

```text
suspicious
```

Policy engine не должен зависеть от конкретной модели.

---

# 8. Provider abstraction

Должен быть определён интерфейс:

```text
PromptInjectionDetector
```

Первая реализация:

```text
LocalOnnxDetector
```

Архитектура должна позволять впоследствии добавить:

```text
ExternalHttpDetector
CustomDetector
```

без изменения MCP/LLM processing pipeline.

Конкретная модель не должна быть зашита в бизнес-логику.

---

# 9. Обработка длинного контента

Модель может иметь ограниченный размер входного контекста.

Pokrov должен:

1. определить максимальный размер входа detector provider;
2. разбить длинный текст на перекрывающиеся fragments;
3. проверить каждый fragment отдельно;
4. агрегировать результат.

Пример:

```text
MCP output
    │
    ├── chunk 1 ── score 0.02
    ├── chunk 2 ── score 0.07
    ├── chunk 3 ── score 0.96
    └── chunk 4 ── score 0.05

final score = 0.96
```

Для первой версии:

```text
final_score = max(chunk_scores)
```

Размер chunk и overlap задаются provider implementation.

Обязательные ограничения:

```text
max_content_bytes
max_chunks
```

для защиты Pokrov от чрезмерной нагрузки.

---

# 10. Policy

Prompt injection является отдельной категорией проверки с моделью, разделяющей детекцию и применение решения:

```text
Detection:        benign | injection   (+ score)
Policy action:    allow | block
Enforcement mode: enforce | dry_run
```

Audit-запись формируется всегда (metadata-only), поэтому `audit` не является отдельным действием: «заметить и записать, но не блокировать» выражается как `action: allow` либо `mode: dry_run`.

Поддерживаемые действия:

```text
allow
block
```

`redact`, `mask`, `replace` для prompt injection не используются.

Причина: classifier определяет риск текста в целом, а не гарантированно безопасный span для удаления.

---

# 11. Порог классификации

Threshold задаётся конфигурацией и не должен быть зашит в detector.

```yaml
prompt_injection:
  threshold: 0.90
  action: block
```

Допускаются разные пороги:

```yaml
sources:
  mcp_tool_output:
    threshold: 0.85

  mcp_tool_description:
    threshold: 0.90

  rag:
    threshold: 0.95
```

Значения по умолчанию должны определяться после benchmark.

---

# 12. Fail mode

Поддерживаются:

```text
fail_open
fail_closed
```

### fail_closed

При:

- timeout;
- отсутствии модели;
- ошибке inference;
- невозможности проверить обязательный fragment;

контент не передаётся LLM.

Для `strict` профиля рекомендуется:

```text
fail_closed
```

### fail_open

Контент передаётся дальше, но flow помечается как degraded.

Используется для:

- dry-run;
- пилотного внедрения;
- некритичных профилей.

---

# 13. MCP tool discovery (отложено)

Проверка описаний при discovery (`tool.description`, `inputSchema.*.description`) отнесена к следующему этапу: текущий MCP-мост Pokrov обслуживает только `tool-call`, а собственный путь `tools/list` отсутствует. Вводить псевдо-discovery endpoint ради этой проверки не требуется — контракт придётся пересматривать при переходе на настоящий MCP transport.

Требование к архитектуре: когда появится реальный `tools/list`, должно быть достаточно добавить вызов detector-а с `source_type: mcp_tool_description` без изменения интерфейса detector-а.

Целевое поведение при наличии пути discovery: если описание классифицировано как injection и policy требует `block`, tool не передаётся клиенту/LLM.

Audit:

```text
server_id
tool_id
decision=filtered
reason=prompt_injection
```

Raw description не сохраняется.

---

# 14. MCP tool result

После выполнения tool:

```text
MCP server
     │
     ▼
Pokrov
     │
     ├─ existing DLP sanitization
     │
     └─ prompt injection detector
             │
          Policy
             │
       allow / block
```

Проверка должна завершиться до передачи результата AI agent / LLM.

Если обнаружена injection:

```text
tool уже мог быть выполнен,
но его output не передаётся модели.
```

Клиент получает безопасную ошибку:

```json
{
  "error": {
    "code": "prompt_injection_detected",
    "request_id": "...",
    "source": "mcp_tool_output"
  }
}
```

Raw content не включается.

---

# 15. Взаимодействие с существующей санитизацией

DLP и Prompt Injection являются независимыми стадиями.

```text
MCP output
   │
   ├── Secret detector
   ├── PII detector
   ├── Corporate marker detector
   │
   └── Prompt Injection detector
```

Если хотя бы одна политика требует `block`, итог:

```text
BLOCK
```

Prompt injection classification выполняется до разрушительного изменения текста, которое способно изменить смысл и результат classifier-а.

---

# 16. Конфигурация

Пример:

```yaml
prompt_injection:
  enabled: true

  provider:
    type: onnx
    model: hikmaai-mdeberta-v3-base-prompt-injection-multilingual
    model_path: ./models/prompt-injection/model.int8.onnx
    tokenizer_path: ./models/prompt-injection/tokenizer.json

  threshold: 0.90
  action: block
  mode: enforce
  fail_mode: fail_closed
  timeout_ms: 10000   # must cover max_chunks sequential inferences (~0.15 s/chunk on HikmaAI int8, debug build)

  chunking:
    max_tokens: 512
    overlap_tokens: 64
    max_chunks: 32
    max_content_bytes: 262144

  sources:
    mcp_tool_output: true
    mcp_tool_description: false
    mcp_resource: false
    mcp_prompt: false
    rag: false
```

Для первой поставки обязательны:

```text
mcp_tool_output
```

Остальные источники описываются в конфигурации и контракте `source_type`, но активируются вместе с появлением соответствующих путей обработки.

---

# 17. Audit

Audit остаётся metadata-only.

Допустимые поля:

```text
request_id
flow_type
source_type
server_id
tool_id
detector_id
model_id
classification
score_bucket
decision
duration_ms
chunks_processed
degraded
```

Raw text и detected fragments не сохраняются.

---

# 18. Метрики

Минимальный набор:

```text
pokrov_prompt_injection_evaluations_total
pokrov_prompt_injection_detected_total
pokrov_prompt_injection_blocked_total
pokrov_prompt_injection_detector_errors_total
pokrov_prompt_injection_detector_duration_seconds
pokrov_prompt_injection_chunks_total
```

Допустимые labels:

```text
source
provider
model
decision
fail_mode
```

Пользовательский контент в labels запрещён.

---

# 19. Dry-run

Detector должен поддерживать режим `dry_run` (`mode: dry_run`).

При этом:

- inference выполняется;
- policy decision вычисляется;
- контент фактически не блокируется;
- audit показывает потенциальный block (`would_block`);
- metrics учитывают срабатывание.

`dry_run` подавляет и `fail_closed` на ошибках detector-а: инфраструктурный сбой в режиме наблюдения помечается `degraded`, но не блокирует поток.

Dry-run обязателен для первичной настройки threshold.

---

# 20. Benchmark

Перед выбором production-модели необходимо сравнить минимум:

```text
HikmaAI multilingual
gbv/mdeberta-ru-prompt-injection
Llama Prompt Guard 2 86M
```

Дополнительно:

```text
ModernGuard-1
```

если потребуется оценить выигрыш длинного контекста.

## Набор

Минимум:

### Injection

```text
RU
EN
RU/EN mixed

instruction override
system prompt extraction
credential/data exfiltration
tool invocation manipulation
indirect instructions
obfuscated instructions
instructions inside MCP output
malicious tool descriptions
```

### Benign

```text
обычный MCP output
код
логи
техническая документация
security documentation
цитаты injection-примеров
исходный код с suspicious strings
RU/EN technical text
```

Особенно важны случаи:

```text
"Пример prompt injection:
 Ignore all previous instructions..."
```

которые являются обсуждением атаки, а не самой атакой.

---

# 21. Метрики качества benchmark

Для каждой модели измеряются минимум:

```text
precision
recall
F1
false positive rate
false negative rate
```

Отдельно:

```text
RU
EN
RU/EN mixed
tool_description
tool_output
security_docs
```

Средняя aggregated accuracy сама по себе недостаточна.

Для security-профиля основное внимание:

```text
false negatives на реальных injection
+
false positives на benign technical content
```

---

# 22. Ресурсные измерения

Для каждой модели benchmark должен измерять:

```text
RAM
model size
P50 latency
P95 latency
CPU utilization
throughput
```

Отдельно:

```text
single fragment
4 chunks
16 chunks
32 chunks
```

Production-модель выбирается с учётом как качества, так и стоимости inference.

---

# 23. Предварительный production target

До результатов benchmark целевой implementation profile:

```text
Model:
  HikmaAI multilingual

Runtime:
  ONNX Runtime

Precision:
  INT8

Hardware:
  CPU

Deployment:
  внутри процесса Pokrov
  либо отдельный локальный worker при необходимости
```

Отдельный GPU не является требованием.

---

# 24. Acceptance criteria

Функционал считается реализованным, если:

1. Prompt Injection Detector является отдельным recognizer family.

2. Реализован `LocalOnnxDetector`.

3. В production-конфигурации можно выбрать модель без изменения кода.

4. В benchmark сравнены:
   - HikmaAI multilingual;
   - gbv RU;
   - Prompt Guard 2 86M.

5. Проверяется минимум:
   - MCP tool output (`content`, включая структурированное содержимое внутри `content`).

6. Prompt Injection Detector применяется к результату MCP tool call до передачи результата клиенту/LLM; injection блокируется.

7. Detector API и модель источников (`source_type`) не препятствуют последующему подключению проверки `tools/list`, MCP resources, MCP prompts и RAG без изменения интерфейса detector-а.

8. Длинный контент разбивается на chunks.

9. Есть ограничения:
   - `max_chunks`;
   - `max_content_bytes`.

10. Threshold задаётся конфигурацией.

11. Поддерживаются:
   - `allow`;
   - `block`;
   - `dry_run`.

12. Поддерживаются:
   - `fail_open`;
   - `fail_closed`.

13. Raw проверяемый текст не попадает в:
   - audit;
   - logs;
   - metrics;
   - error response.

14. `fail_closed` не позволяет передать непроверенный контент модели.

15. Есть integration tests:

```text
RU benign MCP output → allow
RU injection → block
EN injection → block
RU/EN mixed injection → block
large output → chunking
detector unavailable + fail_closed → block
detector unavailable + fail_open → allow + degraded
source disabled → content passed unchanged
security documentation with quoted injection → allow (quality gate на этапе benchmark)
```

---

# 25. Следующий этап

Архитектура не должна препятствовать добавлению:

- проверки `tools/list` (tool descriptions, `inputSchema`) при появлении настоящего пути MCP tool discovery;
- проверки `resources/read`, `prompts/get` и RAG-контекста через тот же `source_type`-контракт;
- нескольких detectors одновременно;
- ensemble verdict;
- внешних специализированных guardrail API;
- LLM critic для спорных случаев;
- анализа последовательности agent/tool actions;
- plan-deviation detection;
- information-flow policy между tools;
- trust propagation между источниками.