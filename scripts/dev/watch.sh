#!/usr/bin/env bash
# =============================================================================
# LogisticOS — Hot-reload dev watcher
# =============================================================================
# Wraps `cargo watch` for any of the 17 backend microservices.
#
# Usage:
#   ./scripts/dev/watch.sh                   # interactive menu
#   ./scripts/dev/watch.sh identity          # start watcher for 'identity'
#   ./scripts/dev/watch.sh dispatch          # start watcher for 'dispatch'
#
# Requirements:
#   cargo-watch   → cargo install cargo-watch
#
# Port resolution order:
#   1. $SERVICE_PORT env var
#   2. .env or services/<name>/.env  (PORT=XXXX)
#   3. Hard-coded defaults in SERVICE_PORTS map below
# =============================================================================

set -euo pipefail

# ─── Color helpers ────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
PURPLE='\033[0;35m'
BOLD='\033[1m'
DIM='\033[2m'
RESET='\033[0m'

log_info()  { echo -e "${CYAN}[INFO]${RESET}  $*"; }
log_ok()    { echo -e "${GREEN}[ OK ]${RESET}  $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${RESET}  $*"; }
log_error() { echo -e "${RED}[ERR ]${RESET}  $*" >&2; }

# ─── Service registry ─────────────────────────────────────────────────────────
# Format: "service-dir:binary-name:default-port:description"
# The binary-name matches the [[bin]] name in the service's Cargo.toml.
declare -a SERVICES=(
  "identity:identity-service:8001:Identity & Tenant Management"
  "cdp:cdp-service:8002:Customer Data Platform"
  "engagement:engagement-service:8003:Unified Engagement Engine"
  "order-intake:order-intake-service:8004:Order & Shipment Intake"
  "dispatch:dispatch-service:8005:Dispatch & Routing"
  "driver-ops:driver-ops-service:8006:Driver Operations"
  "delivery-experience:delivery-experience-service:8007:Customer Delivery Experience"
  "fleet:fleet-service:8008:Fleet Management"
  "hub-ops:hub-ops-service:8009:Warehouse & Hub Operations"
  "carrier:carrier-service:8010:Carrier & Partner Management"
  "pod:pod-service:8011:Proof of Delivery"
  "payments:payments-service:8012:Payments & Billing"
  "analytics:analytics-service:8013:Analytics & BI"
  "marketing:marketing-service:8014:Marketing Automation Engine"
  "business-logic:business-logic-service:8015:Business Logic & Automation Engine"
  "ai-layer:ai-layer-service:8016:AI Intelligence Layer"
  "api-gateway:api-gateway-service:8017:API Gateway & Integration Layer"
)

# ─── Helpers ──────────────────────────────────────────────────────────────────
# Returns all valid service directory names
valid_service_names() {
  for entry in "${SERVICES[@]}"; do
    IFS=':' read -r dir _bin _port _desc <<< "$entry"
    echo "$dir"
  done
}

# Lookup entry by service dir name
find_service_entry() {
  local name="$1"
  for entry in "${SERVICES[@]}"; do
    IFS=':' read -r dir _bin _port _desc <<< "$entry"
    if [[ "$dir" == "$name" ]]; then
      echo "$entry"
      return 0
    fi
  done
  return 1
}

# Resolve the port for a service
resolve_port() {
  local service_dir="$1"
  local default_port="$2"

  # 1. Explicit env var
  if [[ -n "${SERVICE_PORT:-}" ]]; then
    echo "${SERVICE_PORT}"
    return
  fi

  # 2. Service-level .env
  local service_env="services/${service_dir}/.env"
  if [[ -f "$service_env" ]]; then
    local port_from_env
    port_from_env=$(grep -E '^PORT=' "$service_env" 2>/dev/null | head -n1 | cut -d= -f2 | tr -d '[:space:]"' || true)
    if [[ -n "$port_from_env" ]]; then
      echo "$port_from_env"
      return
    fi
  fi

  # 3. Root .env
  if [[ -f ".env" ]]; then
    local upper_name
    upper_name=$(echo "$service_dir" | tr '[:lower:]-' '[:upper:]_')
    local port_from_root
    port_from_root=$(grep -E "^${upper_name}_PORT=" ".env" 2>/dev/null | head -n1 | cut -d= -f2 | tr -d '[:space:]"' || true)
    if [[ -n "$port_from_root" ]]; then
      echo "$port_from_root"
      return
    fi
  fi

  # 4. Default
  echo "$default_port"
}

# ─── Prerequisite checks ──────────────────────────────────────────────────────
check_prerequisites() {
  if ! command -v cargo &>/dev/null; then
    log_error "cargo not found. Install Rust: https://rustup.rs"
    exit 1
  fi

  if ! cargo watch --version &>/dev/null 2>&1; then
    log_warn "cargo-watch not installed."
    echo -e "  Install it with: ${CYAN}cargo install cargo-watch${RESET}"
    read -rp "  Install now? [y/N] " answer
    if [[ "$answer" =~ ^[Yy]$ ]]; then
      cargo install cargo-watch
    else
      log_error "cargo-watch is required. Aborting."
      exit 1
    fi
  fi
}

