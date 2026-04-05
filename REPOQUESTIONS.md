# REPOQUESTIONS

## Purpose

This questionnaire is designed to evaluate whether a repository's tooling, measurements, and verification pipeline are good enough to support trustworthy engineering. It is intentionally language agnostic. It can be applied to repositories written in C, C++, Rust, Python, JavaScript, TypeScript, Java, Go, or mixed stacks.

The emphasis is not on whether the repository contains a fashionable set of tools. The emphasis is on whether the repository can produce persuasive evidence about:

- correctness
- robustness
- performance
- maintainability
- security
- diagnosability
- governance

This document is therefore a meta-quality instrument: it evaluates the system that evaluates the code.

## How To Use This Questionnaire

For each question, assign a maturity score:

```python
score_scale = {
    0: "Absent. No meaningful mechanism exists.",
    1: "Ad hoc. The mechanism exists informally or only for one person.",
    2: "Repeatable. The mechanism can be run by others with moderate effort.",
    3: "Enforced. The mechanism is part of routine local or CI workflow.",
    4: "Institutionalized. The mechanism is enforced, measured, documented, and produces reviewable artifacts.",
}
```

Compute section scores and the overall score as follows:

```python
section_score = sum(question_scores) / len(question_scores)

overall_score = sum(section_scores.values()) / len(section_scores)

grade = (
    "A" if overall_score >= 3.50 else
    "B" if overall_score >= 3.00 else
    "C" if overall_score >= 2.25 else
    "D" if overall_score >= 1.50 else
    "F"
)
```

The minimum standard for "good enough" should not depend only on the average. Critical sections should also clear a floor:

```python
good_enough = (
    overall_score >= 3.00
    and min(
        section_scores["buildability"],
        section_scores["testing"],
        section_scores["security"],
        section_scores["diagnostics"],
    ) >= 2.50
)
```

## Evidence Rule

Never answer these questions from aspiration alone. Require evidence:

- command lines
- CI workflow files
- report artifacts
- JSON summaries
- coverage outputs
- benchmark outputs
- fuzz crash artifacts
- documentation pages
- logs and runtime metrics

If the repository cannot point to the evidence, score the answer lower.

```python
claimed_capability_is_creditworthy = (
    capability_is_documented
    and capability_is_runnable
    and capability_emits_artifacts
)
```

## Section 1: Buildability And Reproducibility

1. Is there a single canonical entrypoint for routine quality checks?
2. Can a new contributor build the repository without tribal knowledge?
3. Are local build commands consistent with CI build commands?
4. Are important toolchain versions pinned or otherwise stabilized?
5. Can the repository produce a runnable artifact, not merely compile sources?
6. Is deployment or packaging smoke-tested somewhere, locally or in CI?
7. Are platform-specific build caveats documented explicitly?
8. Can a clean environment reproduce the build with acceptable effort?

```python
buildability_section = {
    "canonical_entrypoint": q1,
    "new_contributor_bootstrap": q2,
    "local_ci_parity": q3,
    "toolchain_stability": q4,
    "artifact_production": q5,
    "packaging_or_deploy_smoke": q6,
    "platform_documentation": q7,
    "clean_environment_reproducibility": q8,
}
```

## Section 2: Local Enforcement

1. Are there local hooks or wrapper scripts that catch errors before push?
2. Do local hooks cover more than formatting alone?
3. Are fast checks separated from expensive checks?
4. Can local enforcement be installed and used without manual surgery?
5. Are local enforcement failures understandable to contributors?
6. Does local enforcement produce stable, deterministic outcomes?

```python
local_enforcement_section = {
    "has_local_hooks": q1,
    "substantive_local_checks": q2,
    "fast_vs_slow_separation": q3,
    "installability": q4,
    "failure_clarity": q5,
    "determinism": q6,
}
```

## Section 3: Continuous Integration Governance

