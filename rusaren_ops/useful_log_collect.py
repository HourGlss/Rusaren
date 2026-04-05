"""Host-side diagnostics collector for the Linux deployment path.

The goal of this module is to preserve the operator-friendly output shape from
the shell version while making each report section inspectable and testable.
"""

from __future__ import annotations

import argparse
import os
import re
import socket
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .common import (
    apply_env_file,
    basic_auth_header,
    decode_json_document,
    ensure_directory,
    http_get,
    log,
    resolve_default_config_dir,
    run,
    utc_timestamp,
)

PREFIX = "useful-log-collect"
FILTER_PATTERN = re.compile(
    r"warn|error|disconnect|rejection|reject|webrtc|websocket|bootstrap|ingress|peer connection|ice|turn|failed|close",
    flags=re.IGNORECASE,
)


@dataclass(frozen=True)
class CollectorConfig:
    """Resolved operational settings for diagnostics collection."""

    repo_root: Path
    env_file: Path
    compose_override_file: Path | None
    base_url: str
    since: str
    tail: str
    output_file: Path | None
    bundle_dir: Path | None

    @property
    def compose_command_prefix(self) -> list[str]:
        """Build the docker compose prefix once so every call stays consistent."""

        command = [
            "docker",
            "compose",
            "--env-file",
            str(self.env_file),
            "-f",
            str(self.repo_root / "deploy" / "docker-compose.yml"),
        ]
        if self.compose_override_file is not None:
            command.extend(["-f", str(self.compose_override_file)])
        return command


def build_parser() -> argparse.ArgumentParser:
    """Construct the CLI parser for diagnostics collection."""

    parser = argparse.ArgumentParser(
        prog="python -m rusaren_ops collect-logs",
        description=(
            "Collect a compact hosted-backend diagnostics report intended for copy/paste."
        ),
    )
    parser.add_argument("--origin", dest="base_url")
    parser.add_argument("--env-file", dest="env_file")
    parser.add_argument("--since", default=os.environ.get("SINCE", "20m"))
    parser.add_argument("--tail", default=os.environ.get("TAIL", "200"))
    parser.add_argument("--output", dest="output_file")
    parser.add_argument("--bundle-dir", dest="bundle_dir")
    return parser


def resolve_base_url(explicit_base_url: str | None) -> str:
    """Resolve the public origin from args, env, or the local fallback."""

    if explicit_base_url:
        return explicit_base_url
    public_host = os.environ.get("PUBLIC_HOST", "").strip()
    if public_host:
        return f"https://{public_host}"
    return "http://127.0.0.1:3000"


def safe_http_get(
    url: str,
    *,
    headers: dict[str, str] | None = None,
    timeout_seconds: float = 15.0,
    insecure_tls: bool = True,
) -> tuple[int | str, str]:
    """Perform a GET request while downgrading network failures into text."""

    try:
        response = http_get(
            url,
            headers=headers,
            timeout_seconds=timeout_seconds,
            insecure_tls=insecure_tls,
        )
        return response.status_code, response.body
    except Exception as exc:  # noqa: BLE001 - diagnostics should continue collecting
        return "unavailable", str(exc)


def run_capture(argv: list[str], *, cwd: Path | None = None) -> str:
    """Run a subprocess and return combined stdout/stderr without failing hard."""

    result = run(argv, cwd=cwd, capture_output=True, check=False)
    return ((result.stdout or "") + (result.stderr or "")).strip()


def compose_capture(config: CollectorConfig, *args: str) -> str:
    """Run one docker compose command and return its text output."""

    return run_capture(config.compose_command_prefix + list(args), cwd=config.repo_root)


def print_section(name: str) -> str:
    """Render a section header using the historic output style."""

    return f"\n=== {name} ===\n"


