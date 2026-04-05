# GLOSSARY

## Scope

This glossary defines the specialized vocabulary, acronyms, and tool names used in [CODEQUALITY.md] and [REPOQUESTIONS.md]. The definitions are intentionally pragmatic. They describe how each term is used in quality-pipeline analysis rather than attempting to give exhaustive textbook treatments.

## Acronyms And Abbreviations

- **AFL++**: A coverage-guided fuzzing system descended from American Fuzzy Lop. In these documents it appears as one example of a fuzzer that can generate hostile inputs automatically.
- **API**: Application Programming Interface. A stable boundary through which code, tools, or external callers interact with a component.
- **CI**: Continuous Integration. An automated execution environment that runs build, test, lint, reporting, and related workflow steps on commits, branches, or pull requests.
- **HTML**: HyperText Markup Language. A standard text format for publishing browser-readable reports and documentation pages.
- **HTTP**: Hypertext Transfer Protocol. The network protocol commonly used for web requests and responses.
- **JSON**: JavaScript Object Notation. A structured, machine-readable data format used for reports, diagnostics, and tool output.
- **LLM**: Large Language Model. In these documents, an automated reasoning system that benefits from stable paths, structured artifacts, and explicit formulas.
- **MI**: Maintainability Index. A composite maintainability metric produced by some static-analysis tools.
- **RSS**: Resident Set Size. The portion of a process's memory that is currently held in physical RAM.
- **SLOC**: Source Lines of Code. A count of lines considered to contain source logic rather than comments or blanks, depending on the analyzer.
- **TOML**: Tom's Obvious, Minimal Language. A configuration-file format commonly used by Rust tooling and other developer infrastructure.
- **UI**: User Interface. The visible or interactive presentation layer of an application.
- **YAML**: YAML Ain't Markup Language. A human-readable configuration and data format frequently used in CI and deployment files.

## Quality, Measurement, And Governance Terms

