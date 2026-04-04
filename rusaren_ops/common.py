"""Shared operational helpers for Rusaren Python entrypoints.

This module is intentionally standard-library-only so the Linux host setup
path does not depend on a virtual environment or any extra package install.
"""

from __future__ import annotations

import base64
import getpass
import json
import os
import shlex
import ssl
import subprocess
import sys
from datetime import datetime, timezone
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Mapping, Sequence

try:
    import pwd
except ImportError:  # pragma: no cover - only exercised on non-POSIX platforms
    pwd = None


@dataclass(frozen=True)
class HttpResponse:
    """Small response container used by the operational HTTP probes."""

    status_code: int
    body: str
    headers: Mapping[str, str]


class CommandFailure(RuntimeError):
    """Raised when a subprocess exits non-zero and the caller asked for checks."""


def log(prefix: str, message: str) -> None:
    """Emit one consistently-prefixed operational log line."""

    print(f"[{prefix}] {message}")


def fatal(prefix: str, message: str) -> "NoReturn":
    """Abort execution with a consistently-prefixed error message."""

    raise SystemExit(f"[{prefix}] ERROR: {message}")


def repo_root_from(script_path: Path, parents_up: int) -> Path:
    """Resolve the repository root from a known script location.

    The operational entrypoints live in two places:
    - `deploy/*.py`           -> repo root is `parents[1]`
    - `server/scripts/*.py`   -> repo root is `parents[2]`
    """

    return script_path.resolve().parents[parents_up]


def parse_env_file(path: Path) -> dict[str, str]:
    """Parse a simple `KEY=value` env file into a dictionary.

    The checked-in and external deploy env files are intentionally simple.
    We support comments, blank lines, optional `export ` prefixes, and
    matching single- or double-quoted values.
    """

    values: dict[str, str] = {}
    if not path.is_file():
        return values

    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("export "):
            line = line[len("export ") :].strip()
        if "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip()
        if len(value) >= 2 and value[0] == value[-1] and value[0] in {"'", '"'}:
            value = value[1:-1]
        values[key] = value
    return values


def apply_env_file(path: Path, *, overwrite_existing: bool = False) -> dict[str, str]:
    """Load values from an env file into `os.environ`.

    We preserve process-level overrides by default so operators can still
    override a config file from the command line when needed.
    """

    values = parse_env_file(path)
    for key, value in values.items():
        if overwrite_existing or key not in os.environ:
            os.environ[key] = value
    return values


def current_user() -> str:
    """Return the current effective user name."""

    if pwd is not None and hasattr(os, "geteuid"):
        return pwd.getpwuid(os.geteuid()).pw_name
    return getpass.getuser()


def user_home(username: str) -> Path:
    """Resolve a user's home directory via the passwd database."""

    if pwd is not None:
        return Path(pwd.getpwnam(username).pw_dir)
    if username == current_user():
        return Path.home()
    return Path(os.path.expanduser(f"~{username}"))


def effective_uid() -> int | None:
    """Return the current effective UID when the platform exposes one."""

    getter = getattr(os, "geteuid", None)
    if getter is None:
        return None
    return getter()


def resolve_runtime_user(repo_root: Path) -> str:
    """Pick the non-root deploy/runtime user using the same precedence as before."""

    explicit = os.environ.get("DEPLOY_RUNTIME_USER")
    if explicit:
        return explicit

    if effective_uid() == 0:
        sudo_user = os.environ.get("SUDO_USER")
        if sudo_user and sudo_user != "root":
            return sudo_user

        # When the process runs as root without sudo, prefer the repository owner
        # so deploys keep using the checked-out user's home/config area.
        owner_name = ""
        if pwd is not None:
            try:
                owner_name = pwd.getpwuid(repo_root.stat().st_uid).pw_name
            except KeyError:
                owner_name = ""
        if owner_name and owner_name != "root":
            return owner_name

    return current_user()


def resolve_default_config_dir() -> Path:
    """Resolve `~/rusaren-config` for the active deploy/runtime user."""

    explicit = os.environ.get("CONFIG_DIR")
    if explicit:
        return Path(explicit)

    if effective_uid() == 0:
        sudo_user = os.environ.get("SUDO_USER")
        if sudo_user and sudo_user != "root":
            return user_home(sudo_user) / "rusaren-config"

    return Path.home() / "rusaren-config"


def ensure_directory(path: Path, *, mode: int | None = None) -> None:
    """Create a directory tree and optionally enforce a POSIX mode."""

    path.mkdir(parents=True, exist_ok=True)
    if mode is not None:
        path.chmod(mode)


