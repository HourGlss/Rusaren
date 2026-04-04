"""Fresh-host bootstrap for the Rusaren Linux deployment path."""

from __future__ import annotations

import argparse
import os
import shutil
import urllib.request
from dataclasses import dataclass
from pathlib import Path

from .common import ensure_directory, ensure_owner, fatal, run

PREFIX = "linode-setup"


@dataclass
class SetupConfig:
    """Resolved environment-driven configuration for host bootstrap."""

    deploy_user: str
    deploy_dir: Path
    config_dir: Path
    repo_url: str
    repo_ref: str
    timezone: str
    hostname_fqdn: str
    public_host: str
    acme_email: str
    turn_public_host: str
    turn_realm: str
    turn_shared_secret: str
    turn_external_ip: str
    turn_ttl_seconds: str
    prometheus_bind: str
    rust_log: str
    admin_username: str
    admin_password: str
    admin_cidr: str
    install_godot: bool
    godot_snap_name: str
    smoke_interval_minutes: str
    live_probe_interval_minutes: str
    run_deploy: bool

    @property
    def cargo_home_dir(self) -> Path:
        return self.config_dir / "cargo-home"

    @property
    def cargo_target_dir(self) -> Path:
        return self.config_dir / "cargo-target"

    @property
    def probes_dir(self) -> Path:
        return self.config_dir / "probes"


def env_bool(name: str, default: str) -> bool:
    """Interpret common shell-style boolean strings."""

    return os.environ.get(name, default).strip().lower() in {"1", "true", "yes", "on"}


def build_parser() -> argparse.ArgumentParser:
    """Construct a minimal parser so the bootstrap entrypoint supports `--help`."""

    return argparse.ArgumentParser(
        prog="setup.py",
        description=(
            "Bootstrap a fresh Linux host for Rusaren deployment using environment variables."
        ),
    )


def resolve_deploy_user(repo_root: Path) -> str:
    """Mirror the shell precedence for the intended non-root deploy user."""

    owner_name = ""
    if repo_root.exists():
        import pwd

        owner_name = pwd.getpwuid(repo_root.stat().st_uid).pw_name

    return (
        os.environ.get("DEPLOY_USER", "").strip()
        or os.environ.get("SUDO_USER", "").strip()
        or owner_name
        or "rarena"
    )


def build_config(repo_root: Path) -> SetupConfig:
    """Resolve the bootstrap configuration from the host environment."""

    deploy_user = resolve_deploy_user(repo_root)
    deploy_dir = Path(os.environ.get("DEPLOY_DIR", str(repo_root)))
    config_dir = Path(
        os.environ.get("CONFIG_DIR", f"/home/{deploy_user}/rusaren-config")
    )

    public_host = os.environ.get("PUBLIC_HOST", "").strip()
    acme_email = os.environ.get("ACME_EMAIL", "").strip()
    if not public_host:
        fatal(PREFIX, "PUBLIC_HOST is required")
    if not acme_email:
        fatal(PREFIX, "ACME_EMAIL is required")

    turn_public_host = os.environ.get("TURN_PUBLIC_HOST", "").strip() or f"turn.{public_host}"
    turn_realm = os.environ.get("TURN_REALM", "").strip() or public_host
    turn_shared_secret = os.environ.get("TURN_SHARED_SECRET", "").strip()
    if not turn_shared_secret:
        turn_shared_secret = (run(["openssl", "rand", "-hex", "32"], capture_output=True).stdout or "").strip()

    admin_password = os.environ.get("RARENA_ADMIN_PASSWORD", "").strip()
    if not admin_password:
        admin_password = (
            run(["openssl", "rand", "-base64", "24"], capture_output=True).stdout or ""
        ).replace("\n", "").strip()

    turn_external_ip = os.environ.get("TURN_EXTERNAL_IP", "").strip() or detect_public_ipv4()

    return SetupConfig(
        deploy_user=deploy_user,
        deploy_dir=deploy_dir,
        config_dir=config_dir,
        repo_url=os.environ.get("REPO_URL", "https://github.com/HourGlss/Rusaren.git"),
        repo_ref=os.environ.get("REPO_REF", "main"),
        timezone=os.environ.get("TIMEZONE", "UTC"),
        hostname_fqdn=os.environ.get("HOSTNAME_FQDN", public_host),
        public_host=public_host,
        acme_email=acme_email,
        turn_public_host=turn_public_host,
        turn_realm=turn_realm,
        turn_shared_secret=turn_shared_secret,
        turn_external_ip=turn_external_ip,
        turn_ttl_seconds=os.environ.get("TURN_TTL_SECONDS", "3600"),
        prometheus_bind=os.environ.get("PROMETHEUS_BIND", "127.0.0.1:9090"),
        rust_log=os.environ.get("RARENA_RUST_LOG", "info,axum=info,tower_http=info"),
        admin_username=os.environ.get("RARENA_ADMIN_USERNAME", "admin"),
        admin_password=admin_password,
        admin_cidr=os.environ.get("ADMIN_CIDR", "").strip(),
        install_godot=env_bool("INSTALL_GODOT", "1"),
        godot_snap_name=os.environ.get("GODOT_SNAP_NAME", "").strip(),
        smoke_interval_minutes=os.environ.get("SMOKE_INTERVAL_MINUTES", "5"),
        live_probe_interval_minutes=os.environ.get("LIVE_PROBE_INTERVAL_MINUTES", "60"),
        run_deploy=env_bool("RUN_DEPLOY", "1"),
    )


