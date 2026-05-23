#!/usr/bin/env bash
# Install exact-runner on a Linux host (typically a Raspberry Pi).
#
# Run this *on the Pi*, as a user with sudo. Prerequisites:
#   1. The cross-built binary copied here (e.g. via scp from the dev box —
#      see scripts/build-runner-aarch64.sh).
#   2. A device + per-runner token already provisioned in the admin UI;
#      the raw token saved to a local file (shown once at creation).
#
# Idempotent: re-running upgrades the binary + rewrites the unit, but
# does not regenerate the service user, the token file, or restart unless
# something changed.

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: runner-install.sh [options]

Required:
  --binary PATH         Path to the cross-built exact-runner binary
  --backend-url URL     e.g. wss://exact.run/api/runner/ws
  --device-id ID        Stable device id registered in the admin UI
  --serial-port PATH    e.g. /dev/ttyACM0
  --token-file PATH     Path to the one-shot token file (root-readable)

Optional:
  --board NAME          lpc1768 (default) | stm32f429zi | lm3s6965evb
  --cclk-hz N           Override declared core clock (Hz)
  --baud N              Serial baud (default 115200)
  --service-user USER   Service user to create (default exact-runner)
  --dry-run             Print what would happen, do nothing

Environment:
  Same names in CAPS work too (BACKEND_URL, DEVICE_ID, ...).
EOF
}

# Defaults.
SERVICE_USER="${SERVICE_USER:-exact-runner}"
BOARD="${BOARD:-lpc1768}"
CCLK_HZ="${CCLK_HZ:-}"
BAUD="${BAUD:-115200}"
BINARY="${BINARY:-}"
BACKEND_URL="${BACKEND_URL:-}"
DEVICE_ID="${DEVICE_ID:-}"
SERIAL_PORT="${SERIAL_PORT:-}"
TOKEN_FILE="${TOKEN_FILE:-}"
DRY=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --binary) BINARY="$2"; shift 2 ;;
    --backend-url) BACKEND_URL="$2"; shift 2 ;;
    --device-id) DEVICE_ID="$2"; shift 2 ;;
    --serial-port) SERIAL_PORT="$2"; shift 2 ;;
    --token-file) TOKEN_FILE="$2"; shift 2 ;;
    --board) BOARD="$2"; shift 2 ;;
    --cclk-hz) CCLK_HZ="$2"; shift 2 ;;
    --baud) BAUD="$2"; shift 2 ;;
    --service-user) SERVICE_USER="$2"; shift 2 ;;
    --dry-run) DRY=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown arg: $1" >&2; usage >&2; exit 2 ;;
  esac
done

die() { echo "error: $*" >&2; exit 1; }

# Validate inputs.
[[ -n "${BINARY}"        ]] || die "--binary is required"
[[ -n "${BACKEND_URL}"   ]] || die "--backend-url is required"
[[ -n "${DEVICE_ID}"     ]] || die "--device-id is required"
[[ -n "${SERIAL_PORT}"   ]] || die "--serial-port is required"
[[ -n "${TOKEN_FILE}"    ]] || die "--token-file is required"
[[ -f "${BINARY}"        ]] || die "binary not found: ${BINARY}"
[[ -f "${TOKEN_FILE}"    ]] || die "token file not found: ${TOKEN_FILE}"

# Fill in a sane cclk default per board if the caller didn't override.
if [[ -z "${CCLK_HZ}" ]]; then
  case "${BOARD}" in
    lpc1768)      CCLK_HZ=96000000 ;;
    stm32f429zi)  CCLK_HZ=168000000 ;;
    lm3s6965evb)  CCLK_HZ=12000000 ;;
    *) die "unknown --board ${BOARD} (no default cclk)" ;;
  esac
fi

if [[ "${DRY}" -eq 1 ]]; then
  SUDO="echo +"
else
  SUDO="sudo"
  if [[ "$(id -u)" -ne 0 ]] && ! command -v sudo >/dev/null 2>&1; then
    die "this script needs sudo (or run as root)"
  fi
fi

echo "==> create service user: ${SERVICE_USER}"
if id -u "${SERVICE_USER}" >/dev/null 2>&1; then
  echo "    already exists"
else
  ${SUDO} useradd --system --no-create-home --shell /usr/sbin/nologin "${SERVICE_USER}"
fi

# Make sure the service user can open the serial port. On Debian/Ubuntu/
# Raspberry Pi OS the standard group for tty devices is 'dialout'.
if getent group dialout >/dev/null 2>&1; then
  ${SUDO} usermod -aG dialout "${SERVICE_USER}" || true
fi

echo "==> install binary: /usr/local/bin/exact-runner"
${SUDO} install -m 0755 -o root -g root "${BINARY}" /usr/local/bin/exact-runner

echo "==> install token: /etc/exact-runner/token"
${SUDO} install -d -m 0750 -o root -g "${SERVICE_USER}" /etc/exact-runner
${SUDO} install -m 0440 -o root -g "${SERVICE_USER}" "${TOKEN_FILE}" /etc/exact-runner/token

UNIT_TMPL="$(dirname "$0")/exact-runner.service.in"
[[ -f "${UNIT_TMPL}" ]] || die "missing template: ${UNIT_TMPL}"

echo "==> render systemd unit: /etc/systemd/system/exact-runner.service"
TMP_UNIT="$(mktemp)"
trap 'rm -f "${TMP_UNIT}"' EXIT
sed \
  -e "s|\${BACKEND_URL}|${BACKEND_URL}|g" \
  -e "s|\${DEVICE_ID}|${DEVICE_ID}|g" \
  -e "s|\${SERIAL_PORT}|${SERIAL_PORT}|g" \
  -e "s|\${BOARD}|${BOARD}|g" \
  -e "s|\${CCLK_HZ}|${CCLK_HZ}|g" \
  -e "s|\${BAUD}|${BAUD}|g" \
  -e "s|^User=exact-runner$|User=${SERVICE_USER}|" \
  -e "s|^Group=exact-runner$|Group=${SERVICE_USER}|" \
  "${UNIT_TMPL}" > "${TMP_UNIT}"

${SUDO} install -m 0644 -o root -g root "${TMP_UNIT}" /etc/systemd/system/exact-runner.service

echo "==> systemctl daemon-reload + enable + restart"
${SUDO} systemctl daemon-reload
${SUDO} systemctl enable exact-runner.service
${SUDO} systemctl restart exact-runner.service

echo
echo "Installed. Useful commands:"
echo "  systemctl status exact-runner"
echo "  journalctl -u exact-runner -f"
