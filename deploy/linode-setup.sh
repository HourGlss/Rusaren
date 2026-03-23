#!/usr/bin/env bash
set -Eeuo pipefail

if [[ "${EUID:-$(id -u)}" -ne 0 ]]; then
    printf '[linode-setup] ERROR: run this script as root or with sudo\n' >&2
    exit 1
fi

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

DEPLOY_USER="${DEPLOY_USER:-${SUDO_USER:-rarena}}"
DEPLOY_DIR="${DEPLOY_DIR:-${REPO_ROOT}}"
REPO_URL="${REPO_URL:-https://github.com/HourGlss/Rusaren.git}"
REPO_REF="${REPO_REF:-main}"
TIMEZONE="${TIMEZONE:-UTC}"
HOSTNAME_FQDN="${HOSTNAME_FQDN:-${PUBLIC_HOST:-}}"
PUBLIC_HOST="${PUBLIC_HOST:-}"
ACME_EMAIL="${ACME_EMAIL:-}"
TURN_PUBLIC_HOST="${TURN_PUBLIC_HOST:-}"
TURN_REALM="${TURN_REALM:-}"
TURN_SHARED_SECRET="${TURN_SHARED_SECRET:-}"
TURN_EXTERNAL_IP="${TURN_EXTERNAL_IP:-}"
TURN_TTL_SECONDS="${TURN_TTL_SECONDS:-3600}"
PROMETHEUS_BIND="${PROMETHEUS_BIND:-127.0.0.1:9090}"
RARENA_RUST_LOG="${RARENA_RUST_LOG:-info,axum=info,tower_http=info}"
RARENA_ADMIN_USERNAME="${RARENA_ADMIN_USERNAME:-admin}"
RARENA_ADMIN_PASSWORD="${RARENA_ADMIN_PASSWORD:-}"
ADMIN_CIDR="${ADMIN_CIDR:-}"
INSTALL_GODOT="${INSTALL_GODOT:-1}"
GODOT_SNAP_NAME="${GODOT_SNAP_NAME:-}"
SMOKE_INTERVAL_MINUTES="${SMOKE_INTERVAL_MINUTES:-5}"
RUN_DEPLOY="${RUN_DEPLOY:-1}"

log() {
    printf '[linode-setup] %s\n' "$*"
}

fatal() {
    printf '[linode-setup] ERROR: %s\n' "$*" >&2
    exit 1
}

require_value() {
    local name="$1"
    local value="${!name:-}"
    [[ -n "${value}" ]] || fatal "${name} is required"
}

backup_if_present() {
    local path="$1"
    if [[ -f "${path}" ]]; then
        cp "${path}" "${path}.bak.$(date +%Y%m%d%H%M%S)"
    fi
}

detect_admin_cidr() {
    if [[ -n "${ADMIN_CIDR}" ]]; then
        printf '%s\n' "${ADMIN_CIDR}"
        return
    fi

    if [[ -n "${SSH_CLIENT:-}" ]]; then
        printf '%s/32\n' "${SSH_CLIENT%% *}"
        return
    fi

    if [[ -n "${SSH_CONNECTION:-}" ]]; then
        printf '%s/32\n' "${SSH_CONNECTION%% *}"
        return
    fi
}

detect_public_ipv4() {
    local ip=""
    ip="$(curl -4fsS --max-time 5 https://ifconfig.me/ip 2>/dev/null || true)"
    if [[ -n "${ip}" ]]; then
        printf '%s\n' "${ip}"
        return
    fi

    ip="$(ip -4 addr show scope global up 2>/dev/null | awk '/inet / {sub(/\/.*/, "", $2); print $2; exit}')"
    [[ -n "${ip}" ]] || fatal "unable to detect a public IPv4 address; set TURN_EXTERNAL_IP explicitly"
    printf '%s\n' "${ip}"
}

install_base_packages() {
    log "installing base packages"
    export DEBIAN_FRONTEND=noninteractive
    apt-get update
    apt-get upgrade -y
    apt-get install -y \
        apt-transport-https \
        ca-certificates \
        curl \
        fail2ban \
        git \
        gnupg \
        jq \
        openssl \
        snapd \
        software-properties-common \
        ufw \
        unzip \
        unattended-upgrades
}

