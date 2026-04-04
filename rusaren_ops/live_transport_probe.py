"""Live hosted transport probe runner with diagnostics fallback."""

from __future__ import annotations

import argparse
import os
import shutil
from pathlib import Path

from . import useful_log_collect
from .common import (
    apply_env_file,
    compact_utc_timestamp,
    ensure_directory,
    fatal,
    log,
    resolve_default_config_dir,
    run,
)

PREFIX = "run_live_transport_probe"


def build_parser() -> argparse.ArgumentParser:
    """Create a parser that forwards any unknown flags to the Rust probe binary."""

    return argparse.ArgumentParser(
        prog="run_live_transport_probe.py",
        description=(
            "Run the four-client hosted transport probe with a Docker fallback."
        ),
    )


def resolve_origin() -> str:
    """Resolve the public probe origin from env."""

    explicit_origin = os.environ.get("RARENA_PROBE_ORIGIN", "").strip()
    if explicit_origin:
        return explicit_origin

    public_host = os.environ.get("PUBLIC_HOST", "").strip()
    if public_host:
        return f"https://{public_host}"

    fatal(PREFIX, "set RARENA_PROBE_ORIGIN or PUBLIC_HOST")


def local_cargo_commands() -> list[list[str]]:
    """Return cargo command candidates in the same order as the shell script."""

    commands: list[list[str]] = []
    cargo_home_bin = Path.home() / ".cargo" / "bin" / "cargo"
    if cargo_home_bin.is_file():
        commands.append([str(cargo_home_bin)])

    cargo_path = shutil.which("cargo")
    if cargo_path:
        commands.append([cargo_path])

    rustup_path = shutil.which("rustup")
    if rustup_path:
        commands.append([rustup_path, "run", "stable", "cargo"])

    return commands


def run_probe_with_local_cargo(
    *,
    repo_root: Path,
    origin: str,
    probe_log: Path,
    extra_args: list[str],
) -> int:
    """Try to run the probe with a locally installed Rust toolchain."""

    for cargo_command in local_cargo_commands():
        result = run(
            cargo_command
            + [
                "run",
                "-p",
                "live_transport_probe",
                "--release",
                "--",
                "--origin",
                origin,
                "--output",
                str(probe_log),
            ]
            + extra_args,
            cwd=repo_root / "server",
            check=False,
        )
        return result.returncode
    return 127


def run_probe_with_docker(
    *,
    repo_root: Path,
    config_dir: Path,
    cargo_home_dir: Path,
    cargo_target_dir: Path,
    rust_image: str,
    origin: str,
    probe_log: Path,
    extra_args: list[str],
) -> int:
    """Run the probe in a disposable Rust container when cargo is unavailable."""

    ensure_directory(cargo_home_dir)
    ensure_directory(cargo_target_dir)
    log(PREFIX, f"cargo not found, using {rust_image} via docker")

    result = run(
        [
            "docker",
            "run",
            "--rm",
            "--user",
            f"{os.getuid()}:{os.getgid()}",
            "-e",
            f"CARGO_HOME={cargo_home_dir}",
            "-e",
            f"CARGO_TARGET_DIR={cargo_target_dir}",
            "-v",
            f"{repo_root / 'server'}:/workspace",
            "-v",
            f"{config_dir}:{config_dir}",
            "-w",
            "/workspace",
            rust_image,
            "cargo",
            "run",
            "-p",
            "live_transport_probe",
            "--release",
            "--",
            "--origin",
            origin,
            "--output",
            str(probe_log),
        ]
        + extra_args,
        check=False,
    )
    return result.returncode


def main(argv: list[str] | None = None, *, repo_root: Path | None = None) -> int:
    """Run the live probe, then collect backend diagnostics if it fails."""

    parser = build_parser()
    _args, extra_args = parser.parse_known_args(argv)

    resolved_repo_root = (
        repo_root if repo_root is not None else Path(__file__).resolve().parents[1]
    )
    config_dir = Path(
        os.environ.get("RARENA_CONFIG_DIR", str(resolve_default_config_dir()))
    )
    env_file = Path(
        os.environ.get("RARENA_ENV_FILE", str(config_dir / "config.env"))
    )
    apply_env_file(env_file)

    origin = resolve_origin()
    output_dir = Path(
        os.environ.get("RARENA_PROBE_OUTPUT_DIR", str(config_dir / "probes"))
    )
    rust_image = os.environ.get("RARENA_PROBE_RUST_IMAGE", "rust:1.94-bookworm")
    cargo_home_dir = Path(
        os.environ.get("RARENA_PROBE_CARGO_HOME", str(config_dir / "cargo-home"))
    )
    cargo_target_dir = Path(
        os.environ.get(
            "RARENA_PROBE_TARGET_DIR",
            str(config_dir / "cargo-target" / "live-transport-probe"),
        )
    )

    ensure_directory(output_dir)
    stamp = compact_utc_timestamp()
    probe_log = output_dir / f"live-transport-probe-{stamp}.jsonl"
    diagnostics_log = output_dir / f"live-transport-diagnostics-{stamp}.txt"

    log(PREFIX, f"origin={origin}")
    log(PREFIX, f"probe_log={probe_log}")

    status = run_probe_with_local_cargo(
        repo_root=resolved_repo_root,
        origin=origin,
        probe_log=probe_log,
        extra_args=extra_args,
    )
    if status == 127:
        status = run_probe_with_docker(
            repo_root=resolved_repo_root,
            config_dir=config_dir,
            cargo_home_dir=cargo_home_dir,
            cargo_target_dir=cargo_target_dir,
            rust_image=rust_image,
            origin=origin,
            probe_log=probe_log,
            extra_args=extra_args,
        )

    if status != 0:
        log(
            PREFIX,
            f"probe failed, collecting backend diagnostics into {diagnostics_log}",
        )
        useful_log_collect.main(
            ["--origin", origin, "--output", str(diagnostics_log)],
            repo_root=resolved_repo_root,
        )
        log(PREFIX, "paste these files when reporting the failure:")
        log(PREFIX, f"  {probe_log}")
        log(PREFIX, f"  {diagnostics_log}")

    return status
