.PHONY: all fmt lint hack test soak doc fuzz fuzz-build fuzz-live mutants verus reports deny audit

all:
	pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/quality.ps1 all

fmt:
	pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/quality.ps1 fmt

lint:
	pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/quality.ps1 lint

hack:
	pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/quality.ps1 hack

test:
	pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/quality.ps1 test

soak:
	pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/quality.ps1 soak

doc:
	pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/quality.ps1 doc

fuzz:
	pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/quality.ps1 fuzz

fuzz-build:
	pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/quality.ps1 fuzz-build

fuzz-live:
	pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/quality.ps1 fuzz-live

mutants:
	pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/quality.ps1 mutants

verus:
	pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/quality.ps1 verus

reports:
	pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/quality.ps1 reports

deny:
	pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/quality.ps1 deny

audit:
	pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/quality.ps1 audit