configure_timezone_and_hostname() {
    log "setting timezone and hostname"
    timedatectl set-timezone "${TIMEZONE}"
    if [[ -n "${HOSTNAME_FQDN}" ]]; then
        hostnamectl set-hostname "${HOSTNAME_FQDN}"
    fi
}

ensure_limited_user() {
    if ! id -u "${DEPLOY_USER}" >/dev/null 2>&1; then
        log "creating limited admin user ${DEPLOY_USER}"
        adduser --disabled-password --gecos "" "${DEPLOY_USER}"
    fi

    usermod -aG sudo "${DEPLOY_USER}"

    if [[ ! -f "/home/${DEPLOY_USER}/.ssh/authorized_keys" && -f /root/.ssh/authorized_keys ]]; then
        install -d -m 700 -o "${DEPLOY_USER}" -g "${DEPLOY_USER}" "/home/${DEPLOY_USER}/.ssh"
        install -m 600 -o "${DEPLOY_USER}" -g "${DEPLOY_USER}" /root/.ssh/authorized_keys "/home/${DEPLOY_USER}/.ssh/authorized_keys"
    fi
}

configure_ssh_hardening() {
    local ssh_keys_present=0
    if [[ -s /root/.ssh/authorized_keys ]]; then
        ssh_keys_present=1
    elif [[ -s "/home/${DEPLOY_USER}/.ssh/authorized_keys" ]]; then
        ssh_keys_present=1
    fi

    if [[ "${ssh_keys_present}" -ne 1 ]]; then
        log "skipping SSH hardening because no authorized_keys file was detected"
        return
    fi

    log "hardening sshd"
    install -d -m 755 /etc/ssh/sshd_config.d
    cat > /etc/ssh/sshd_config.d/99-rarena-hardening.conf <<'EOF'
PasswordAuthentication no
KbdInteractiveAuthentication no
ChallengeResponseAuthentication no
PubkeyAuthentication yes
PermitRootLogin prohibit-password
X11Forwarding no
AllowTcpForwarding yes
ClientAliveInterval 300
ClientAliveCountMax 2
EOF

    sshd -t
    systemctl restart ssh || systemctl restart sshd
}

configure_unattended_upgrades() {
    log "enabling unattended upgrades"
    cat > /etc/apt/apt.conf.d/20auto-upgrades <<'EOF'
APT::Periodic::Update-Package-Lists "1";
APT::Periodic::Unattended-Upgrade "1";
EOF
    systemctl enable --now unattended-upgrades
}

configure_fail2ban() {
    log "configuring fail2ban"
    install -d -m 755 /etc/fail2ban/jail.d
    cat > /etc/fail2ban/jail.d/sshd.local <<'EOF'
[sshd]
enabled = true
backend = systemd
bantime = 1h
findtime = 10m
maxretry = 5
EOF
    systemctl enable --now fail2ban
}

configure_firewall() {
    local admin_rule
    admin_rule="$(detect_admin_cidr)"

    log "configuring ufw"
    ufw --force reset
    ufw default deny incoming
    ufw default allow outgoing

    if [[ -n "${admin_rule}" ]]; then
        ufw allow from "${admin_rule}" to any port 22 proto tcp
    else
        log "ADMIN_CIDR was not supplied and no SSH client IP was detected; allowing ssh from anywhere"
        ufw allow 22/tcp
    fi

    ufw allow 80/tcp
    ufw allow 443/tcp
    ufw allow 3478/tcp
    ufw allow 3478/udp
    ufw allow 49160:49200/udp
    ufw --force enable
}

