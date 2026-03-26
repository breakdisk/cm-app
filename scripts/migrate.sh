#!/usr/bin/env bash
# ============================================================
# LogisticOS — Database Migration Runner
# ============================================================
# Runs or reverts SQLx migrations for one or all services.
#
# Usage:
#   ./scripts/migrate.sh <service|all> [--down] [--dry-run]
#
# Arguments:
#   service     One of the 17 service names (e.g., identity, dispatch)
#               or "all" to migrate every service in dependency order
#
# Options:
#   --down      Revert the latest migration (sqlx migrate revert)
#   --dry-run   Print what would run without executing anything
#
# Examples:
#   ./scripts/migrate.sh all                # Migrate all services up
#   ./scripts/migrate.sh identity           # Migrate only identity service
#   ./scripts/migrate.sh dispatch --down    # Revert latest dispatch migration
#   ./scripts/migrate.sh all --dry-run      # Preview all pending migrations
#
# Environment:
#   DATABASE_URL  PostgreSQL connection string (required)
#                 Falls back to: postgres://logisticos:password@localhost:5432/logisticos
# ============================================================

set -euo pipefail
IFS=$'\n\t'

# ── Colors ──────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
RESET='\033[0m'

# ── Logging ──────────────────────────────────────────────────
info()    { echo -e "${CYAN}[INFO]${RESET}  $*" | tee -a "$LOG_FILE"; }
success() { echo -e "${GREEN}[OK]${RESET}    $*" | tee -a "$LOG_FILE"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET}  $*" | tee -a "$LOG_FILE"; }
error()   { echo -e "${RED}[ERROR]${RESET} $*" | tee -a "$LOG_FILE" >&2; }

# ── Log file ─────────────────────────────────────────────────
LOG_FILE="/tmp/logisticos-migrate.log"
echo "=== Migration run started at $(date) ===" >> "$LOG_FILE"

# ── Script root ───────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# ── Argument parsing ─────────────────────────────────────────
TARGET_SERVICE=""
DO_DOWN=false
DRY_RUN=false

if [ $# -lt 1 ]; then
  echo -e "${BOLD}Usage:${RESET} ./scripts/migrate.sh <service|all> [--down] [--dry-run]"
  echo ""
  echo -e "${BOLD}Available services:${RESET}"
  echo "  all | identity | cdp | engagement | order-intake | dispatch |"
  echo "  driver-ops | delivery-experience | fleet | hub-ops | carrier |"
  echo "  pod | payments | analytics | marketing | business-logic | ai-layer | api-gateway"
  exit 1
fi

TARGET_SERVICE="$1"
shift

while [ $# -gt 0 ]; do
  case "$1" in
    --down)     DO_DOWN=true ;;
    --dry-run)  DRY_RUN=true ;;
    *)
      error "Unknown option: $1"
      exit 1
      ;;
  esac
  shift
done

# ── Database URL ─────────────────────────────────────────────
DATABASE_URL="${DATABASE_URL:-postgres://logisticos:password@localhost:5432/logisticos}"
export DATABASE_URL

# ── Services in dependency order ─────────────────────────────
# Identity must run first (other services reference tenant/user tables).
# CDP runs after identity. Etc.
ALL_SERVICES=(
  identity
  cdp
  engagement
  order-intake
  dispatch
  driver-ops
  delivery-experience
  fleet
  hub-ops
  carrier
  pod
  payments
  analytics
  marketing
  business-logic
  ai-layer
  api-gateway
)

# Reverse order for --down to avoid FK violations
ALL_SERVICES_DOWN=(
  api-gateway
  ai-layer
  business-logic
  marketing
  analytics
  payments
  pod
  carrier
  hub-ops
  fleet
  delivery-experience
  driver-ops
  dispatch
  order-intake
  engagement
  cdp
  identity
)

# ── Validate target service ───────────────────────────────────
validate_service() {
  local SVC="$1"
  if [ "$SVC" == "all" ]; then
    return 0
  fi
  for VALID in "${ALL_SERVICES[@]}"; do
    if [ "$SVC" == "$VALID" ]; then
      return 0
    fi
  done
  error "Unknown service: '$SVC'"
  echo -e "Valid services: ${ALL_SERVICES[*]}"
  exit 1
}

validate_service "$TARGET_SERVICE"

# ── Check sqlx-cli is installed ───────────────────────────────
if ! command -v sqlx &>/dev/null; then
  error "sqlx-cli is not installed."
  error "Install with: cargo install sqlx-cli --no-default-features --features postgres,rustls --locked"
  exit 1
fi

# ── Check DATABASE_URL is reachable ──────────────────────────
info "Testing database connectivity..."
if ! pg_isready -d "$DATABASE_URL" &>/dev/null; then
  # pg_isready may not accept connection strings — try psql instead
  if ! psql "$DATABASE_URL" -c "SELECT 1" &>/dev/null 2>&1; then
    error "Cannot connect to database: $DATABASE_URL"
    error "Ensure PostgreSQL is running (docker compose up postgres) and DATABASE_URL is set correctly."
    exit 1
  fi
fi
success "Database connection OK."