1. Does CI run on pull requests and on protected branches?
2. Does CI test the same things developers are expected to run locally?
3. Are there separate lanes for routine checks and deep checks?
4. Are CI artifacts retained for inspection?
5. Are failed checks actionable rather than opaque?
6. Are workflow definitions themselves linted or scanned for risk?
7. Is there any evidence that the repository distinguishes smoke checks from release checks?

```python
ci_governance_section = {
    "pr_and_branch_coverage": q1,
    "local_ci_alignment": q2,
    "routine_vs_deep_lanes": q3,
    "artifact_retention": q4,
    "failure_actionability": q5,
    "workflow_security": q6,
    "smoke_vs_release_differentiation": q7,
}
```

## Section 4: Static Analysis And Hygiene

1. Is formatting enforced automatically?
2. Is linting enforced automatically?
3. Are configuration files validated?
4. Are spelling or documentation hygiene checks present?
5. Are dependency policies or license policies enforced?
6. Are security advisories or supply-chain risks scanned?
7. Are unused dependencies or dead imports detected?
8. Are there any language-specific advanced analyzers in use?

```python
static_analysis_section = {
    "formatter": q1,
    "linter": q2,
    "config_validation": q3,
    "doc_hygiene": q4,
    "dependency_policy": q5,
    "advisory_scanning": q6,
    "unused_dependency_detection": q7,
    "advanced_static_analysis": q8,
}
```

## Section 5: Dynamic Testing

1. Are unit tests present?
2. Are integration tests present?
3. Are end-to-end or scenario tests present where architecture warrants them?
4. Are negative tests present for malformed input or forbidden states?
5. Are regression tests preserved when bugs are fixed?
6. Are replay-style tests for previously interesting inputs preserved?
7. Are tests structured so that failures can be localized?
8. Does the test runner emit machine-readable output?

```python
testing_section = {
    "unit_tests": q1,
    "integration_tests": q2,
    "scenario_tests": q3,
    "negative_tests": q4,
    "regression_preservation": q5,
    "replay_tests": q6,
    "localizable_failures": q7,
    "machine_readable_test_output": q8,
}
```

## Section 6: Coverage And Gap Analysis

1. Does the repository measure code coverage?
2. Is coverage scoped to runtime or production code rather than being diluted by tests and tooling?
3. Are there thresholds for critical files or modules?
4. Are multiple coverage dimensions tracked, such as lines, functions, or regions?
5. Are coverage failures blocking where appropriate?
6. Are coverage reports published as artifacts?
7. Is the coverage lane itself reliable?
8. Does the repository explicitly document what coverage does not measure?

```python
coverage_section = {
    "coverage_exists": q1,
    "coverage_scope_quality": q2,
    "critical_file_thresholds": q3,
    "multiple_dimensions": q4,
    "coverage_gating": q5,
    "artifact_publication": q6,
    "lane_reliability": q7,
    "declared_blind_spots": q8,
}
```

## Section 7: Complexity And Maintainability

1. Does the repository measure complexity?
2. Are hotspot functions or files ranked explicitly?
3. Is there any structural cleanliness measurement distinct from ordinary linting?
4. Does the repository penalize oversized modules or mixed responsibilities?
5. Are inline tests in production code treated deliberately rather than accidentally?
6. Are complexity findings used to prioritize refactoring work?
7. Are the formulas or heuristics documented?

```python
maintainability_section = {
    "complexity_measurement": q1,
    "ranked_hotspots": q2,
    "structural_cleanliness": q3,
    "size_and_responsibility_limits": q4,
    "test_production_separation": q5,
    "refactoring_prioritization": q6,
    "formula_transparency": q7,
}
```

## Section 8: Fuzzing And Adversarial Robustness

1. Does the repository fuzz any untrusted boundary?
2. Are fuzz targets selected by risk rather than convenience?
3. Are fuzz runs bounded and repeatable enough for CI or local smoke use?
4. Are discovered crashes preserved as artifacts?
5. Are saved crashes replayed as deterministic tests?
6. Is there any notion of fuzz coverage or target breadth?
7. Are findings converted into a hardening queue or remediation backlog?
8. Is it clear which important surfaces are not yet fuzzed?