def public_probe_summary(base_url: str) -> str:
    """Summarize the public HTTP probes used in hosted diagnostics handoff."""

    lines: list[str] = []
    status, _body = safe_http_get(f"{base_url}/")
    lines.append(f"root_status: {status}")

    status, body = safe_http_get(f"{base_url}/healthz")
    lines.append(f"healthz_status: {status}")
    if isinstance(body, str) and body:
        lines.append(f"healthz_body: {body.replace(chr(13), '').replace(chr(10), '')}")

    status, body = safe_http_get(f"{base_url}/session/bootstrap")
    lines.append(f"session_bootstrap_status: {status}")
    if isinstance(body, str) and body:
        try:
            payload = decode_json_document(body)
            token = payload.get("token", "")
            lines.append(
                "session_bootstrap_token_present: "
                + ("yes" if isinstance(token, str) and len(token) > 0 else "no")
            )
            lines.append(
                f"session_bootstrap_expires_in_ms: {payload.get('expires_in_ms')}"
            )
        except Exception as exc:  # noqa: BLE001 - report parse failures instead of aborting
            lines.append(f"session_bootstrap_parse_error: {exc}")

    return "\n".join(lines)


def emit_runtime_scalars(runtime: dict[str, Any], lines: list[str]) -> None:
    """Emit the scalar runtime counters while skipping the bulky list fields."""

    for key, value in runtime.items():
        if isinstance(value, list):
            continue
        lines.append(f"admin_{key}: {value}")


def emit_event_list(
    heading: str,
    events: list[dict[str, Any]],
    lines: list[str],
    *,
    elapsed_key: str,
) -> None:
    """Emit a compact bullet list for repeated diagnostic/admin event shapes."""

    if not events:
        lines.append(f"{heading}: none")
        return

    lines.append(f"{heading}:")
    for event in events[:25]:
        lines.append(
            (
                f"  - {elapsed_key}={event.get(elapsed_key)} "
                f"category={event.get('category')} "
                f"connection={event.get('connection_id')} "
                f"player={event.get('player_id')} "
                f"detail={event.get('detail')}"
            )
        )


def admin_summary(base_url: str) -> str:
    """Fetch `/adminz?format=json` and reduce it to the most useful fields."""

    username = os.environ.get("RARENA_ADMIN_USERNAME", "").strip()
    password = os.environ.get("RARENA_ADMIN_PASSWORD", "")
    if not username or not password:
        return "adminz: skipped (credentials not configured)"

    status, body = safe_http_get(
        f"{base_url}/adminz?format=json",
        headers={"Authorization": basic_auth_header(username, password)},
        timeout_seconds=20.0,
    )
    lines = [f"adminz_status: {status}"]
    if status != 200 or not isinstance(body, str):
        return "\n".join(lines)

    try:
        payload = decode_json_document(body)
    except Exception as exc:  # noqa: BLE001 - diagnostics should remain printable
        lines.append(f"adminz_parse_error: {exc}")
        return "\n".join(lines)

    runtime = payload.get("runtime", {})
    if isinstance(runtime, dict):
        emit_runtime_scalars(runtime, lines)
        emit_event_list(
            "admin_recent_errors",
            runtime.get("recent_errors", []) if isinstance(runtime.get("recent_errors"), list) else [],
            lines,
            elapsed_key="elapsed_ms",
        )
        emit_event_list(
            "admin_recent_diagnostics",
            runtime.get("recent_diagnostics", [])
            if isinstance(runtime.get("recent_diagnostics"), list)
            else [],
            lines,
            elapsed_key="elapsed_ms",
        )

    recent_matches = payload.get("recent_matches", [])
    if isinstance(recent_matches, list) and recent_matches:
        lines.append("admin_recent_matches:")
        for summary in recent_matches[:10]:
            if not isinstance(summary, dict):
                continue
            lines.append(
                (
                    f"  - match_id={summary.get('match_id')} "
                    f"event_count={summary.get('event_count')} "
                    f"last_round={summary.get('last_round')} "
                    f"last_phase={summary.get('last_phase')} "
                    f"last_event={summary.get('last_event_kind')}"
                )
            )
    else:
        lines.append("admin_recent_matches: none")

    selected_match_log = payload.get("selected_match_log")
    if isinstance(selected_match_log, dict):
        summary = selected_match_log.get("summary", {})
        entries = selected_match_log.get("entries", [])
        lines.append(
            (
                f"admin_selected_match_log: match_id={summary.get('match_id')} "
                f"events={summary.get('event_count')} "
                f"recent_entries={len(entries) if isinstance(entries, list) else 0}"
            )
        )
    else:
        lines.append("admin_selected_match_log: none")

    return "\n".join(lines)