def detect_admin_cidr(config: SetupConfig) -> str:
    """Derive the default SSH admin CIDR from the current session."""

    if config.admin_cidr:
        return config.admin_cidr
    ssh_client = os.environ.get("SSH_CLIENT", "").strip()
    if ssh_client:
        return f"{ssh_client.split()[0]}/32"
    ssh_connection = os.environ.get("SSH_CONNECTION", "").strip()
    if ssh_connection:
        return f"{ssh_connection.split()[0]}/32"
    return ""


def detect_public_ipv4() -> str:
    """Resolve the public IPv4 used by the TURN service."""

    try:
        with urllib.request.urlopen("https://ifconfig.me/ip", timeout=5.0) as response:
            ip_text = response.read().decode("utf-8", errors="replace").strip()
            if ip_text:
                return ip_text
    except Exception:
        pass

    result = run(
        ["ip", "-4", "addr", "show", "scope", "global", "up"],
        capture_output=True,
        check=False,
    )
    for line in (result.stdout or "").splitlines():
        line = line.strip()
        if line.startswith("inet "):
            return line.split()[1].split("/")[0]
    fatal(
        PREFIX,
        "unable to detect a public IPv4 address; set TURN_EXTERNAL_IP explicitly",
    )


def write_text(path: Path, text: str, *, mode: int | None = None) -> None:
    """Write a text file and optionally enforce a POSIX mode."""

    ensure_directory(path.parent)
    path.write_text(text, encoding="utf-8")
    if mode is not None:
        path.chmod(mode)


def install_base_packages() -> None:
    """Install the baseline host packages used by deploy and diagnostics."""

    print("[linode-setup] installing base packages")
    run(["apt-get", "update"])
    run(["apt-get", "upgrade", "-y"])
    run(
        [
            "apt-get",
            "install",
            "-y",
            "apt-transport-https",
            "ca-certificates",
            "curl",
            "fail2ban",
            "git",
            "gnupg",
            "jq",
            "openssl",
            "snapd",
            "software-properties-common",
            "ufw",
            "unzip",
            "unattended-upgrades",
        ]
    )


def configure_timezone_and_hostname(config: SetupConfig) -> None:
    """Apply timezone and hostname settings."""

    print("[linode-setup] setting timezone and hostname")
    run(["timedatectl", "set-timezone", config.timezone])
    if config.hostname_fqdn:
        run(["hostnamectl", "set-hostname", config.hostname_fqdn])


def ensure_limited_user(config: SetupConfig) -> None:
    """Create the non-root deploy user and seed SSH keys when possible."""

    result = run(["id", "-u", config.deploy_user], check=False, capture_output=True)
    if result.returncode != 0:
        print(f"[linode-setup] creating limited admin user {config.deploy_user}")
        run(["adduser", "--disabled-password", "--gecos", "", config.deploy_user])

    run(["usermod", "-aG", "sudo", config.deploy_user])

    user_authorized_keys = Path(f"/home/{config.deploy_user}/.ssh/authorized_keys")
    root_authorized_keys = Path("/root/.ssh/authorized_keys")
    if not user_authorized_keys.is_file() and root_authorized_keys.is_file():
        ensure_directory(user_authorized_keys.parent, mode=0o700)
        shutil.copy2(root_authorized_keys, user_authorized_keys)
        ensure_owner(user_authorized_keys.parent, config.deploy_user)
        user_authorized_keys.chmod(0o600)


