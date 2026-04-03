# Quickstart: Bootstrap Runtime

## Prerequisites

- Rust stable toolchain
- Container runtime for self-hosted validation
- YAML config file with env-backed secret references

## Local Run

1. Подготовить конфиг по примеру `config/pokrov.example.yaml`.
2. Передать обязательные env vars для секретных значений.
3. Запустить сервис.
4. Проверить:
   - `GET /health` возвращает успешный JSON-ответ
   - `GET /ready` становится успешным после завершения старта

## Container Run

1. Собрать контейнерный образ сервиса.
2. Подмонтировать YAML-конфиг и secret/env values.
3. Запустить контейнер с пробросом service port.
4. Проверить служебные endpoint'ы и graceful shutdown через остановку контейнера.

## Expected Behavior

- Невалидный конфиг блокирует успешный startup.
- Каждый ответ содержит `request_id`.
- Structured logs содержат lifecycle-события без raw payload.
