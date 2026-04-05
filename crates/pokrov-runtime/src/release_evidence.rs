use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseEvidence {
    pub release_id: String,
    pub generated_at: String,
    pub git_commit: String,
    pub environment: EvidenceEnvironment,
    pub performance: PerformanceEvidence,
    pub security: SecurityEvidence,
    pub operational: OperationalEvidence,
    pub artifacts: Vec<ArtifactChecksum>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    pub gate_status: GateStatus,
    pub failed_gates: Vec<String>,
    pub remediation: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceEnvironment {
    pub rust_version: String,
    pub os: String,
    pub cpu: String,
    pub benchmark_tool: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceEvidence {
    pub runs: u8,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub throughput_rps: f64,
    pub startup_seconds: f64,
    pub pass: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityEvidence {
    pub invalid_auth: GateStatus,
    pub rate_limit_abuse: GateStatus,
    pub log_safety: GateStatus,
    pub secret_handling: GateStatus,
    pub pass: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationalEvidence {
    pub metrics_coverage_percent: u8,
    pub readiness_behavior: GateStatus,
    pub graceful_shutdown_behavior: GateStatus,
    pub observability_behavior: GateStatus,
    pub pass: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactChecksum {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GateStatus {
    Pass,
    Fail,
}

impl ReleaseEvidence {
    pub fn build(
        release_id: String,
        git_commit: String,
        benchmark_tool: String,
        performance: PerformanceEvidence,
        security: SecurityEvidence,
        operational: OperationalEvidence,
        artifacts: Vec<ArtifactChecksum>,
        notes: Vec<String>,
    ) -> Self {
        let failed_gates = collect_failed_gates(&performance, &security, &operational);
        let gate_status = if failed_gates.is_empty() { GateStatus::Pass } else { GateStatus::Fail };
        let remediation = failed_gates.iter().map(|gate| remediation_for_gate(gate)).collect();

        Self {
            release_id,
            generated_at: OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string()),
            git_commit,
            environment: EvidenceEnvironment {
                rust_version: rustc_version(),
                os: std::env::consts::OS.to_string(),
                cpu: std::env::consts::ARCH.to_string(),
                benchmark_tool,
            },
            performance,
            security,
            operational,
            artifacts,
            notes,
            gate_status,
            failed_gates,
            remediation,
        }
    }
}

fn collect_failed_gates(
    performance: &PerformanceEvidence,
    security: &SecurityEvidence,
    operational: &OperationalEvidence,
) -> Vec<String> {
    let mut failed = Vec::new();

    if !performance.pass {
        failed.push("performance".to_string());
    }

    if security.invalid_auth == GateStatus::Fail {
        failed.push("security.invalid_auth".to_string());
    }
    if security.rate_limit_abuse == GateStatus::Fail {
        failed.push("security.rate_limit_abuse".to_string());
    }
    if security.log_safety == GateStatus::Fail {
        failed.push("security.log_safety".to_string());
    }
    if security.secret_handling == GateStatus::Fail {
        failed.push("security.secret_handling".to_string());
    }
    if !security.pass && !failed.iter().any(|gate| gate.starts_with("security.")) {
        failed.push("security".to_string());
    }

    if operational.readiness_behavior == GateStatus::Fail {
        failed.push("operational.readiness_behavior".to_string());
    }
    if operational.graceful_shutdown_behavior == GateStatus::Fail {
        failed.push("operational.graceful_shutdown_behavior".to_string());
    }
    if operational.observability_behavior == GateStatus::Fail {
        failed.push("operational.observability_behavior".to_string());
    }
    if !operational.pass && !failed.iter().any(|gate| gate.starts_with("operational.")) {
        failed.push("operational".to_string());
    }

    failed
}

fn remediation_for_gate(gate: &str) -> String {
    match gate {
        "performance" => {
            "Re-run performance verification and update latency/throughput baselines.".to_string()
        }
        "security.invalid_auth" => {
            "Fix unauthorized request handling and verify metadata-only error responses."
                .to_string()
        }
        "security.rate_limit_abuse" => {
            "Harden rate-limit enforcement and re-run abuse scenario checks.".to_string()
        }
        "security.log_safety" => {
            "Remove sensitive fields from logs and re-run log safety checks.".to_string()
        }
        "security.secret_handling" => {
            "Move secrets to env/file refs and re-validate deployment handling.".to_string()
        }
        "security" => {
            "Re-run all security verification suites and update evidence artifacts.".to_string()
        }
        "operational.readiness_behavior" => {
            "Fix readiness transitions and validate degraded/draining behavior.".to_string()
        }
        "operational.graceful_shutdown_behavior" => {
            "Fix graceful shutdown sequencing and validate inflight request draining.".to_string()
        }
        "operational.observability_behavior" => {
            "Fix metrics/log observability path and re-run operational checks.".to_string()
        }
        "operational" => {
            "Re-run operational verification suites and refresh release evidence.".to_string()
        }
        _ => format!("Resolve verification failure for gate '{gate}' and regenerate evidence."),
    }
}

pub fn collect_artifact_checksums(paths: &[PathBuf]) -> io::Result<Vec<ArtifactChecksum>> {
    let mut checksums = Vec::new();
    for path in paths {
        let bytes = fs::read(path)?;
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let digest = hasher.finalize();
        checksums.push(ArtifactChecksum {
            path: path.display().to_string(),
            sha256: hex::encode(digest),
        });
    }

    Ok(checksums)
}

pub fn write_release_evidence(path: &Path, evidence: &ReleaseEvidence) -> io::Result<()> {
    let body = serde_json::to_vec_pretty(evidence)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
    fs::write(path, body)
}

fn rustc_version() -> String {
    std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        GateStatus, OperationalEvidence, PerformanceEvidence, ReleaseEvidence, SecurityEvidence,
    };

    #[test]
    fn aggregate_gate_status_is_pass_only_when_all_sections_pass() {
        let evidence = ReleaseEvidence::build(
            "release-1".to_string(),
            "deadbee".to_string(),
            "k6".to_string(),
            PerformanceEvidence {
                runs: 3,
                p50_ms: 10.0,
                p95_ms: 30.0,
                p99_ms: 90.0,
                throughput_rps: 700.0,
                startup_seconds: 2.0,
                pass: true,
            },
            SecurityEvidence {
                invalid_auth: GateStatus::Pass,
                rate_limit_abuse: GateStatus::Pass,
                log_safety: GateStatus::Pass,
                secret_handling: GateStatus::Pass,
                pass: true,
            },
            OperationalEvidence {
                metrics_coverage_percent: 100,
                readiness_behavior: GateStatus::Pass,
                graceful_shutdown_behavior: GateStatus::Pass,
                observability_behavior: GateStatus::Pass,
                pass: true,
            },
            Vec::new(),
            Vec::new(),
        );

        assert_eq!(evidence.gate_status, GateStatus::Pass);
        assert!(evidence.failed_gates.is_empty());
        assert!(evidence.remediation.is_empty());
    }

    #[test]
    fn fail_evidence_contains_deterministic_failed_gates_and_remediation() {
        let evidence = ReleaseEvidence::build(
            "release-2".to_string(),
            "deadbee".to_string(),
            "k6".to_string(),
            PerformanceEvidence {
                runs: 3,
                p50_ms: 10.0,
                p95_ms: 30.0,
                p99_ms: 90.0,
                throughput_rps: 700.0,
                startup_seconds: 2.0,
                pass: false,
            },
            SecurityEvidence {
                invalid_auth: GateStatus::Pass,
                rate_limit_abuse: GateStatus::Fail,
                log_safety: GateStatus::Pass,
                secret_handling: GateStatus::Pass,
                pass: false,
            },
            OperationalEvidence {
                metrics_coverage_percent: 100,
                readiness_behavior: GateStatus::Fail,
                graceful_shutdown_behavior: GateStatus::Pass,
                observability_behavior: GateStatus::Pass,
                pass: false,
            },
            Vec::new(),
            Vec::new(),
        );

        assert_eq!(evidence.gate_status, GateStatus::Fail);
        assert_eq!(
            evidence.failed_gates,
            vec![
                "performance".to_string(),
                "security.rate_limit_abuse".to_string(),
                "operational.readiness_behavior".to_string(),
            ]
        );
        assert_eq!(evidence.remediation.len(), evidence.failed_gates.len());
        assert!(
            evidence.remediation.iter().all(|item| !item.trim().is_empty()),
            "each failed gate must have remediation guidance"
        );
    }
}