- **Adversarial testing**: Testing that intentionally stresses a system with malformed, hostile, extreme, or unexpected inputs rather than only confirming happy-path behavior.
- **Artifact**: A file or dataset produced by a pipeline step and retained as evidence, such as a report, log, coverage export, crash file, or benchmark output.
- **Artifact publication**: The practice of retaining and exposing generated artifacts so failures and scores can be inspected after a run completes.
- **Backend**: The server-side or non-UI portion of a system that implements core logic, storage, or service behavior.
- **Bootstrap**: The minimum setup process required to make a repository buildable and testable on a fresh machine or environment.
- **Buildability**: The degree to which a repository can be compiled, interpreted, packaged, or otherwise turned into runnable output in a repeatable way.
- **Call graph**: A representation of which functions or modules invoke which other functions or modules.
- **Canonical entrypoint**: The one preferred command or script that contributors are expected to use for routine quality checks.
- **Clean-code report**: In these documents, a structural report that grades file size, production-test separation, and related hygiene signals rather than just logical correctness.
- **Complexity report**: A generated report that measures and ranks structural difficulty, usually at file and function level.
- **Compiler**: A tool that translates source code into another form, often machine code or bytecode, before execution.
- **Coverage**: A family of measurements describing how much of the code was exercised by a test or execution run.
- **Coverage gate**: A blocking rule that requires measured coverage to stay above an explicit threshold.
- **Critical path**: The small set of workflows or components whose failure most directly harms correctness, performance, or release readiness.
- **Crate**: The basic Rust packaging and compilation unit, analogous to a package, module bundle, or library target in other ecosystems.
- **Dependency policy**: A repository rule that constrains which dependencies, versions, sources, or licenses are acceptable.
- **Deployment**: The act of installing or releasing runnable software into an environment where it can actually be used.
- **Diagnosability**: The degree to which a system can explain its own failures through logs, metrics, traces, reports, or debug bundles.
- **Drift**: The divergence between what the repository claims to measure and what its scripts, reports, or workflows actually do.
- **Dynamic correctness**: Evidence about behavior obtained by executing the system, such as tests, scenario runs, fuzzing, or soak runs.
- **Frontend**: The user-facing or presentation-facing portion of a system, such as a browser client or desktop UI.
- **Formal methods**: Techniques that use mathematical specifications, proofs, or model checking to reason about software behavior more rigorously than ordinary testing.
- **Function coverage**: A coverage measure indicating which functions were executed by tests or instrumented runs.
- **Generated documentation**: Documentation that is built automatically from source material or annotations rather than curated entirely by hand.
- **Governance**: The set of mechanisms that turn quality measurement into enforceable engineering behavior rather than optional advice.
- **Hardening queue**: A prioritized list of robustness or maintainability work generated from measurements such as complexity, fuzzing, or saved failures.
- **Headless**: Running a program or test without an interactive graphical display.
- **Heuristic**: A practical scoring or detection rule that is useful but not a formal semantic guarantee.
- **Hotspot**: A function, file, or module that ranks poorly on some risk signal and therefore deserves prioritized attention.
- **Inline test**: Test logic written inside a production source file rather than in a dedicated test file or test module.
- **Machine-readable**: Structured in a format that software can parse reliably, such as JSON, XML, CSV, or stable plain-text tables.
- **Measurement lane**: One distinct pipeline path that measures a particular property, such as coverage, lint, fuzzing, or documentation publication.
- **Interpreter**: A tool or runtime that executes source code or an intermediate representation directly, usually without a separate ahead-of-time compile step.
- **Line coverage**: A coverage measure indicating which source lines were executed by tests or instrumented runs.
- **License policy**: A repository rule that constrains which software licenses may appear in dependencies or distributed artifacts.
- **Operational diagnostics**: The evidence produced during runtime or deployment that helps explain failures after software has already been built or released.
- **Packaging**: The process of assembling built software into a distributable artifact, image, archive, bundle, or installer.
- **Policy check**: A gate that verifies repository rules such as dependency policy, workflow safety, or documentation publication requirements.
- **Protected branch**: A version-control branch on which direct modification, merging, or release actions are restricted by repository policy.
- **Pull request**: A reviewable change proposal submitted to a shared repository before merge.
- **Publication coverage**: The proportion of intended documentation or report content that is actually emitted into the published artifact set.
- **Quality gate**: A binary pass-fail condition that blocks further progress when violated.
- **Reference environment**: A named machine, operating system, runtime, or configuration against which performance and resource budgets are defined.
- **Regression**: A defect in which behavior that previously worked now fails or degrades.
- **Region coverage**: A coverage measure that tracks execution of finer-grained control-flow regions rather than only whole lines or whole functions.
- **Report lane**: A pipeline lane whose primary purpose is to produce diagnostic or ranking artifacts rather than block immediately.
- **Reproducibility**: The degree to which a build, report, or test outcome can be regenerated reliably by different people or machines.
- **Registry**: A service or index from which packages, crates, images, or other dependencies are fetched.
- **Runtime**: The state in which software is actively executing, as opposed to merely being built or statically analyzed.
- **Runtime monitor**: A metric or timing source sampled while the program is running.
- **Static analysis**: Code or configuration analysis performed without executing the target program.
- **Static correctness**: Evidence gathered from formatting, linting, typing, or static-analysis tools rather than runtime execution.
- **Structured log**: A log line written in a stable fielded format, such as JSON, rather than only free-form prose.
- **Supply chain**: The external ecosystem of dependencies, registries, build steps, and tooling that a repository relies on.
- **Toolchain**: The compiler, interpreter, package manager, linker, runtime, and related tools required to build or analyze a repository.
- **Transitive dependency**: A dependency that is not declared directly by the repository but is pulled in by some other dependency.
- **Undefined behavior**: Program behavior for which the language or runtime provides no reliable guarantees.
- **Workflow security**: The practice of checking whether CI or automation workflows expose unsafe patterns, permissions, or trust assumptions.

## Testing, Robustness, And Failure-Discovery Terms

- **Benchmark**: A performance-focused executable measurement of a specific operation or workflow.
- **Bounded execution**: A test or fuzz run that is limited in time, iterations, or resources so it can complete predictably.
- **Corpus**: A saved collection of inputs used to seed, replay, or extend fuzzing and regression checks.
- **Crash artifact**: A persisted input or report showing how a tool or system failed.
- **Deterministic**: Producing the same output or decision when run with the same inputs and environment.
- **End-to-end test**: A test that exercises a full user-visible or system-visible workflow across multiple layers.
- **Fuzz smoke**: A short, bounded fuzzing run used as a quick confidence check rather than a deep campaign.
- **Fuzz target**: A specific harness or entrypoint against which a fuzzer generates test inputs.
- **Fuzzer**: A tool that automatically generates many inputs, often malformed or extreme, in order to discover crashes or logic errors.
- **Fuzzing**: The practice of using generated or mutated inputs to probe software for robustness failures.
- **Integration test**: A test that verifies multiple components working together rather than one isolated function.
- **Microbenchmark**: A benchmark that measures a narrowly scoped hot path rather than an entire system workflow.
- **Mutation strength**: The proportion of meaningful code mutations that are caught by the test suite.
- **Mutation testing**: A technique that modifies code in small ways to see whether the test suite notices the behavioral change.
- **Negative test**: A test that verifies rejection, failure handling, or safe behavior under invalid or forbidden input.
- **Replay test**: A test that reuses a previously saved interesting input so the case remains reproducible without rediscovery.
- **Scenario test**: A test organized around a realistic operational sequence rather than a single unit.
- **Shard**: One subdivided slice of a large campaign, such as mutation testing or fuzzing, used to make execution operationally manageable.
- **Sharding**: The act of splitting a large run into multiple smaller runs.
- **Smoke test**: A shallow, fast check that verifies a system can start and perform its most basic expected function.
- **Soak test**: A long-running test intended to reveal leaks, state drift, cumulative latency, or other time-dependent failures.
- **Unit test**: A test focused on one function, method, or small component in isolation.