def configure_ssh_hardening(config: SetupConfig) -> None:
    """Install a small hardening drop-in when SSH keys are present."""

    root_keys = Path("/root/.ssh/authorized_keys")
    user_keys = Path(f"/home/{config.deploy_user}/.ssh/authorized_keys")
    if not root_keys.is_file() and not user_keys.is_file():
        print("[linode-setup] skipping SSH hardening because no authorized_keys file was detected")
        return

    print("[linode-setup] hardening sshd")
    write_text(
        Path("/etc/ssh/sshd_config.d/99-rarena-hardening.conf"),
        "\n".join(
            [
                "PasswordAuthentication no",
                "KbdInteractiveAuthentication no",
                "ChallengeResponseAuthentication no",
                "PubkeyAuthentication yes",
                "PermitRootLogin prohibit-password",
                "X11Forwarding no",
                "AllowTcpForwarding yes",
                "ClientAliveInterval 300",
                "ClientAliveCountMax 2",
                "",
            ]
        ),
    )
    run(["sshd", "-t"])
    restart = run(["systemctl", "restart", "ssh"], check=False)
    if restart.returncode != 0:
        run(["systemctl", "restart", "sshd"])


def configure_unattended_upgrades() -> None:
    """Enable unattended upgrades on the host."""

    print("[linode-setup] enabling unattended upgrades")
    write_text(
        Path("/etc/apt/apt.conf.d/20auto-upgrades"),
        'APT::Periodic::Update-Package-Lists "1";\nAPT::Periodic::Unattended-Upgrade "1";\n',
    )
    run(["systemctl", "enable", "--now", "unattended-upgrades"])


def configure_fail2ban() -> None:
    """Enable the basic fail2ban sshd jail."""

    print("[linode-setup] configuring fail2ban")
    write_text(
        Path("/etc/fail2ban/jail.d/sshd.local"),
        "\n".join(
            [
                "[sshd]",
                "enabled = true",
                "backend = systemd",
                "bantime = 1h",
                "findtime = 10m",
                "maxretry = 5",
                "",
            ]
        ),
    )
    run(["systemctl", "enable", "--now", "fail2ban"])


def configure_firewall(config: SetupConfig) -> None:
    """Apply the hosted firewall rules used by the deploy runbook."""

    print("[linode-setup] configuring ufw")
    admin_rule = detect_admin_cidr(config)
    run(["ufw", "--force", "reset"])
    run(["ufw", "default", "deny", "incoming"])
    run(["ufw", "default", "allow", "outgoing"])

    if admin_rule:
        run(["ufw", "allow", "from", admin_rule, "to", "any", "port", "22", "proto", "tcp"])
    else:
        print(
            "[linode-setup] ADMIN_CIDR was not supplied and no SSH client IP was detected; allowing ssh from anywhere"
        )
        run(["ufw", "allow", "22/tcp"])

    for rule in [
        "80/tcp",
        "443/tcp",
        "3478/tcp",
        "3478/udp",
        "49160:49200/udp",
    ]:
        run(["ufw", "allow", rule])
    run(["ufw", "--force", "enable"])


def install_godot_cli(config: SetupConfig) -> None:
    """Install the Godot CLI from snap unless disabled."""

    if not config.install_godot:
        print("[linode-setup] skipping Godot install because INSTALL_GODOT is disabled")
        return

    print("[linode-setup] installing Godot CLI from snap")
    run(["systemctl", "enable", "--now", "snapd.socket"])
    run(["systemctl", "enable", "--now", "snapd.service"], check=False)
    run(["systemctl", "enable", "--now", "snapd.apparmor.service"], check=False)
    run(["snap", "wait", "system", "seed.loaded"], check=False)

    snap_name = config.godot_snap_name
    if not snap_name:
        for candidate in ["godot4", "godot-4"]:
            if run(["snap", "list", candidate], check=False).returncode == 0:
                snap_name = candidate
                break
    if not snap_name:
        for candidate in ["godot4", "godot-4"]:
            if run(["snap", "install", candidate], check=False).returncode == 0:
                snap_name = candidate
                break
    if not snap_name:
        fatal(
            PREFIX,
            "failed to install a Godot snap; set GODOT_SNAP_NAME explicitly to a valid snap package",
        )
    if run(["snap", "list", snap_name], check=False).returncode != 0:
        run(["snap", "install", snap_name])
    print(f"[linode-setup] Godot CLI is available via snap package {snap_name}")