# ─── Interactive menu ─────────────────────────────────────────────────────────
show_menu() {
  echo ""
  echo -e "${BOLD}${PURPLE}╔════════════════════════════════════════════════╗${RESET}"
  echo -e "${BOLD}${PURPLE}║  LogisticOS — Dev Service Watcher              ║${RESET}"
  echo -e "${BOLD}${PURPLE}╚════════════════════════════════════════════════╝${RESET}"
  echo ""
  echo -e "  Select a service to watch (${DIM}Ctrl+C to exit${RESET}):"
  echo ""

  local i=1
  for entry in "${SERVICES[@]}"; do
    IFS=':' read -r dir _bin port desc <<< "$entry"
    printf "  ${CYAN}%2d.${RESET}  ${BOLD}%-28s${RESET} ${DIM}:%s${RESET}  %s\n" \
      "$i" "$dir" "$port" "$desc"
    (( i++ )) || true
  done

  echo ""
  echo -n "  Enter number or service name: "
  read -r selection

  # Numeric selection
  if [[ "$selection" =~ ^[0-9]+$ ]]; then
    local idx=$(( selection - 1 ))
    local total=${#SERVICES[@]}
    if (( idx < 0 || idx >= total )); then
      log_error "Invalid selection: $selection"
      exit 1
    fi
    IFS=':' read -r selected_dir _b _p _d <<< "${SERVICES[$idx]}"
    echo "$selected_dir"
  else
    # Name selection
    echo "$selection"
  fi
}

# ─── Watcher ──────────────────────────────────────────────────────────────────
start_watcher() {
  local service_dir="$1"

  # Validate service name
  local entry
  if ! entry=$(find_service_entry "$service_dir"); then
    log_error "Unknown service: '${service_dir}'"
    echo ""
    echo -e "  Valid services:"
    valid_service_names | while read -r name; do
      echo -e "    ${DIM}•${RESET} $name"
    done
    exit 1
  fi

  IFS=':' read -r dir bin_name default_port desc <<< "$entry"

  # Check service directory exists
  if [[ ! -d "services/${dir}" ]]; then
    log_error "Service directory not found: services/${dir}"
    log_error "Make sure you are running this script from the repository root."
    exit 1
  fi

  local port
  port=$(resolve_port "$dir" "$default_port")

  # ── Banner ────────────────────────────────────────────────────────────────
  echo ""
  echo -e "${BOLD}${CYAN}╔════════════════════════════════════════════════╗${RESET}"
  echo -e "${BOLD}${CYAN}║  LogisticOS — Hot-reload Watcher               ║${RESET}"
  echo -e "${BOLD}${CYAN}╚════════════════════════════════════════════════╝${RESET}"
  echo ""
  echo -e "  ${BOLD}Service  :${RESET} ${CYAN}${desc}${RESET}"
  echo -e "  ${BOLD}Directory:${RESET} services/${dir}/"
  echo -e "  ${BOLD}Binary   :${RESET} ${bin_name}"
  echo -e "  ${BOLD}Port     :${RESET} ${GREEN}${port}${RESET}"
  echo -e "  ${BOLD}Watch    :${RESET} services/${dir}/src/**  libs/**  Cargo.toml"
  echo ""
  echo -e "  ${DIM}Press Ctrl+C to stop${RESET}"
  echo ""

  # ── Trap SIGINT for clean exit ────────────────────────────────────────────
  trap 'echo -e "\n\n${YELLOW}  Watcher stopped.${RESET}\n"; exit 0' INT TERM

  # ── Launch cargo watch ────────────────────────────────────────────────────
  cargo watch \
    --exec "run --bin ${bin_name}" \
    --watch "services/${dir}/src" \
    --watch "libs/" \
    --watch "Cargo.toml" \
    --watch "Cargo.lock" \
    --why \
    --clear
}

# ─── Entry point ──────────────────────────────────────────────────────────────
main() {
  # Must run from repo root
  if [[ ! -f "Cargo.toml" ]]; then
    log_error "Cargo.toml not found. Run this script from the repository root."
    exit 1
  fi

  check_prerequisites

  local service_name=""

  if [[ $# -eq 0 ]]; then
    service_name=$(show_menu)
  elif [[ $# -eq 1 ]]; then
    service_name="$1"
  else
    log_error "Too many arguments."
    echo "Usage: $0 [service-name]"
    exit 1
  fi

  # Strip whitespace
  service_name="${service_name// /}"

  if [[ -z "$service_name" ]]; then
    log_error "No service selected."
    exit 1
  fi

  start_watcher "$service_name"
}

main "$@"
