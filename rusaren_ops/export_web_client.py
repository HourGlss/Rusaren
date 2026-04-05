"""Linux-friendly Godot web export helper used by the hosted deploy path."""

from __future__ import annotations

import argparse
import os
import re
import shutil
import tempfile
import urllib.request
import zipfile
from dataclasses import dataclass
from pathlib import Path

from .common import (
    CommandFailure,
    ensure_directory,
    fatal,
    first_existing_path,
    log,
    run,
)


@dataclass(frozen=True)
class ExportPaths:
    """Resolved file-system locations for the export job."""

    project_path: Path
    output_path: Path
    output_root: Path


@dataclass(frozen=True)
class GodotBuildInfo:
    """Minimal parsed version information from the Godot CLI."""

    binary: Path
    version_text: str
    channel: str

    @property
    def version_tag(self) -> str:
        return f"{self.version_text}-{self.channel}"

    @property
    def template_dir_name(self) -> str:
        return f"{self.version_text}.{self.channel}"


def build_parser() -> argparse.ArgumentParser:
    """Construct the command-line parser for the Linux export helper."""

    parser = argparse.ArgumentParser(
        prog="python -m rusaren_ops export-web-client",
        description=(
            "Export the Godot Web build into server/static/webclient for the hosted backend."
        ),
    )
    parser.add_argument("--godot-bin", dest="godot_bin")
    parser.add_argument("--project-path", dest="project_path")
    parser.add_argument("--output-path", dest="output_path")
    parser.add_argument("--template-root", dest="template_root")
    parser.add_argument(
        "--skip-template-install",
        action="store_true",
        help="Require templates to exist already instead of auto-downloading them.",
    )
    return parser


def resolve_paths(repo_root: Path, args: argparse.Namespace) -> ExportPaths:
    """Resolve project and output paths from args or their operational defaults."""

    project_path = (
        Path(args.project_path)
        if args.project_path
        else repo_root / "client" / "godot"
    )
    output_path = (
        Path(args.output_path)
        if args.output_path
        else repo_root / "server" / "static" / "webclient" / "index.html"
    )
    if not (project_path / "project.godot").is_file():
        fatal("export-web-client", f"Godot project not found at {project_path}")
    return ExportPaths(
        project_path=project_path,
        output_path=output_path,
        output_root=output_path.parent,
    )


def find_godot_binary(explicit_path: str | None) -> Path:
    """Resolve the Godot executable from args, env, PATH, or snap defaults."""

    if explicit_path:
        candidate = Path(explicit_path)
        if not candidate.is_file():
            fatal("export-web-client", f"Godot binary is not executable: {candidate}")
        return candidate

    candidates = [
        shutil.which("godot4"),
        shutil.which("godot-4"),
        shutil.which("godot"),
        "/snap/bin/godot4",
        "/snap/bin/godot-4",
    ]
    match = first_existing_path(Path(candidate) for candidate in candidates if candidate)
    if match is None:
        fatal(
            "export-web-client",
            "No Godot binary found. Install godot4 or set GODOT_BIN/--godot-bin.",
        )
    return match


def parse_godot_build_info(godot_binary: Path) -> GodotBuildInfo:
    """Extract the version/channel tuple Godot uses for export template lookup."""

    result = run([str(godot_binary), "--version"], capture_output=True)
    version_line = (result.stdout or "").splitlines()[0] if result.stdout else ""
    match = re.match(r"^([0-9]+\.[0-9]+(?:\.[0-9]+)?)\.([A-Za-z0-9]+)", version_line)
    if match is None:
        fatal(
            "export-web-client",
            f"Unable to parse Godot version from: {version_line or '<empty output>'}",
        )
    return GodotBuildInfo(
        binary=godot_binary,
        version_text=match.group(1),
        channel=match.group(2),
    )


def resolve_template_root(godot_info: GodotBuildInfo, explicit_root: str | None) -> Path:
    """Pick the standard Linux or snap export-template root for this editor."""

    if explicit_root:
        return Path(explicit_root)

    home = Path.home()

    # Snap installs keep their per-app writable data under `~/snap/<snap>/current`.
    if "/snap/" in str(godot_info.binary):
        snap_name = godot_info.binary.name
        return home / "snap" / snap_name / "current" / ".local" / "share" / "godot" / "export_templates"

    xdg_data_home = os.environ.get("XDG_DATA_HOME")
    if xdg_data_home:
        return Path(xdg_data_home) / "godot" / "export_templates"
    return home / ".local" / "share" / "godot" / "export_templates"


