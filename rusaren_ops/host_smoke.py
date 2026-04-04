"""Hosted smoke probes for the Rusaren Linux deployment path.

The original shell version was intentionally small and operationally direct.
This Python port keeps the same scope, but makes the logic easier to read and
modify by using named functions instead of chained shell commands.
"""

from __future__ import annotations

import argparse
import os
import re
from pathlib import Path
from typing import Any

from .common import (
    basic_auth_header,
    decode_json_document,
    fatal,
    http_get,
    log,
    resolve_default_config_dir,
    apply_env_file,
)

PREFIX = "host-smoke"


def build_parser() -> argparse.ArgumentParser:
    """Construct the command-line parser for hosted smoke probes."""

    parser = argparse.ArgumentParser(
        prog="host-smoke.py",
        description=(
            "Probe the hosted root, health, bootstrap, and admin routes after deploy."
        ),
    )
    parser.add_argument("--origin", dest="base_url")
    parser.add_argument("--env-file", dest="env_file")
    parser.add_argument(
        "--skip-admin",
        action="store_true",
        help="Skip the authenticated /adminz checks even when credentials exist.",
    )
    return parser


def load_environment(explicit_env_file: str | None) -> Path:
    """Load the deploy env file using the historic external-config convention."""

    env_file = (
        Path(explicit_env_file)
        if explicit_env_file
        else resolve_default_config_dir() / "config.env"
    )
    apply_env_file(env_file)
    return env_file


def resolve_base_url(explicit_base_url: str | None) -> str:
    """Resolve the probe origin with the same precedence as the shell script."""

    if explicit_base_url:
        return explicit_base_url

    public_host = os.environ.get("PUBLIC_HOST", "").strip()
    if public_host:
        return f"https://{public_host}"
    return "http://127.0.0.1:3000"


def require_nested_number(payload: dict[str, Any], path: list[str]) -> None:
    """Assert that a nested JSON path resolves to a numeric value."""

    value: Any = payload
    for key in path:
        if not isinstance(value, dict) or key not in value:
            fatal(PREFIX, f"missing JSON path: {'.'.join(path)}")
        value = value[key]
    if not isinstance(value, (int, float)):
        fatal(PREFIX, f"expected numeric JSON path: {'.'.join(path)}")


def require_nested_list(payload: dict[str, Any], path: list[str]) -> None:
    """Assert that a nested JSON path resolves to a list value."""

    value: Any = payload
    for key in path:
        if not isinstance(value, dict) or key not in value:
            fatal(PREFIX, f"missing JSON path: {'.'.join(path)}")
        value = value[key]
    if not isinstance(value, list):
        fatal(PREFIX, f"expected list JSON path: {'.'.join(path)}")


def assert_html_root(base_url: str) -> None:
    """Verify that the hosted root responds successfully with HTML."""

    response = http_get(f"{base_url}/")
    if response.status_code != 200:
        fatal(PREFIX, f"expected {base_url}/ to return 200, got {response.status_code}")
    if re.search(r"<!doctype html|<html", response.body, flags=re.IGNORECASE) is None:
        fatal(PREFIX, f"expected {base_url}/ to return HTML")
    log(PREFIX, "root page responded with HTML")


def assert_healthz(base_url: str) -> None:
    """Verify that `/healthz` returns the expected plain-text sentinel."""

    response = http_get(f"{base_url}/healthz", timeout_seconds=10.0)
    if response.status_code != 200:
        fatal(
            PREFIX,
            f"expected {base_url}/healthz to return 200, got {response.status_code}",
        )
    if response.body != "ok":
        fatal(PREFIX, f"expected {base_url}/healthz to return ok, got {response.body!r}")
    log(PREFIX, "healthz responded with ok")


def assert_session_bootstrap(base_url: str) -> None:
    """Verify that the anonymous bootstrap route still mints usable tokens."""

    response = http_get(f"{base_url}/session/bootstrap", timeout_seconds=10.0)
    if response.status_code != 200:
        fatal(
            PREFIX,
            (
                f"expected {base_url}/session/bootstrap to return 200, "
                f"got {response.status_code}"
            ),
        )
    payload = decode_json_document(response.body)
    token = payload.get("token", "")
    expires_in_ms = payload.get("expires_in_ms")
    if not isinstance(token, str) or not token:
        fatal(PREFIX, "session bootstrap response did not contain a token")
    if not isinstance(expires_in_ms, (int, float)) or expires_in_ms <= 0:
        fatal(
            PREFIX,
            "session bootstrap response did not contain a positive expires_in_ms",
        )
    log(PREFIX, "session bootstrap minted a token")


def assert_admin_dashboard(base_url: str, *, skip_admin: bool) -> None:
    """Verify that `/adminz` still enforces auth and renders correctly."""

    if skip_admin:
        log(PREFIX, "skipping admin dashboard probe by request")
        return

    username = os.environ.get("RARENA_ADMIN_USERNAME", "").strip()
    password = os.environ.get("RARENA_ADMIN_PASSWORD", "")
    if not username or not password:
        log(
            PREFIX,
            "skipping admin dashboard probe because admin credentials are not configured",
        )
        return

    unauthenticated = http_get(f"{base_url}/adminz")
    if unauthenticated.status_code != 401:
        fatal(
            PREFIX,
            (
                f"expected unauthenticated {base_url}/adminz to return 401, "
                f"got {unauthenticated.status_code}"
            ),
        )

    auth_headers = {"Authorization": basic_auth_header(username, password)}
    authenticated = http_get(f"{base_url}/adminz", headers=auth_headers)
    if authenticated.status_code != 200:
        fatal(
            PREFIX,
            (
                f"expected authenticated {base_url}/adminz to return 200, "
                f"got {authenticated.status_code}"
            ),
        )
    if "Rusaren Admin Dashboard" not in authenticated.body:
        fatal(
            PREFIX,
            f"expected authenticated {base_url}/adminz to render the admin dashboard",
        )

    json_response = http_get(
        f"{base_url}/adminz?format=json",
        headers=auth_headers,
    )
    if json_response.status_code != 200:
        fatal(
            PREFIX,
            (
                f"expected authenticated {base_url}/adminz?format=json to return 200, "
                f"got {json_response.status_code}"
            ),
        )

    payload = decode_json_document(json_response.body)
    require_nested_number(payload, ["runtime", "connected_players"])
    require_nested_number(payload, ["app_diagnostics", "combat_log", "append", "p99_ms"])
    require_nested_list(payload, ["recent_matches"])
    log(PREFIX, "admin dashboard requires auth and renders successfully in HTML and JSON")


def main(argv: list[str] | None = None) -> int:
    """Run the hosted smoke suite."""

    args = build_parser().parse_args(argv)
    load_environment(args.env_file)
    base_url = resolve_base_url(args.base_url)

    log(PREFIX, f"probing {base_url}")
    assert_html_root(base_url)
    assert_healthz(base_url)
    assert_session_bootstrap(base_url)
    assert_admin_dashboard(base_url, skip_admin=args.skip_admin)
    log(PREFIX, "all smoke probes passed")
    return 0