```python
fuzzing_section = {
    "fuzzing_exists": q1,
    "risk_based_targeting": q2,
    "bounded_execution": q3,
    "artifact_preservation": q4,
    "replay_regressions": q5,
    "fuzz_coverage_measurement": q6,
    "remediation_queue": q7,
    "declared_unfuzzed_surfaces": q8,
}
```

## Section 9: Mutation Testing

1. Is mutation testing present?
2. Is it applied to high-value logic rather than indiscriminately?
3. Are long mutation campaigns shardable or otherwise operationally manageable?
4. Are mutation reports retained as artifacts?
5. Is mutation strength measured explicitly?
6. Is mutation testing part of a schedule or regular program, not only a one-off experiment?

```python
mutation_section = {
    "mutation_exists": q1,
    "high_value_targeting": q2,
    "operational_manageability": q3,
    "artifact_retention": q4,
    "mutation_strength_measurement": q5,
    "regularity": q6,
}
```

## Section 10: Performance And Resource Budgets

1. Are there explicit numerical performance budgets?
2. Are those budgets tied to a named reference environment?
3. Are microbenchmarks present for hot paths?
4. Are end-to-end or system-level performance gates present?
5. Are memory and resource budgets measured, not merely latency?
6. Are long-running soak tests present where state leakage is possible?
7. Are performance regressions treated as release blockers where appropriate?

```python
performance_section = {
    "explicit_budgets": q1,
    "reference_environment": q2,
    "microbenchmarks": q3,
    "system_level_gates": q4,
    "resource_budgeting": q5,
    "soak_testing": q6,
    "release_blocking_regressions": q7,
}
```

## Section 11: Frontend And UI Measurement

This section should be applied whenever a repository contains a UI, browser client, desktop shell, or other interactive presentation layer.

1. Is the frontend tested separately from the backend?
2. Are headless or smoke checks present for the UI layer?
3. Are runtime performance monitors exposed?
4. Are render-path, frame-loop, or draw-call diagnostics available?
5. Can the frontend emit machine-readable runtime artifacts?
6. Can the frontend explain its own lag or rendering failures?
7. Are frontend quality signals incorporated into the broader quality dashboard?

```python
frontend_section = {
    "separate_frontend_validation": q1,
    "headless_or_smoke_checks": q2,
    "runtime_monitors": q3,
    "render_or_frame_diagnostics": q4,
    "machine_readable_artifacts": q5,
    "self_diagnosis": q6,
    "dashboard_integration": q7,
}
```

## Section 12: Security And Supply Chain

1. Are dependency vulnerabilities scanned?
2. Are licenses and dependency sources governed?
3. Are CI workflows themselves examined for insecure patterns?
4. Are secrets or deployment assumptions kept out of source control?
5. Is there any mechanism to identify risky transitive dependencies?
6. Is the security posture documented sufficiently for reviewers to interpret failures?

```python
security_section = {
    "advisory_scans": q1,
    "dependency_policy": q2,
    "workflow_security": q3,
    "secret_hygiene": q4,
    "transitive_dependency_awareness": q5,
    "interpretability": q6,
}
```

## Section 13: Documentation And Explainability

1. Is there a generated documentation surface?
2. Are docs versioned with the code?
3. Is publication coverage known?
4. Are API docs generated automatically where relevant?
5. Are design, architecture, or operational docs present?
6. Do reports explain caveats and known gaps, rather than only emit scores?
7. Can a reviewer or LLM navigate the repository from generated documentation alone?

```python
documentation_section = {
    "generated_docs": q1,
    "docs_versioned_with_code": q2,
    "publication_coverage": q3,
    "api_docs": q4,
    "architecture_and_ops_docs": q5,
    "caveat_reporting": q6,
    "navigability": q7,
}
```