def filtered_logs_text(config: CollectorConfig) -> str:
    """Collect recent transport-related logs and keep only the diagnostic lines."""

    text = compose_capture(
        config,
        "logs",
        "--no-color",
        "--since",
        config.since,
        "--tail",
        str(config.tail),
        "rarena-server",
        "caddy",
        "coturn",
    )
    filtered = [line for line in text.splitlines() if FILTER_PATTERN.search(line)]
    if not filtered:
        return "(no matching log lines found in the selected window)"
    return "\n".join(filtered)


def host_summary_text() -> str:
    """Emit a compact host resource summary for copy/paste diagnostics."""

    lines = [
        f"time_utc: {utc_timestamp()}",
        f"host: {socket.gethostname()}",
    ]
    loadavg_path = Path("/proc/loadavg")
    if loadavg_path.is_file():
        lines.append(f"loadavg: {loadavg_path.read_text(encoding='utf-8').strip()}")

    for label, argv in (
        ("uptime", ["uptime"]),
        ("mem", ["free", "-h"]),
        ("df_root", ["df", "-h", "/"]),
    ):
        output = run_capture(argv)
        if not output:
            continue
        if label == "uptime":
            lines.append(f"uptime: {output}")
        else:
            lines.append(f"\n[{label}]")
            lines.append(output)
    return "\n".join(lines)


def docker_stats_text() -> str:
    """Fetch one `docker stats --no-stream` snapshot."""

    return run_capture(
        [
            "docker",
            "stats",
            "--no-stream",
            "--format",
            "table {{.Name}}\t{{.CPUPerc}}\t{{.MemUsage}}\t{{.NetIO}}\t{{.BlockIO}}",
        ]
    )


def fetch_admin_json(base_url: str) -> str:
    """Fetch the raw admin JSON payload when credentials are configured."""

    username = os.environ.get("RARENA_ADMIN_USERNAME", "").strip()
    password = os.environ.get("RARENA_ADMIN_PASSWORD", "")
    if not username or not password:
        return ""

    _status, body = safe_http_get(
        f"{base_url}/adminz?format=json",
        headers={"Authorization": basic_auth_header(username, password)},
        timeout_seconds=20.0,
    )
    return body if isinstance(body, str) else ""


def build_report(config: CollectorConfig) -> str:
    """Assemble the complete multi-section diagnostics report."""

    lines: list[str] = []
    lines.append(print_section("Context").rstrip())
    lines.extend(
        [
            f"time_utc: {utc_timestamp()}",
            f"host: {socket.gethostname()}",
            f"repo_head: {run_capture(['git', '-C', str(config.repo_root), 'rev-parse', '--short', 'HEAD']) or 'unavailable'}",
            f"base_url: {config.base_url}",
            f"env_file: {config.env_file}",
            (
                f"compose_override: {config.compose_override_file}"
                if config.compose_override_file is not None
                else "compose_override: none"
            ),
            f"public_host: {os.environ.get('PUBLIC_HOST', 'unset')}",
            f"turn_public_host: {os.environ.get('TURN_PUBLIC_HOST', 'unset')}",
            f"turn_external_ip: {os.environ.get('TURN_EXTERNAL_IP', 'unset')}",
            f"rust_log: {os.environ.get('RARENA_RUST_LOG', 'unset')}",
            f"log_format: {os.environ.get('RARENA_LOG_FORMAT', 'unset')}",
        ]
    )
    lines.append(print_section("Docker Compose PS").rstrip())
    lines.append(compose_capture(config, "ps"))
    lines.append(print_section("Public Probes").rstrip())
    lines.append(public_probe_summary(config.base_url))
    lines.append(print_section("Admin Summary").rstrip())
    lines.append(admin_summary(config.base_url))
    lines.append(print_section("Filtered Logs").rstrip())
    lines.append(filtered_logs_text(config))
    return "\n".join(lines).rstrip() + "\n"


