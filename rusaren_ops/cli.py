"""Top-level CLI dispatcher for the Rusaren operational tooling."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

from . import export_web_client, host_smoke, linode_deploy, linode_setup, live_transport_probe, useful_log_collect

CLI_PROG = "python -m rusaren_ops"


@dataclass(frozen=True)
class CommandSpec:
    """Describe one top-level operational subcommand."""

    name: str
    aliases: tuple[str, ...]
    summary: str
    handler: Callable[..., int]
    passes_repo_root: bool


COMMANDS: tuple[CommandSpec, ...] = (
    CommandSpec(
        name="setup",
        aliases=("linode-setup",),
        summary="Bootstrap a fresh Linux host and external config directory.",
        handler=linode_setup.main,
        passes_repo_root=True,
    ),
    CommandSpec(
        name="deploy",
        aliases=("linode-deploy",),
        summary="Build, validate, and start the hosted Docker Compose stack.",
        handler=linode_deploy.main,
        passes_repo_root=True,
    ),
    CommandSpec(
        name="smoke",
        aliases=("host-smoke",),
        summary="Probe the hosted root, health, bootstrap, and admin endpoints.",
        handler=host_smoke.main,
        passes_repo_root=False,
    ),
    CommandSpec(
        name="collect-logs",
        aliases=("useful-log-collect", "diagnostics"),
        summary="Collect the compact host diagnostics bundle used in runbooks.",
        handler=useful_log_collect.main,
        passes_repo_root=True,
    ),
    CommandSpec(
        name="live-probe",
        aliases=("run-live-transport-probe", "transport-probe"),
        summary="Run the real hosted transport probe with diagnostics fallback.",
        handler=live_transport_probe.main,
        passes_repo_root=True,
    ),
    CommandSpec(
        name="export-web-client",
        aliases=("export-web",),
        summary="Build the Linux Godot web export into server/static/webclient.",
        handler=export_web_client.main,
        passes_repo_root=True,
    ),
)


def command_lookup() -> dict[str, CommandSpec]:
    """Build the canonical-name and alias lookup table."""

    mapping: dict[str, CommandSpec] = {}
    for spec in COMMANDS:
        mapping[spec.name] = spec
        for alias in spec.aliases:
            mapping[alias] = spec
    return mapping


def build_parser() -> argparse.ArgumentParser:
    """Construct the top-level CLI help surface."""

    parser = argparse.ArgumentParser(
        prog=CLI_PROG,
        description=(
            "Rusaren operational CLI for Linux deploy, diagnostics collection, "
            "host bootstrap, transport probing, and web-client export."
        ),
        add_help=False,
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=(
            "Commands:\n"
            "  setup            Bootstrap a fresh host.\n"
            "  deploy           Update or stop the hosted stack.\n"
            "  smoke            Run the hosted smoke probes.\n"
            "  collect-logs     Gather the compact host diagnostics bundle.\n"
            "  live-probe       Exercise the hosted transport path.\n"
            "  export-web-client Build the Linux Godot web bundle.\n\n"
            "Examples:\n"
            f"  {CLI_PROG} deploy\n"
            f"  {CLI_PROG} smoke --env-file ~/rusaren-config/config.env\n"
            f"  {CLI_PROG} collect-logs --output /tmp/rusaren-diagnostics.txt\n"
            f"  {CLI_PROG} export-web-client --godot-bin godot4\n"
        ),
    )
    parser.add_argument("-h", "--help", action="store_true", help="Show this help message and exit.")
    parser.add_argument("command", nargs="?", help="Subcommand to run.")
    parser.add_argument("args", nargs=argparse.REMAINDER, help="Arguments forwarded to the subcommand.")
    return parser


def dispatch(command_name: str, forwarded_args: list[str], *, repo_root: Path) -> int:
    """Dispatch one CLI command to its underlying module entrypoint."""

    spec = command_lookup().get(command_name)
    if spec is None:
        raise SystemExit(f"Unknown rusaren_ops command: {command_name}")

    if spec.passes_repo_root:
        return spec.handler(forwarded_args, repo_root=repo_root)
    return spec.handler(forwarded_args)


def main(argv: list[str] | None = None, *, repo_root: Path | None = None) -> int:
    """Entry point for `python -m rusaren_ops`."""

    parser = build_parser()
    args = parser.parse_args(argv)
    resolved_repo_root = (
        repo_root if repo_root is not None else Path(__file__).resolve().parents[1]
    )

    if args.help and args.command is None:
        parser.print_help()
        return 0

    if args.command is None:
        parser.print_help()
        return 1

    if args.command == "help":
        if args.args:
            return dispatch(args.args[0], ["--help"], repo_root=resolved_repo_root)
        parser.print_help()
        return 0

    if args.help:
        return dispatch(args.command, ["--help"], repo_root=resolved_repo_root)

    return dispatch(args.command, list(args.args), repo_root=resolved_repo_root)