def install_docker(config: SetupConfig) -> None:
    """Install Docker Engine from Docker's official apt repository."""

    print("[linode-setup] installing docker engine from Docker's apt repository")
    run(
        [
            "apt-get",
            "remove",
            "-y",
            "docker.io",
            "docker-compose",
            "docker-compose-v2",
            "docker-doc",
            "podman-docker",
            "containerd",
            "runc",
        ],
        check=False,
    )

    ensure_directory(Path("/etc/apt/keyrings"), mode=0o755)
    run(
        [
            "curl",
            "-fsSL",
            "https://download.docker.com/linux/ubuntu/gpg",
            "-o",
            "/etc/apt/keyrings/docker.asc",
        ]
    )
    run(["chmod", "a+r", "/etc/apt/keyrings/docker.asc"])

    os_release = run(
        [
            "sh",
            "-c",
            ". /etc/os-release && echo \"${UBUNTU_CODENAME:-$VERSION_CODENAME}\"",
        ],
        capture_output=True,
    )
    codename = (os_release.stdout or "").strip()
    write_text(
        Path("/etc/apt/sources.list.d/docker.sources"),
        "\n".join(
            [
                "Types: deb",
                "URIs: https://download.docker.com/linux/ubuntu",
                f"Suites: {codename}",
                "Components: stable",
                "Signed-By: /etc/apt/keyrings/docker.asc",
                "",
            ]
        ),
    )

    run(["apt-get", "update"])
    run(
        [
            "apt-get",
            "install",
            "-y",
            "docker-ce",
            "docker-ce-cli",
            "containerd.io",
            "docker-buildx-plugin",
            "docker-compose-plugin",
        ]
    )
    run(["systemctl", "enable", "--now", "docker", "containerd"])
    if run(["getent", "group", "docker"], check=False).returncode == 0:
        run(["usermod", "-aG", "docker", config.deploy_user])


def configure_docker_daemon() -> None:
    """Install the minimal daemon.json used by the hosted stack."""

    print("[linode-setup] configuring the docker daemon")
    ensure_directory(Path("/etc/docker"), mode=0o755)
    write_text(
        Path("/etc/docker/daemon.json"),
        "{\n"
        '  "features": {\n'
        '    "buildkit": true\n'
        "  },\n"
        '  "live-restore": true,\n'
        '  "log-driver": "local"\n'
        "}\n",
    )
    run(["systemctl", "restart", "docker"])


def prepare_repo(config: SetupConfig) -> None:
    """Clone the repo if needed and ensure the hosted static root exists."""

    ensure_directory(config.deploy_dir.parent, mode=0o755)
    if not (config.deploy_dir / "deploy" / "docker-compose.yml").is_file():
        print(f"[linode-setup] cloning repository into {config.deploy_dir}")
        if config.deploy_dir.exists():
            shutil.rmtree(config.deploy_dir)
        run(
            [
                "git",
                "clone",
                "--branch",
                config.repo_ref,
                "--depth",
                "1",
                config.repo_url,
                str(config.deploy_dir),
            ]
        )

    ensure_directory(config.deploy_dir / "server" / "static" / "webclient", mode=0o755)
    ensure_owner(config.deploy_dir, config.deploy_user)


def prepare_config_dir(config: SetupConfig) -> None:
    """Create the external config directory and its operational subdirectories."""

    ensure_directory(config.config_dir, mode=0o700)
    ensure_directory(config.cargo_home_dir, mode=0o775)
    ensure_directory(config.cargo_target_dir, mode=0o775)
    ensure_directory(config.probes_dir, mode=0o775)
    ensure_owner(config.config_dir, config.deploy_user)