def ensure_templates_installed(
    godot_info: GodotBuildInfo,
    template_root: Path,
    *,
    allow_download: bool,
) -> None:
    """Ensure the required web export templates exist, downloading them if needed."""

    template_dir = template_root / godot_info.template_dir_name
    required_templates = [
        "web_debug.zip",
        "web_release.zip",
        "web_nothreads_debug.zip",
        "web_nothreads_release.zip",
    ]
    missing = [name for name in required_templates if not (template_dir / name).is_file()]
    if not missing:
        return

    if not allow_download:
        fatal(
            "export-web-client",
            f"Godot export templates are incomplete at {template_dir}. Missing: {', '.join(missing)}.",
        )

    ensure_directory(template_dir)
    with tempfile.TemporaryDirectory(prefix="godot-export-templates-") as temp_dir_text:
        temp_dir = Path(temp_dir_text)
        archive_path = temp_dir / "godot-templates.tpz"
        extract_root = temp_dir / "extract"
        extract_root.mkdir(parents=True, exist_ok=True)

        release_url = (
            "https://github.com/godotengine/godot-builds/releases/download/"
            f"{godot_info.version_tag}/Godot_v{godot_info.version_tag}_export_templates.tpz"
        )
        log(
            "export-web-client",
            f"downloading Godot export templates for {godot_info.version_tag} into {template_dir}",
        )
        with urllib.request.urlopen(release_url) as response:
            archive_path.write_bytes(response.read())

        with zipfile.ZipFile(archive_path) as archive:
            archive.extractall(extract_root)

        payload_root = next(extract_root.rglob("version.txt"), None)
        if payload_root is None:
            fatal(
                "export-web-client",
                "Could not locate extracted Godot export templates payload.",
            )
        payload_dir = payload_root.parent

        # We explicitly clear the destination before copying to avoid stale
        # templates surviving across Godot upgrades.
        if template_dir.exists():
            shutil.rmtree(template_dir)
        shutil.copytree(payload_dir, template_dir)
        log("export-web-client", f"installed Godot export templates into {template_dir}")


def clear_output_root(output_root: Path) -> None:
    """Remove any stale export artifacts before writing a fresh bundle."""

    ensure_directory(output_root)
    for child in output_root.iterdir():
        if child.is_dir():
            shutil.rmtree(child)
        else:
            child.unlink()


def assert_export_artifacts(output_path: Path) -> None:
    """Verify that the Godot export produced the browser artifacts we need."""

    output_root = output_path.parent
    if not output_path.is_file():
        fatal("export-web-client", f"Export did not produce {output_path}")
    if not any(output_root.glob("*.js")):
        fatal(
            "export-web-client",
            f"Export did not produce a JavaScript bundle in {output_root}",
        )
    if not any(output_root.glob("*.wasm")):
        fatal(
            "export-web-client",
            f"Export did not produce a WebAssembly bundle in {output_root}",
        )


def run_export(paths: ExportPaths, godot_info: GodotBuildInfo) -> None:
    """Execute the Godot headless web export using the checked-in export preset."""

    clear_output_root(paths.output_root)
    log("export-web-client", f"exporting Web build with {godot_info.binary}")

    try:
        run(
            [
                str(godot_info.binary),
                "--headless",
                "--path",
                str(paths.project_path),
                "--export-release",
                "Web",
                str(paths.output_path),
            ]
        )
    except CommandFailure as error:
        fatal(
            "export-web-client",
            (
                "Godot export failed. Ensure the Web export preset and export templates "
                f"are installed for this editor. ({error})"
            ),
        )

    assert_export_artifacts(paths.output_path)
    log("export-web-client", "Godot Web export complete")
    log("export-web-client", f"project: {paths.project_path}")
    log("export-web-client", f"output: {paths.output_path}")


def main(argv: list[str] | None = None, *, repo_root: Path | None = None) -> int:
    """Command entrypoint shared by the wrapper script and tests."""

    args = build_parser().parse_args(argv)
    resolved_repo_root = repo_root if repo_root is not None else Path(__file__).resolve().parents[1]
    paths = resolve_paths(resolved_repo_root, args)
    godot_info = parse_godot_build_info(find_godot_binary(args.godot_bin))
    template_root = resolve_template_root(godot_info, args.template_root)
    ensure_templates_installed(
        godot_info,
        template_root,
        allow_download=not args.skip_template_install,
    )
    run_export(paths, godot_info)
    return 0
