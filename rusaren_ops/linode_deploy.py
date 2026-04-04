"""Idempotent deployment runner for an existing Linux host."""

from __future__ import annotations

import argparse
import os
import sys
import time
from pathlib import Path

from . import host_smoke
from .common import (
    copy_text,
    current_user,
    ensure_directory,
    ensure_owner,
    fatal,
    resolve_runtime_user,
    run,
    user_home,
)

PREFIX = "linode-deploy"


def build_parser() -> argparse.ArgumentParser:
    """Construct the argument parser for idempotent Linux deploys."""

    parser = argparse.ArgumentParser(
        prog="deploy.py",
        description=(
            "Deploy or stop the Rusaren hosted stack using ~/rusaren-config by default."
        ),
    )
    parser.add_argument("--down", action="store_true", help="Stop the hosted stack.")
    parser.add_argument("--config-dir")
    parser.add_argument("--env-file")
    parser.add_argument("--compose-override")
    return parser


def resolve_runtime_home(repo_root: Path) -> Path:
    """Resolve the home directory for the runtime/deploy user."""

    return user_home(resolve_runtime_user(repo_root))


def resolve_config_dir(repo_root: Path, explicit_path: str | None) -> Path:
    """Resolve the external config directory used for deploy state."""

    if explicit_path:
        return Path(explicit_path)
    return resolve_runtime_home(repo_root) / "rusaren-config"


def resolve_export_user(repo_root: Path) -> str:
    """Resolve the user that should build the Godot web export on-host."""

    explicit = os.environ.get("DEPLOY_EXPORT_USER", "").strip()
    if explicit:
        return explicit
    return resolve_runtime_user(repo_root)


def compose_prefix(
    repo_root: Path,
    *,
    env_file: Path,
    compose_override_file: Path | None,
) -> list[str]:
    """Build the stable docker compose argument prefix."""

    command = [
        "docker",
        "compose",
        "--env-file",
        str(env_file),
        "-f",
        str(repo_root / "deploy" / "docker-compose.yml"),
    ]
    if compose_override_file is not None and compose_override_file.is_file():
        command.extend(["-f", str(compose_override_file)])
    return command


def run_compose(
    repo_root: Path,
    *,
    env_file: Path,
    compose_override_file: Path | None,
    args: list[str],
    capture_output: bool = False,
    check: bool = True,
):
    """Execute one docker compose command inside the repo root."""

    return run(
        compose_prefix(
            repo_root,
            env_file=env_file,
            compose_override_file=compose_override_file,
        )
        + args,
        cwd=repo_root,
        capture_output=capture_output,
        check=check,
    )


def ensure_env_file(repo_root: Path, env_file: Path) -> None:
    """Create a starter env file on first run, then stop for operator edits."""

    default_example = repo_root / "deploy" / "config.env.example"
    legacy_example = repo_root / "deploy" / ".env.example"
    if env_file.is_file():
        return

    ensure_directory(env_file.parent)
    if default_example.is_file():
        copy_text(default_example, env_file)
    elif legacy_example.is_file():
        copy_text(legacy_example, env_file)
    else:
        fatal(PREFIX, "no deploy env example file was found")

    fatal(PREFIX, f"created {env_file}; set real values first, then rerun this script")


def ensure_static_root(repo_root: Path) -> None:
    """Create the static web root and hand ownership to the export user when root."""

    static_root = repo_root / "server" / "static" / "webclient"
    ensure_directory(static_root)
    if os.geteuid() == 0:
        ensure_owner(static_root, resolve_export_user(repo_root))


def run_web_client_export(repo_root: Path) -> None:
    """Run the Linux Godot export helper as the intended export user."""

    export_script = repo_root / "server" / "scripts" / "export-web-client.py"
    export_user = resolve_export_user(repo_root)
    export_home = user_home(export_user)

    export_command = [sys.executable, str(export_script)]
    godot_bin = os.environ.get("GODOT_BIN", "").strip()
    if godot_bin:
        export_command.extend(["--godot-bin", godot_bin])

    print(f"[linode-deploy] building the Godot web client on the host as {export_user}")

    if export_user == current_user():
        run(export_command, env={"HOME": str(export_home)})
        return

    command = [
        "runuser",
        "-u",
        export_user,
        "--",
        "env",
        f"HOME={export_home}",
    ]
    if godot_bin:
        command.append(f"GODOT_BIN={godot_bin}")
    command.extend([sys.executable, str(export_script)])
    run(command)