def write_deploy_env(config: SetupConfig) -> None:
    """Write the external deploy env file on first bootstrap."""

    env_file = config.config_dir / "config.env"
    if env_file.is_file():
        print(f"[linode-setup] preserving existing {env_file}")
        return

    print(f"[linode-setup] writing {env_file}")
    write_text(
        env_file,
        "\n".join(
            [
                f"PUBLIC_HOST={config.public_host}",
                f"ACME_EMAIL={config.acme_email}",
                f"RARENA_RUST_LOG={config.rust_log}",
                f"PROMETHEUS_BIND={config.prometheus_bind}",
                f"TURN_PUBLIC_HOST={config.turn_public_host}",
                f"TURN_REALM={config.turn_realm}",
                f"TURN_SHARED_SECRET={config.turn_shared_secret}",
                f"TURN_EXTERNAL_IP={config.turn_external_ip}",
                f"TURN_TTL_SECONDS={config.turn_ttl_seconds}",
                f"RARENA_ADMIN_USERNAME={config.admin_username}",
                f"RARENA_ADMIN_PASSWORD={config.admin_password}",
                "",
            ]
        ),
        mode=0o600,
    )
    ensure_owner(env_file, config.deploy_user)


def write_compose_override(config: SetupConfig) -> None:
    """Write the external compose override on first bootstrap."""

    override_file = config.config_dir / "docker-compose.override.yml"
    if override_file.is_file():
        print(f"[linode-setup] preserving existing {override_file}")
        return

    print(f"[linode-setup] writing {override_file}")
    write_text(override_file, "services: {}\n", mode=0o600)
    ensure_owner(override_file, config.deploy_user)


def install_compose_service(config: SetupConfig) -> None:
    """Install the systemd unit that owns the compose lifecycle."""

    print("[linode-setup] installing systemd unit")
    write_text(
        Path("/etc/systemd/system/rusaren-compose.service"),
        "\n".join(
            [
                "[Unit]",
                "Description=Rusaren Docker Compose stack",
                "Requires=docker.service",
                "After=docker.service network-online.target",
                "Wants=network-online.target",
                "",
                "[Service]",
                "Type=oneshot",
                "RemainAfterExit=yes",
                f"User={config.deploy_user}",
                f"Group={config.deploy_user}",
                "SupplementaryGroups=docker",
                f"Environment=HOME=/home/{config.deploy_user}",
                "Environment=PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/snap/bin",
                f"Environment=CONFIG_DIR={config.config_dir}",
                f"WorkingDirectory={config.deploy_dir}",
                f"ExecStart=/usr/bin/env python3 {config.deploy_dir}/deploy/deploy.py",
                f"ExecStop=/usr/bin/env python3 {config.deploy_dir}/deploy/deploy.py --down",
                "TimeoutStartSec=0",
                "",
                "[Install]",
                "WantedBy=multi-user.target",
                "",
            ]
        ),
    )
    run(["systemctl", "daemon-reload"])
    run(["systemctl", "enable", "rusaren-compose.service"])


def install_smoke_probe_timer(config: SetupConfig) -> None:
    """Install the recurring hosted smoke timer."""

    print("[linode-setup] installing hosted smoke probe timer")
    write_text(
        Path("/etc/systemd/system/rusaren-smoke.service"),
        "\n".join(
            [
                "[Unit]",
                "Description=Rusaren hosted smoke probes",
                "After=rusaren-compose.service network-online.target",
                "Wants=network-online.target",
                "",
                "[Service]",
                "Type=oneshot",
                f"User={config.deploy_user}",
                f"Group={config.deploy_user}",
                f"Environment=HOME=/home/{config.deploy_user}",
                "Environment=PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/snap/bin",
                f"Environment=CONFIG_DIR={config.config_dir}",
                f"WorkingDirectory={config.deploy_dir}",
                f"ExecStart=/usr/bin/env python3 {config.deploy_dir}/deploy/host-smoke.py --env-file {config.config_dir / 'config.env'}",
                "",
            ]
        ),
    )
    write_text(
        Path("/etc/systemd/system/rusaren-smoke.timer"),
        "\n".join(
            [
                "[Unit]",
                "Description=Run Rusaren hosted smoke probes on a schedule",
                "",
                "[Timer]",
                "OnBootSec=5m",
                f"OnUnitActiveSec={config.smoke_interval_minutes}m",
                "Unit=rusaren-smoke.service",
                "",
                "[Install]",
                "WantedBy=timers.target",
                "",
            ]
        ),
    )
    run(["systemctl", "daemon-reload"])
    run(["systemctl", "enable", "--now", "rusaren-smoke.timer"])