def write_bundle_artifacts(config: CollectorConfig, report_text: str) -> None:
    """Write the structured diagnostics bundle requested by the hosted runbooks."""

    if config.bundle_dir is None:
        return

    ensure_directory(config.bundle_dir)
    (config.bundle_dir / "summary.txt").write_text(report_text, encoding="utf-8")
    (config.bundle_dir / "docker-compose-ps.txt").write_text(
        compose_capture(config, "ps"),
        encoding="utf-8",
    )
    (config.bundle_dir / "public-probes.txt").write_text(
        public_probe_summary(config.base_url),
        encoding="utf-8",
    )
    (config.bundle_dir / "admin-summary.txt").write_text(
        admin_summary(config.base_url),
        encoding="utf-8",
    )
    (config.bundle_dir / "adminz.json").write_text(
        fetch_admin_json(config.base_url),
        encoding="utf-8",
    )
    metrics_status, metrics_body = safe_http_get(
        f"{config.base_url}/metrics",
        timeout_seconds=20.0,
    )
    metrics_text = metrics_body if metrics_status == 200 and isinstance(metrics_body, str) else ""
    (config.bundle_dir / "metrics.prom").write_text(metrics_text, encoding="utf-8")
    (config.bundle_dir / "filtered-logs.txt").write_text(
        filtered_logs_text(config),
        encoding="utf-8",
    )
    (config.bundle_dir / "host.txt").write_text(host_summary_text(), encoding="utf-8")
    (config.bundle_dir / "docker-stats.txt").write_text(
        docker_stats_text(),
        encoding="utf-8",
    )


def resolve_config(args: argparse.Namespace, *, repo_root: Path) -> CollectorConfig:
    """Resolve external config and docker compose paths using deploy defaults."""

    config_dir = resolve_default_config_dir()
    env_file = Path(args.env_file) if args.env_file else config_dir / "config.env"
    apply_env_file(env_file)

    override_env = os.environ.get("COMPOSE_OVERRIDE_FILE", "").strip()
    compose_override = (
        Path(override_env)
        if override_env
        else config_dir / "docker-compose.override.yml"
    )
    if not compose_override.is_file():
        compose_override = None

    return CollectorConfig(
        repo_root=repo_root,
        env_file=env_file,
        compose_override_file=compose_override,
        base_url=resolve_base_url(args.base_url),
        since=str(args.since),
        tail=str(args.tail),
        output_file=Path(args.output_file) if args.output_file else None,
        bundle_dir=Path(args.bundle_dir) if args.bundle_dir else None,
    )


def main(argv: list[str] | None = None, *, repo_root: Path | None = None) -> int:
    """Run the diagnostics collector and optionally write bundle artifacts."""

    args = build_parser().parse_args(argv)
    resolved_repo_root = (
        repo_root if repo_root is not None else Path(__file__).resolve().parents[1]
    )
    config = resolve_config(args, repo_root=resolved_repo_root)
    report_text = build_report(config)

    if config.output_file is not None:
        ensure_directory(config.output_file.parent)
        config.output_file.write_text(report_text, encoding="utf-8")
    print(report_text, end="")

    if config.bundle_dir is not None:
        write_bundle_artifacts(config, report_text)
        log(PREFIX, f"wrote diagnostics bundle to {config.bundle_dir}")

    return 0