def build_web_client_if_requested(repo_root: Path) -> None:
    """Conditionally rebuild the hosted Godot web bundle before deploy."""

    mode = os.environ.get("BUILD_WEB_CLIENT", "1").strip().lower()
    index_path = repo_root / "server" / "static" / "webclient" / "index.html"
    should_build = False

    if mode in {"1", "true", "always"}:
        should_build = True
    elif mode == "auto":
        should_build = not index_path.is_file()
    elif mode in {"0", "false", "never"}:
        if not index_path.is_file():
            print(
                "[linode-deploy] no exported web bundle detected at "
                "server/static/webclient; deploy will continue and the backend "
                "will serve the placeholder root page"
            )
        return
    else:
        fatal(PREFIX, f"invalid BUILD_WEB_CLIENT value: {mode}")

    if should_build:
        run_web_client_export(repo_root)


def wait_for_healthz(
    repo_root: Path,
    *,
    env_file: Path,
    compose_override_file: Path | None,
) -> None:
    """Wait for the backend container to become healthy or at least running."""

    for _attempt in range(60):
        result = run_compose(
            repo_root,
            env_file=env_file,
            compose_override_file=compose_override_file,
            args=["ps", "-q", "rarena-server"],
            capture_output=True,
            check=False,
        )
        container_id = (result.stdout or "").strip()
        if container_id:
            inspect = run(
                [
                    "docker",
                    "inspect",
                    "--format",
                    "{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}",
                    container_id,
                ],
                capture_output=True,
                check=False,
            )
            health_status = (inspect.stdout or "").strip()
            if health_status in {"healthy", "running"}:
                print("[linode-deploy] backend container health check passed")
                return
        time.sleep(2)

    fatal(PREFIX, "backend container did not become healthy")


def main(argv: list[str] | None = None, *, repo_root: Path | None = None) -> int:
    """Run the deploy or stop path for the hosted compose stack."""

    args = build_parser().parse_args(argv)
    resolved_repo_root = (
        repo_root if repo_root is not None else Path(__file__).resolve().parents[1]
    )

    compose_file = resolved_repo_root / "deploy" / "docker-compose.yml"
    smoke_script = resolved_repo_root / "deploy" / "host-smoke.py"
    export_script = resolved_repo_root / "server" / "scripts" / "export-web-client.py"
    if not compose_file.is_file():
        fatal(PREFIX, f"missing required file: {compose_file}")
    if not smoke_script.is_file():
        fatal(PREFIX, f"missing required file: {smoke_script}")
    if not export_script.is_file():
        fatal(PREFIX, f"missing required file: {export_script}")

    config_dir = resolve_config_dir(resolved_repo_root, args.config_dir)
    env_file = Path(args.env_file) if args.env_file else config_dir / "config.env"
    compose_override_file = (
        Path(args.compose_override)
        if args.compose_override
        else config_dir / "docker-compose.override.yml"
    )

    ensure_env_file(resolved_repo_root, env_file)

    if args.down:
        print("[linode-deploy] stopping the production stack")
        run_compose(
            resolved_repo_root,
            env_file=env_file,
            compose_override_file=compose_override_file,
            args=["down"],
        )
        return 0

    ensure_static_root(resolved_repo_root)
    build_web_client_if_requested(resolved_repo_root)

    print("[linode-deploy] validating compose configuration")
    run_compose(
        resolved_repo_root,
        env_file=env_file,
        compose_override_file=compose_override_file,
        args=["config", "-q"],
    )

    print("[linode-deploy] building and starting the production stack")
    run_compose(
        resolved_repo_root,
        env_file=env_file,
        compose_override_file=compose_override_file,
        args=["build", "--pull"],
    )
    run_compose(
        resolved_repo_root,
        env_file=env_file,
        compose_override_file=compose_override_file,
        args=["up", "-d", "--remove-orphans"],
    )

    wait_for_healthz(
        resolved_repo_root,
        env_file=env_file,
        compose_override_file=compose_override_file,
    )

    if os.environ.get("RUN_PUBLIC_SMOKE", "1") == "1":
        print("[linode-deploy] running hosted smoke probes")
        host_smoke.main(["--env-file", str(env_file)])
    else:
        print(
            "[linode-deploy] skipping hosted smoke probes because "
            f"RUN_PUBLIC_SMOKE={os.environ.get('RUN_PUBLIC_SMOKE', '1')}"
        )

    print("[linode-deploy] current service status")
    result = run_compose(
        resolved_repo_root,
        env_file=env_file,
        compose_override_file=compose_override_file,
        args=["ps"],
        capture_output=True,
    )
    print(result.stdout or "", end="")
    return 0