def install_live_transport_probe_timer(config: SetupConfig) -> None:
    """Install the recurring live transport probe timer."""

    print("[linode-setup] installing hosted live transport probe timer")
    write_text(
        Path("/etc/systemd/system/rusaren-liveprobe.service"),
        "\n".join(
            [
                "[Unit]",
                "Description=Rusaren hosted live transport probe",
                "After=rusaren-compose.service network-online.target",
                "Wants=network-online.target",
                "",
                "[Service]",
                "Type=oneshot",
                f"User={config.deploy_user}",
                f"Group={config.deploy_user}",
                f"Environment=HOME=/home/{config.deploy_user}",
                "Environment=PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/snap/bin",
                f"Environment=RARENA_CONFIG_DIR={config.config_dir}",
                f"Environment=RARENA_PROBE_CARGO_HOME={config.cargo_home_dir}",
                f"Environment=RARENA_PROBE_TARGET_DIR={config.cargo_target_dir / 'live-transport-probe'}",
                f"Environment=RARENA_PROBE_OUTPUT_DIR={config.probes_dir}",
                f"WorkingDirectory={config.deploy_dir}",
                f"ExecStart=/usr/bin/env python3 {config.deploy_dir}/deploy/run_live_transport_probe.py",
                "",
            ]
        ),
    )
    write_text(
        Path("/etc/systemd/system/rusaren-liveprobe.timer"),
        "\n".join(
            [
                "[Unit]",
                "Description=Run the Rusaren hosted live transport probe on a schedule",
                "",
                "[Timer]",
                "OnBootSec=20m",
                f"OnUnitActiveSec={config.live_probe_interval_minutes}m",
                "Unit=rusaren-liveprobe.service",
                "",
                "[Install]",
                "WantedBy=timers.target",
                "",
            ]
        ),
    )
    run(["systemctl", "daemon-reload"])
    run(["systemctl", "enable", "--now", "rusaren-liveprobe.timer"])


def main(argv: list[str] | None = None, *, repo_root: Path | None = None) -> int:
    """Bootstrap the Linux host for Rusaren deployment."""

    build_parser().parse_args(argv)
    if os.geteuid() != 0:
        fatal(PREFIX, "run this script as root or with sudo")

    resolved_repo_root = (
        repo_root if repo_root is not None else Path(__file__).resolve().parents[1]
    )
    config = build_config(resolved_repo_root)

    install_base_packages()
    configure_timezone_and_hostname(config)
    ensure_limited_user(config)
    configure_ssh_hardening(config)
    configure_unattended_upgrades()
    configure_fail2ban()
    configure_firewall(config)
    install_godot_cli(config)
    install_docker(config)
    configure_docker_daemon()
    prepare_repo(config)
    prepare_config_dir(config)
    write_deploy_env(config)
    write_compose_override(config)
    install_compose_service(config)
    install_smoke_probe_timer(config)
    install_live_transport_probe_timer(config)

    if config.run_deploy:
        run(["systemctl", "restart", "rusaren-compose.service"])

    print("[linode-setup] bootstrap complete")
    print(f"[linode-setup] deploy config directory: {config.config_dir}")
    print(
        f"[linode-setup] private admin dashboard: https://{config.public_host}/adminz (user {config.admin_username})"
    )
    print(
        f"[linode-setup] smoke timer: rusaren-smoke.timer every {config.smoke_interval_minutes} minute(s)"
    )
    print(
        "[linode-setup] live transport probe timer: "
        f"rusaren-liveprobe.timer every {config.live_probe_interval_minutes} minute(s)"
    )
    if config.install_godot:
        print(
            "[linode-setup] Godot CLI installed via snap; deploy can now build the web client "
            "on-host with server/scripts/export-web-client.py"
        )
    print(
        "[linode-setup] Cloud Firewall should still be enabled in Linode to restrict SSH source ranges at the network edge"
    )
    return 0