install_godot_cli() {
    if [[ "${INSTALL_GODOT}" != "1" ]]; then
        log "skipping Godot install because INSTALL_GODOT=${INSTALL_GODOT}"
        return
    fi

    log "installing Godot CLI from snap"
    systemctl enable --now snapd.socket
    systemctl enable --now snapd.service >/dev/null 2>&1 || true
    systemctl enable --now snapd.apparmor.service >/dev/null 2>&1 || true
    snap wait system seed.loaded >/dev/null 2>&1 || true

    local snap_name=""
    if [[ -n "${GODOT_SNAP_NAME}" ]]; then
        snap_name="${GODOT_SNAP_NAME}"
    elif snap list godot4 >/dev/null 2>&1; then
        snap_name="godot4"
    elif snap list godot-4 >/dev/null 2>&1; then
        snap_name="godot-4"
    fi

    if [[ -z "${snap_name}" ]]; then
        for candidate in godot4 godot-4; do
            if snap install "${candidate}" >/dev/null 2>&1; then
                snap_name="${candidate}"
                break
            fi
        done
    fi

    [[ -n "${snap_name}" ]] || fatal "failed to install a Godot snap; set GODOT_SNAP_NAME explicitly to a valid snap package"

    if ! snap list "${snap_name}" >/dev/null 2>&1; then
        snap install "${snap_name}"
    fi
    log "Godot CLI is available via snap package ${snap_name}"
}

install_docker() {
    log "installing docker engine from Docker's apt repository"
    apt-get remove -y docker.io docker-compose docker-compose-v2 docker-doc podman-docker containerd runc >/dev/null 2>&1 || true

    install -m 0755 -d /etc/apt/keyrings
    curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
    chmod a+r /etc/apt/keyrings/docker.asc

    cat > /etc/apt/sources.list.d/docker.sources <<EOF
Types: deb
URIs: https://download.docker.com/linux/ubuntu
Suites: $(. /etc/os-release && echo "${UBUNTU_CODENAME:-$VERSION_CODENAME}")
Components: stable
Signed-By: /etc/apt/keyrings/docker.asc
EOF

    apt-get update
    apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
    systemctl enable --now docker containerd

    if getent group docker >/dev/null 2>&1; then
        usermod -aG docker "${DEPLOY_USER}"
    fi
}

configure_docker_daemon() {
    log "configuring the docker daemon"
    install -d -m 755 /etc/docker
    backup_if_present /etc/docker/daemon.json
    cat > /etc/docker/daemon.json <<'EOF'
{
  "features": {
    "buildkit": true
  },
  "live-restore": true,
  "log-driver": "local"
}
EOF
    systemctl restart docker
}

prepare_repo() {
    install -d -m 755 "$(dirname -- "${DEPLOY_DIR}")"

    if [[ ! -f "${DEPLOY_DIR}/deploy/docker-compose.yml" ]]; then
        log "cloning repository into ${DEPLOY_DIR}"
        rm -rf "${DEPLOY_DIR}"
        git clone --branch "${REPO_REF}" --depth 1 "${REPO_URL}" "${DEPLOY_DIR}"
    fi

    install -d -m 755 "${DEPLOY_DIR}/server/static/webclient"
    chown -R "${DEPLOY_USER}:${DEPLOY_USER}" "${DEPLOY_DIR}"
}

write_deploy_env() {
    local env_file="${DEPLOY_DIR}/deploy/.env"

    log "writing ${env_file}"
    backup_if_present "${env_file}"
    cat > "${env_file}" <<EOF
PUBLIC_HOST=${PUBLIC_HOST}
ACME_EMAIL=${ACME_EMAIL}
RARENA_RUST_LOG=${RARENA_RUST_LOG}
PROMETHEUS_BIND=${PROMETHEUS_BIND}
TURN_PUBLIC_HOST=${TURN_PUBLIC_HOST}
TURN_REALM=${TURN_REALM}
TURN_SHARED_SECRET=${TURN_SHARED_SECRET}
TURN_EXTERNAL_IP=${TURN_EXTERNAL_IP}
TURN_TTL_SECONDS=${TURN_TTL_SECONDS}
RARENA_ADMIN_USERNAME=${RARENA_ADMIN_USERNAME}
RARENA_ADMIN_PASSWORD=${RARENA_ADMIN_PASSWORD}
EOF
    chown "${DEPLOY_USER}:${DEPLOY_USER}" "${env_file}"
}

