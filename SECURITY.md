# Security Policy

## Supported Scope

Pokrov.AI v1 focuses on sanitization-first proxying for LLM and MCP traffic.
Security reports are accepted for issues that can impact:

- leakage of secrets, PII, or corporate markers;
- bypass of sanitization or policy enforcement;
- auth or rate-limit bypass;
- metadata-only audit guarantees;
- denial-of-service vectors in exposed runtime endpoints.

## Reporting a Vulnerability

Please report vulnerabilities privately to the project maintainers.
Include:

- affected version/commit;
- reproduction steps;
- impact assessment;
- suggested fix if available.

Do not open a public issue for unpatched vulnerabilities.

## Response Expectations

Maintainers will:

- acknowledge receipt;
- triage severity and scope;
- prepare and validate a fix;
- coordinate disclosure after patch availability.

## Disclosure Policy

Coordinated disclosure is preferred.
Public disclosure should happen only after a fix is released or maintainers confirm that immediate disclosure is safe.