# ── Migration runner ──────────────────────────────────────────
run_migration() {
  local SERVICE="$1"
  local MIGRATIONS_DIR="$REPO_ROOT/services/$SERVICE/migrations"

  if [ ! -d "$MIGRATIONS_DIR" ]; then
    warn "No migrations directory for $SERVICE ($MIGRATIONS_DIR) — skipping."
    return 0
  fi

  # Count pending migrations
  local PENDING
  PENDING=$(sqlx migrate info \
    --source "$MIGRATIONS_DIR" \
    --database-url "$DATABASE_URL" 2>/dev/null \
    | grep "pending\|not applied" | wc -l || echo "0")

  if [ "$DO_DOWN" == "true" ]; then
    ACTION="revert"
    SQLX_CMD="migrate revert"
  else
    ACTION="apply"
    SQLX_CMD="migrate run"
  fi

  if [ "$DRY_RUN" == "true" ]; then
    echo -e "${YELLOW}[DRY-RUN]${RESET} Would ${ACTION} migrations for ${BOLD}$SERVICE${RESET}"
    info "  Source: $MIGRATIONS_DIR"
    info "  Pending: $PENDING"
    # Show migration files
    if [ "$DO_DOWN" == "false" ]; then
      sqlx migrate info \
        --source "$MIGRATIONS_DIR" \
        --database-url "$DATABASE_URL" 2>/dev/null || true
    fi
    return 0
  fi

  info "Running '${SQLX_CMD}' for ${BOLD}$SERVICE${RESET}..."
  info "  Source:      $MIGRATIONS_DIR"
  info "  Database:    ${DATABASE_URL%%@*}@…"

  local START_TIME=$SECONDS

  if sqlx $SQLX_CMD \
      --source "$MIGRATIONS_DIR" \
      --database-url "$DATABASE_URL" 2>&1 | tee -a "$LOG_FILE"; then
    local ELAPSED=$(( SECONDS - START_TIME ))
    success "Migration ${ACTION}d for $SERVICE (${ELAPSED}s)"
    echo "$SERVICE: ${ACTION} OK at $(date)" >> "$LOG_FILE"
    return 0
  else
    error "Migration FAILED for $SERVICE"
    echo "$SERVICE: ${ACTION} FAILED at $(date)" >> "$LOG_FILE"
    return 1
  fi
}

# ── Main execution ────────────────────────────────────────────
FAILED_SERVICES=()
SKIPPED_SERVICES=()
SUCCESS_SERVICES=()

echo ""
echo -e "${BOLD}Migration run${RESET}"
echo -e "  Target:   ${BOLD}$TARGET_SERVICE${RESET}"
echo -e "  Mode:     $([ "$DO_DOWN" == "true" ] && echo "DOWN (revert)" || echo "UP (apply)")"
echo -e "  Dry run:  $DRY_RUN"
echo -e "  Log:      $LOG_FILE"
echo ""

if [ "$TARGET_SERVICE" == "all" ]; then
  if [ "$DO_DOWN" == "true" ]; then
    SERVICES_TO_RUN=("${ALL_SERVICES_DOWN[@]}")
  else
    SERVICES_TO_RUN=("${ALL_SERVICES[@]}")
  fi
else
  SERVICES_TO_RUN=("$TARGET_SERVICE")
fi

for SERVICE in "${SERVICES_TO_RUN[@]}"; do
  echo -e "─────────────────────────────────────────"
  if run_migration "$SERVICE"; then
    # Check if it was a real success or a skip
    MIGRATIONS_DIR="$REPO_ROOT/services/$SERVICE/migrations"
    if [ -d "$MIGRATIONS_DIR" ]; then
      SUCCESS_SERVICES+=("$SERVICE")
    else
      SKIPPED_SERVICES+=("$SERVICE")
    fi
  else
    FAILED_SERVICES+=("$SERVICE")
    if [ "$TARGET_SERVICE" != "all" ]; then
      # Single service — exit immediately on failure
      exit 1
    fi
    # In "all" mode, continue and report all failures at end
  fi
done

# ── Summary ───────────────────────────────────────────────────
echo ""
echo -e "─────────────────────────────────────────"
echo -e "${BOLD}Migration Summary${RESET}"
echo -e "─────────────────────────────────────────"

if [ ${#SUCCESS_SERVICES[@]} -gt 0 ]; then
  echo -e "${GREEN}Succeeded (${#SUCCESS_SERVICES[@]}):${RESET}"
  for SVC in "${SUCCESS_SERVICES[@]}"; do
    echo -e "  ${GREEN}✓${RESET} $SVC"
  done
fi

if [ ${#SKIPPED_SERVICES[@]} -gt 0 ]; then
  echo -e "${YELLOW}Skipped (no migrations) (${#SKIPPED_SERVICES[@]}):${RESET}"
  for SVC in "${SKIPPED_SERVICES[@]}"; do
    echo -e "  ${YELLOW}–${RESET} $SVC"
  done
fi

if [ ${#FAILED_SERVICES[@]} -gt 0 ]; then
  echo -e "${RED}Failed (${#FAILED_SERVICES[@]}):${RESET}"
  for SVC in "${FAILED_SERVICES[@]}"; do
    echo -e "  ${RED}✗${RESET} $SVC"
  done
  echo ""
  error "One or more migrations failed. Check log: $LOG_FILE"
  exit 1
fi

echo ""
if [ "$DRY_RUN" == "true" ]; then
  success "Dry run complete. No changes were applied."
else
  success "All migrations completed successfully."
fi

echo "=== Migration run completed at $(date) ===" >> "$LOG_FILE"