## Section 14: Diagnostics And Troubleshooting Readiness

1. Can the repository emit a structured diagnostics bundle after failure?
2. Are logs structured and searchable?
3. Are runtime metrics exposed in machine-readable form?
4. Are client-side and server-side diagnostics correlatable?
5. Are artifact paths and filenames stable enough for automation?
6. Is there a documented debug handoff procedure?
7. Are diagnostics detailed enough for an LLM to reason from them?

```python
diagnostics_section = {
    "structured_bundle": q1,
    "structured_logs": q2,
    "machine_readable_metrics": q3,
    "cross_layer_correlation": q4,
    "stable_artifact_paths": q5,
    "documented_handoff": q6,
    "llm_usable_diagnostics": q7,
}
```

## Section 15: Governance, Drift, And Honest Self-Assessment

1. Does the repository distinguish between enforced gates and advisory reports?
2. Does it record known gaps and reasons?
3. Are report formulas visible rather than hidden?
4. Are stale or failing report lanes noticed and treated as quality work?
5. Does the repository retain old findings long enough to create accountability?
6. Are there signs of documentation drift between what the repo claims and what scripts actually do?
7. Is there a process for improving the quality pipeline itself?

```python
governance_section = {
    "gate_report_distinction": q1,
    "known_gaps_are_recorded": q2,
    "formula_visibility": q3,
    "measurement_lane_health": q4,
    "finding_retention": q5,
    "drift_awareness": q6,
    "pipeline_improvement_process": q7,
}
```

## Interpreting The Results

The same overall score can hide very different risk profiles. For that reason, interpret the questionnaire by section before interpreting the average.

```python
interpretation = {
    "A": "The repository exhibits mature, multi-layer quality governance.",
    "B": "The repository is strong but still has meaningful blind spots.",
    "C": "The repository is serviceable, but its pipeline is not yet persuasive under stress.",
    "D": "The repository depends too heavily on trust, heroics, or manual review.",
    "F": "The repository lacks a credible quality pipeline.",
}
```

A repository with strong testing but no diagnostics is fragile. A repository with fuzzing but no artifact publication is opaque. A repository with coverage but no thresholds is descriptive rather than governing.

The best repositories exhibit overlap:

```python
high_confidence_repo = (
    buildability_is_repeatable
    and static_analysis_is_enforced
    and testing_is_layered
    and adversarial_inputs_are_exercised
    and performance_is_budgeted
    and failures_are_explainable
)
```

## Minimal "Good Enough" Checklist

If a quick triage is required, the following minimal checklist is a reasonable threshold for a healthy engineering pipeline:

1. One canonical local quality command exists.
2. CI runs on pull requests and protected branches.
3. Formatting and linting are enforced.
4. Unit and integration tests exist.
5. Critical-path tests are blocking.
6. Coverage is measured and at least some critical files have thresholds.
7. At least one adversarial test mode exists, such as fuzzing or strong negative testing.
8. Performance or resource budgets are explicit for critical workflows.
9. Machine-readable artifacts are produced.
10. There is a documented method to troubleshoot failures after deployment.

```python
minimal_good_enough = all([
    canonical_quality_command,
    pr_ci,
    enforced_format_and_lint,
    layered_tests,
    blocking_critical_tests,
    measured_coverage_with_thresholds,
    adversarial_testing_exists,
    explicit_performance_budgets,
    machine_readable_artifacts,
    documented_troubleshooting_path,
])
```

If a repository cannot satisfy this minimum, then the answer to "is the tooling good enough?" should generally be "not yet."

## Final Question

The most important question is not "does this repository have many tools?" It is:

> If a subtle bug, performance regression, security defect, or deployment failure occurs tomorrow, can the repository produce enough trustworthy evidence to explain what happened and prevent recurrence?

If the answer is no, then the pipeline is not yet adequate, no matter how attractive its badges or dashboards may appear.
