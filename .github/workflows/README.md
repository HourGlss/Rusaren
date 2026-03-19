# workflows

This directory contains GitHub Actions workflows that build, test, fuzz, publish reports, and smoke-check deployments.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `deploy-stack-smoke.yml`: GitHub Actions workflow that smoke-tests the deployment stack path.
- `godot-web-smoke.yml`: GitHub Actions workflow that exercises the Godot web export path.
- `server-advanced-quality.yml`: GitHub Actions workflow for heavier scheduled or advanced backend quality checks.
- `server-quality.yml`: Main GitHub Actions workflow for build, lint, test, report, and publish tasks.