install_compose_service() {
    local unit_file="/etc/systemd/system/rusaren-compose.service"

    log "installing systemd unit"
    cat > "${unit_file}" <<EOF
[Unit]
Description=Rusaren Docker Compose stack
Requires=docker.service
After=docker.service network-online.target
Wants=network-online.target

[Service]
Type=oneshot
RemainAfterExit=yes
User=${DEPLOY_USER}
Group=${DEPLOY_USER}
SupplementaryGroups=docker
Environment=HOME=/home/${DEPLOY_USER}
Environment=PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/snap/bin
WorkingDirectory=${DEPLOY_DIR}
ExecStart=/usr/bin/env bash ${DEPLOY_DIR}/deploy/deploy.sh
ExecStop=/usr/bin/docker compose --env-file ${DEPLOY_DIR}/deploy/.env -f ${DEPLOY_DIR}/deploy/docker-compose.yml down
TimeoutStartSec=0

[Install]
WantedBy=multi-user.target
EOF

    systemctl daemon-reload
    systemctl enable rusaren-compose.service
}

install_smoke_probe_timer() {
    local service_file="/etc/systemd/system/rusaren-smoke.service"
    local timer_file="/etc/systemd/system/rusaren-smoke.timer"

    log "installing hosted smoke probe timer"
    cat > "${service_file}" <<EOF
[Unit]
Description=Rusaren hosted smoke probes
After=rusaren-compose.service network-online.target
Wants=network-online.target

[Service]
Type=oneshot
User=${DEPLOY_USER}
Group=${DEPLOY_USER}
Environment=HOME=/home/${DEPLOY_USER}
Environment=PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/snap/bin
WorkingDirectory=${DEPLOY_DIR}
ExecStart=/usr/bin/env bash ${DEPLOY_DIR}/deploy/host-smoke.sh --env-file ${DEPLOY_DIR}/deploy/.env
EOF

    cat > "${timer_file}" <<EOF
[Unit]
Description=Run Rusaren hosted smoke probes on a schedule

[Timer]
OnBootSec=5m
OnUnitActiveSec=${SMOKE_INTERVAL_MINUTES}m
Unit=rusaren-smoke.service

[Install]
WantedBy=timers.target
EOF

    systemctl daemon-reload
    systemctl enable --now rusaren-smoke.timer
}

main() {
    require_value PUBLIC_HOST
    require_value ACME_EMAIL

    if [[ -z "${TURN_PUBLIC_HOST}" ]]; then
        TURN_PUBLIC_HOST="turn.${PUBLIC_HOST}"
    fi

    if [[ -z "${TURN_REALM}" ]]; then
        TURN_REALM="${PUBLIC_HOST}"
    fi

    if [[ -z "${TURN_SHARED_SECRET}" ]]; then
        TURN_SHARED_SECRET="$(openssl rand -hex 32)"
    fi
    if [[ -z "${RARENA_ADMIN_PASSWORD}" ]]; then
        RARENA_ADMIN_PASSWORD="$(openssl rand -base64 24 | tr -d '\n')"
    fi

    if [[ -z "${TURN_EXTERNAL_IP}" ]]; then
        TURN_EXTERNAL_IP="$(detect_public_ipv4)"
    fi

    install_base_packages
    configure_timezone_and_hostname
    ensure_limited_user
    configure_ssh_hardening
    configure_unattended_upgrades
    configure_fail2ban
    configure_firewall
    install_godot_cli
    install_docker
    configure_docker_daemon
    prepare_repo
    write_deploy_env
    install_compose_service
    install_smoke_probe_timer

    if [[ "${RUN_DEPLOY}" == "1" ]]; then
        systemctl restart rusaren-compose.service
    fi

    log "bootstrap complete"
    log "private admin dashboard: https://${PUBLIC_HOST}/adminz (user ${RARENA_ADMIN_USERNAME})"
    log "smoke timer: rusaren-smoke.timer every ${SMOKE_INTERVAL_MINUTES} minute(s)"
    if [[ "${INSTALL_GODOT}" == "1" ]]; then
        log "Godot CLI installed via snap; deploy can now build the web client on-host with server/scripts/export-web-client.sh"
    fi
    log "Cloud Firewall should still be enabled in Linode to restrict SSH source ranges at the network edge"
}

main "$@"