## Complexity, Maintainability, And Structure Terms

- **Cognitive complexity**: A measure intended to approximate how difficult a function is for a human reader to understand, especially when nested or branch-heavy logic is involved.
- **Cyclomatic complexity**: A measure of branching complexity based on the number of independent paths through a function.
- **Feature matrix**: A set of build or check combinations that verifies code under multiple feature-flag configurations.
- **Formatter**: A tool that rewrites source code or configuration files into a standardized style automatically.
- **Function-level metric**: A complexity or maintainability metric computed for one function rather than for a whole file.
- **Grade band**: A mapping from raw numeric measurements into categories such as A through F.
- **Linter**: A static-analysis tool that flags suspicious patterns, style violations, or maintainability risks.
- **Maintainability**: The degree to which code can be understood, modified, extended, and repaired safely over time.
- **Maintainability Index**: A composite metric that attempts to summarize maintainability from several lower-level measures.
- **Oversized module**: A file or module whose line count exceeds the repository's chosen reviewable-size threshold.
- **Production-test separation**: The architectural practice of keeping runtime code and verification code distinct unless there is a deliberate reason to mix them.
- **Responsibility mixing**: The condition in which one file, function, or module performs too many conceptually separate jobs.

## Performance And Diagnostics Terms

- **Budget**: An explicit numerical limit, such as a latency, memory, or throughput threshold, that the system is expected to stay within.
- **Cross-layer correlation**: The ability to relate client-side, server-side, transport, and host diagnostics to the same event or failure.
- **Debug handoff**: A documented procedure for collecting and sharing the evidence needed to troubleshoot a failure.
- **Draw call**: One request from application code to the graphics system to render something.
- **Frame loop**: The repeatedly executed update and render cycle of an interactive application.
- **Latency**: The delay between a request or action and the observable result.
- **Performance regression**: A measurable slowdown or increase in resource use relative to an earlier accepted baseline.
- **Resource budget**: An explicit cap on memory, CPU, disk, or network usage.
- **Structured diagnostics bundle**: A collected set of logs, metrics, traces, and report files packaged together for troubleshooting.

## Tool And Ecosystem Names Cited In The Documents

- **`cargo-fuzz`**: A Rust-oriented interface for libFuzzer-based fuzzing campaigns.
- **`cargo-llvm-cov`**: A Rust coverage wrapper that uses LLVM-based coverage instrumentation and reporting.
- **`cargo-mutants`**: A Rust mutation-testing tool.
- **`cargo-nextest`**: An alternative Rust test runner optimized for speed and reporting.
- **`criterion`**: A benchmarking library commonly used in Rust for statistically oriented microbenchmarks.
- **`coverage.py`**: A Python coverage measurement tool.
- **`Docusaurus`**: A static documentation-site generator.
- **`gcov`**: A coverage tool traditionally associated with GCC-based C and C++ builds.
- **`GitHub Actions`**: GitHub's hosted CI and automation system.
- **`google-benchmark`**: A C++ benchmarking library.
- **`Jazzer`**: A coverage-guided fuzzer for the JVM ecosystem.
- **`lcov`**: A coverage-reporting tool commonly used with C and C++ coverage data.
- **`libFuzzer`**: A coverage-guided in-process fuzzing engine.
- **`mdBook`**: A documentation generator designed around Markdown content.
- **`Miri`**: A Rust interpreter used to detect certain classes of undefined behavior and memory-model problems.
- **`MkDocs`**: A static-site generator focused on project documentation written in Markdown.
- **`Mull`**: A mutation-testing tool commonly used in C and C++ environments.
- **`mutmut`**: A mutation-testing tool for Python.
- **`nyc`**: A JavaScript and TypeScript coverage tool built on Istanbul.
- **`pre-commit`**: A framework for installing and running repository hooks before commits and other Git events.
- **`pre-push`**: A Git hook stage that runs before a push is sent to a remote.
- **`post-commit`**: A Git hook stage that runs immediately after a commit is created.
- **`pytest-benchmark`**: A benchmarking plugin for Python's `pytest`.
- **`rust-code-analysis-cli`**: A static-analysis tool that emits complexity and maintainability metrics for Rust and other languages.
- **`Sphinx`**: A documentation generator widely used in Python and mixed-language ecosystems.
- **`Stryker`**: A mutation-testing framework used in JavaScript, TypeScript, and some other ecosystems.
- **`Verus`**: A verification-oriented system for specifying and proving properties about Rust-like code.