def ensure_owner(path: Path, username: str) -> None:
    """Recursively apply user/group ownership when running as root."""

    if effective_uid() != 0 or pwd is None:
        return

    user_entry = pwd.getpwnam(username)
    target_uid = user_entry.pw_uid
    target_gid = user_entry.pw_gid

    if path.is_file():
        os.chown(path, target_uid, target_gid)
        return

    # We explicitly walk the tree so host bootstrap can fix ownership for the
    # external config, cargo cache, cargo target, and probe directories.
    for root, dirnames, filenames in os.walk(path):
        os.chown(root, target_uid, target_gid)
        for dirname in dirnames:
            os.chown(Path(root, dirname), target_uid, target_gid)
        for filename in filenames:
            os.chown(Path(root, filename), target_uid, target_gid)


def shell_join(argv: Sequence[str]) -> str:
    """Render a subprocess argv list in a copy-pasteable shell form."""

    return shlex.join(list(argv))


def run(
    argv: Sequence[str],
    *,
    cwd: Path | None = None,
    env: Mapping[str, str] | None = None,
    check: bool = True,
    capture_output: bool = False,
) -> subprocess.CompletedProcess[str]:
    """Run a subprocess with text-mode I/O.

    The deploy scripts are operational tooling, so we prefer clear process
    boundaries over shell-string interpolation. Every script calls this helper
    instead of composing large shell fragments.
    """

    merged_env = None
    if env is not None:
        merged_env = os.environ.copy()
        merged_env.update(env)

    result = subprocess.run(
        list(argv),
        cwd=str(cwd) if cwd is not None else None,
        env=merged_env,
        text=True,
        capture_output=capture_output,
        check=False,
    )
    if check and result.returncode != 0:
        command = shell_join(argv)
        stderr = (result.stderr or "").strip()
        stdout = (result.stdout or "").strip()
        detail = stderr or stdout or f"exit code {result.returncode}"
        raise CommandFailure(f"{command} failed: {detail}")
    return result


def http_get(
    url: str,
    *,
    headers: Mapping[str, str] | None = None,
    timeout_seconds: float = 15.0,
    insecure_tls: bool = False,
) -> HttpResponse:
    """Issue a small GET request for smoke checks and diagnostics collection."""

    request = urllib.request.Request(url, method="GET")
    for name, value in (headers or {}).items():
        request.add_header(name, value)

    # Diagnostics tooling often needs to keep gathering evidence even when the
    # remote TLS chain is misconfigured. The caller explicitly opts into that.
    ssl_context = None
    if insecure_tls and url.startswith("https://"):
        ssl_context = ssl._create_unverified_context()

    try:
        with urllib.request.urlopen(
            request,
            timeout=timeout_seconds,
            context=ssl_context,
        ) as response:
            body = response.read().decode("utf-8", errors="replace")
            headers_dict = {key: value for key, value in response.headers.items()}
            return HttpResponse(
                status_code=int(response.status),
                body=body,
                headers=headers_dict,
            )
    except urllib.error.HTTPError as error:
        body = error.read().decode("utf-8", errors="replace")
        headers_dict = {key: value for key, value in error.headers.items()}
        return HttpResponse(
            status_code=int(error.code),
            body=body,
            headers=headers_dict,
        )


def basic_auth_header(username: str, password: str) -> str:
    """Build a Basic-auth header value for admin probes."""

    encoded = base64.b64encode(f"{username}:{password}".encode("utf-8")).decode("ascii")
    return f"Basic {encoded}"


def decode_json_document(text: str) -> dict:
    """Decode JSON text into a dictionary with a consistent failure surface."""

    payload = json.loads(text)
    if not isinstance(payload, dict):
        raise ValueError("expected a top-level JSON object")
    return payload


def copy_text(src: Path, dest: Path) -> None:
    """Copy a text file while ensuring the destination directory exists."""

    ensure_directory(dest.parent)
    dest.write_text(src.read_text(encoding="utf-8"), encoding="utf-8")


def first_existing_path(paths: Iterable[Path]) -> Path | None:
    """Return the first existing path from a candidate list."""

    for path in paths:
        if path.exists():
            return path
    return None


def append_repo_root_to_path(repo_root: Path) -> None:
    """Prepend the repository root to `sys.path` for script wrappers."""

    repo_root_text = str(repo_root)
    if repo_root_text not in sys.path:
        sys.path.insert(0, repo_root_text)


def utc_timestamp() -> str:
    """Return an ISO-like UTC timestamp used by diagnostics artifacts."""

    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def compact_utc_timestamp() -> str:
    """Return a filesystem-safe UTC timestamp used in probe filenames."""

    return datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
