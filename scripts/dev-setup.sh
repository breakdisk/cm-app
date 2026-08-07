#!/usr/bin/env bash
# ============================================================
# LogisticOS — Developer Environment Setup
# ============================================================
# Sets up a complete local development environment from scratch.
# Run once after cloning the repository.
#
# Usage:
#   chmod +x scripts/dev-setup.sh
#   ./scripts/dev-setup.sh
#
# Requirements:
#   - macOS or Linux (WSL2 on Windows)
#   - Internet connection for downloading tools
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

# ── Logging helpers ──────────────────────────────────────────
info()    { echo -e "${CYAN}[INFO]${RESET}  $*"; }
success() { echo -e "${GREEN}[OK]${RESET}    $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET}  $*"; }
error()   { echo -e "${RED}[ERROR]${RESET} $*" >&2; }
section() { echo -e "\n${BOLD}${CYAN}══════════════════════════════════════════${RESET}"; \
            echo -e "${BOLD}${CYAN}  $*${RESET}"; \
            echo -e "${BOLD}${CYAN}══════════════════════════════════════════${RESET}"; }

# ── Script root (repo root) ──────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "$REPO_ROOT"

# ── Minimum required versions ────────────────────────────────
REQUIRED_NODE_MAJOR=20
REQUIRED_DOCKER_MAJOR=24

section "LogisticOS Dev Setup"
info "Repository root: $REPO_ROOT"
info "Date: $(date)"

# ============================================================
# STEP 1 — Check required system tools
# ============================================================
section "Step 1: Checking required tools"

MISSING_TOOLS=()

check_tool() {
  local CMD="$1"
  local INSTALL_HINT="$2"
  if command -v "$CMD" &>/dev/null; then
    local VER
    VER=$("$CMD" --version 2>&1 | head -1)
    success "$CMD — $VER"
  else
    error "Missing: $CMD — $INSTALL_HINT"
    MISSING_TOOLS+=("$CMD")
  fi
}

check_tool "docker"         "https://docs.docker.com/get-docker/"
check_tool "docker"         "ensure docker compose (v2) is bundled"

# Check docker compose (v2 plugin, not standalone docker-compose)
if docker compose version &>/dev/null 2>&1; then
  success "docker compose (v2) — $(docker compose version 2>&1 | head -1)"
elif command -v docker-compose &>/dev/null; then
  success "docker-compose — $(docker-compose --version)"
else
  error "Missing: docker compose — https://docs.docker.com/compose/install/"
  MISSING_TOOLS+=("docker-compose")
fi

check_tool "cargo"          "https://rustup.rs"
check_tool "rustup"         "https://rustup.rs"
check_tool "kubectl"        "https://kubernetes.io/docs/tasks/tools/"
check_tool "helm"           "https://helm.sh/docs/intro/install/"
check_tool "terraform"      "https://developer.hashicorp.com/terraform/install"
check_tool "pnpm"           "npm install -g pnpm@9"
check_tool "git"            "https://git-scm.com/downloads"

# Node.js version check
if command -v node &>/dev/null; then
  NODE_MAJOR=$(node --version | sed 's/v\([0-9]*\).*/\1/')
  if (( NODE_MAJOR >= REQUIRED_NODE_MAJOR )); then
    success "node — $(node --version)"
  else
    error "node v${NODE_MAJOR} is below required v${REQUIRED_NODE_MAJOR}. Upgrade: https://nodejs.org"
    MISSING_TOOLS+=("node>=20")
  fi
else
  error "Missing: node — https://nodejs.org"
  MISSING_TOOLS+=("node")
fi

if [ ${#MISSING_TOOLS[@]} -gt 0 ]; then
  echo ""
  error "The following required tools are missing:"
  for T in "${MISSING_TOOLS[@]}"; do
    error "  • $T"
  done
  echo ""
  error "Install the missing tools and re-run this script."
  exit 1
fi

success "All required system tools are present."

# ============================================================
# STEP 2 — Rust toolchain components
# ============================================================
section "Step 2: Installing Rust toolchain components"

info "Updating rustup..."
rustup update stable

info "Setting stable as default toolchain..."
rustup default stable

info "Installing required Rust components..."
rustup component add rustfmt clippy

info "Installing cargo-watch (hot reload for development)..."
cargo install cargo-watch --locked --quiet || warn "cargo-watch already installed."

info "Installing cargo-audit (CVE scanner)..."
cargo install cargo-audit --locked --quiet || warn "cargo-audit already installed."

info "Installing cargo-deny (license + bans policy)..."
cargo install cargo-deny --locked --quiet || warn "cargo-deny already installed."

info "Installing sqlx-cli (database migrations)..."
cargo install sqlx-cli \
  --no-default-features \
  --features postgres,rustls \
  --locked \
  --quiet || warn "sqlx-cli already installed."

info "Installing cargo-nextest (faster test runner)..."
cargo install cargo-nextest --locked --quiet || warn "cargo-nextest already installed."

success "Rust toolchain components installed."

# ============================================================
# STEP 3 — Environment configuration
# ============================================================
section "Step 3: Environment configuration"

if [ ! -f "$REPO_ROOT/.env" ]; then
  if [ -f "$REPO_ROOT/.env.example" ]; then
    cp "$REPO_ROOT/.env.example" "$REPO_ROOT/.env"
    success "Created .env from .env.example"
    warn "Review and update .env with your local credentials before running services."
  else
    warn ".env.example not found — skipping .env creation."
  fi
else
  info ".env already exists — skipping copy."
fi

# Load .env for use in subsequent steps
if [ -f "$REPO_ROOT/.env" ]; then
  set -o allexport
  # shellcheck source=/dev/null
  source "$REPO_ROOT/.env" 2>/dev/null || true
  set +o allexport
fi

# ============================================================
# STEP 4 — Start infrastructure services
# ============================================================
section "Step 4: Starting infrastructure services"

info "Pulling latest infrastructure images..."
docker compose --profile infra pull 2>/dev/null || true

info "Starting infrastructure containers (postgres, redis, kafka, zookeeper, clickhouse)..."
docker compose up -d \
  postgres \
  redis \
  zookeeper \
  kafka \
  clickhouse \
  minio \
  mailhog \
  jaeger

success "Infrastructure containers started."

# ── Wait for PostgreSQL ──────────────────────────────────────
info "Waiting for PostgreSQL to be ready..."
POSTGRES_HOST="${POSTGRES_HOST:-localhost}"
POSTGRES_PORT="${POSTGRES_PORT:-5432}"
POSTGRES_USER="${POSTGRES_USER:-logisticos}"

MAX_RETRIES=30
RETRY=0
until pg_isready -h "$POSTGRES_HOST" -p "$POSTGRES_PORT" -U "$POSTGRES_USER" &>/dev/null; do
  RETRY=$((RETRY + 1))
  if [ $RETRY -ge $MAX_RETRIES ]; then
    error "PostgreSQL did not become ready within $((MAX_RETRIES * 2)) seconds."
    error "Check: docker logs logisticos-postgres"
    exit 1
  fi
  echo -n "."
  sleep 2
done
echo ""
success "PostgreSQL is ready."

# ── Wait for Redis ───────────────────────────────────────────
info "Waiting for Redis to be ready..."
REDIS_RETRY=0
until docker exec logisticos-redis redis-cli ping &>/dev/null; do
  REDIS_RETRY=$((REDIS_RETRY + 1))
  if [ $REDIS_RETRY -ge 15 ]; then
    error "Redis did not become ready within 30 seconds."
    error "Check: docker logs logisticos-redis"
    exit 1
  fi
  sleep 2
done
success "Redis is ready."

# ── Wait for Kafka ───────────────────────────────────────────
info "Waiting for Kafka to be ready (this may take ~20s)..."
KAFKA_RETRY=0
until docker exec logisticos-kafka kafka-broker-api-versions \
  --bootstrap-server localhost:29092 &>/dev/null 2>&1; do
  KAFKA_RETRY=$((KAFKA_RETRY + 1))
  if [ $KAFKA_RETRY -ge 20 ]; then
    warn "Kafka may not be ready yet — continuing. Services will retry on connect."
    break
  fi
  sleep 3
done
success "Infrastructure services are up."

# ============================================================
# STEP 5 — Database migrations
# ============================================================
section "Step 5: Running database migrations"

DATABASE_URL="${DATABASE_URL:-postgres://logisticos:password@localhost:5432/logisticos}"
export DATABASE_URL

# Initialize base schema (extensions, shared types)
info "Initializing base schema (PostGIS, pgcrypto, uuid-ossp)..."
psql "$DATABASE_URL" -f scripts/db/init.sql 2>/dev/null || \
  warn "Base schema init skipped (may already exist)."

SERVICES=(
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

MIGRATION_ERRORS=()

for SERVICE in "${SERVICES[@]}"; do
  MIGRATIONS_DIR="$REPO_ROOT/services/$SERVICE/migrations"
  if [ -d "$MIGRATIONS_DIR" ]; then
    info "Running migrations for $SERVICE..."
    if sqlx migrate run \
        --source "$MIGRATIONS_DIR" \
        --database-url "$DATABASE_URL" 2>&1; then
      success "Migrations applied: $SERVICE"
    else
      error "Migration failed: $SERVICE"
      MIGRATION_ERRORS+=("$SERVICE")
    fi
  else
    warn "No migrations directory for $SERVICE — skipping."
  fi
done

if [ ${#MIGRATION_ERRORS[@]} -gt 0 ]; then
  echo ""
  error "Database migrations failed for:"
  for SVC in "${MIGRATION_ERRORS[@]}"; do
    error "  • $SVC"
  done
  error "Fix migration errors before proceeding."
  exit 1
fi

success "All database migrations applied."

# ============================================================
# STEP 6 — Frontend dependencies
# ============================================================
section "Step 6: Installing frontend dependencies"

info "Installing pnpm workspace dependencies..."
pnpm install

success "Frontend dependencies installed."

# ============================================================
# STEP 7 — Create Kafka topics
# ============================================================
section "Step 7: Creating Kafka topics"

KAFKA_TOPICS=(
  "order.created"
  "order.updated"
  "order.cancelled"
  "shipment.assigned"
  "shipment.picked_up"
  "shipment.in_transit"
  "shipment.delivered"
  "shipment.failed"
  "driver.location_update"
  "driver.status_changed"
  "driver.task_completed"
  "payment.cod_collected"
  "payment.invoice_generated"
  "engagement.notification_requested"
  "engagement.notification_sent"
  "engagement.notification_failed"
  "customer.profile_updated"
  "customer.churn_risk_detected"
  "marketing.campaign_triggered"
  "analytics.event_ingested"
  "hub.parcel_received"
  "hub.parcel_sorted"
  "carrier.assigned"
  "fleet.telemetry"
)

KAFKA_CONTAINER="logisticos-kafka"
KAFKA_BOOTSTRAP="localhost:29092"

for TOPIC in "${KAFKA_TOPICS[@]}"; do
  if docker exec "$KAFKA_CONTAINER" kafka-topics \
      --bootstrap-server "$KAFKA_BOOTSTRAP" \
      --list 2>/dev/null | grep -q "^${TOPIC}$"; then
    info "Topic exists: $TOPIC"
  else
    if docker exec "$KAFKA_CONTAINER" kafka-topics \
        --bootstrap-server "$KAFKA_BOOTSTRAP" \
        --create \
        --topic "$TOPIC" \
        --partitions 3 \
        --replication-factor 1 \
        2>/dev/null; then
      success "Created topic: $TOPIC"
    else
      warn "Failed to create topic: $TOPIC (Kafka may still be starting)"
    fi
  fi
done

# ============================================================
# STEP 8 — Create MinIO buckets
# ============================================================
section "Step 8: Initializing MinIO storage buckets"

MINIO_BUCKETS=(
  "pod-evidence"
  "profile-photos"
  "carrier-documents"
  "merchant-uploads"
  "export-reports"
)

for BUCKET in "${MINIO_BUCKETS[@]}"; do
  docker exec logisticos-minio mc mb \
    --ignore-existing \
    "local/$BUCKET" 2>/dev/null || \
  info "MinIO mc not configured — buckets will be created on first use."
  break
done

# ============================================================
# FINAL — Print success summary
# ============================================================
section "Setup Complete!"

echo -e "${GREEN}${BOLD}LogisticOS development environment is ready.${RESET}"
echo ""
echo -e "${BOLD}Service URLs:${RESET}"
echo -e "  ${CYAN}API Gateway:${RESET}      http://localhost:8000"
echo -e "  ${CYAN}Identity:${RESET}         http://localhost:8001"
echo -e "  ${CYAN}CDP:${RESET}              http://localhost:8002"
echo -e "  ${CYAN}Engagement:${RESET}       http://localhost:8003"
echo -e "  ${CYAN}Order Intake:${RESET}     http://localhost:8004"
echo -e "  ${CYAN}Dispatch:${RESET}         http://localhost:8005"
echo -e "  ${CYAN}Driver Ops:${RESET}       http://localhost:8006"
echo -e "  ${CYAN}Delivery Exp:${RESET}     http://localhost:8007"
echo -e "  ${CYAN}Fleet:${RESET}            http://localhost:8008"
echo -e "  ${CYAN}Hub Ops:${RESET}          http://localhost:8009"
echo -e "  ${CYAN}Carrier:${RESET}          http://localhost:8010"
echo -e "  ${CYAN}POD:${RESET}              http://localhost:8011"
echo -e "  ${CYAN}Payments:${RESET}         http://localhost:8012"
echo -e "  ${CYAN}Analytics:${RESET}        http://localhost:8013"
echo -e "  ${CYAN}Marketing:${RESET}        http://localhost:8014"
echo -e "  ${CYAN}Business Logic:${RESET}   http://localhost:8015"
echo -e "  ${CYAN}AI Layer:${RESET}         http://localhost:8016"
echo ""
echo -e "${BOLD}Infrastructure:${RESET}"
echo -e "  ${CYAN}PostgreSQL:${RESET}       localhost:5432"
echo -e "  ${CYAN}Redis:${RESET}            localhost:6379"
echo -e "  ${CYAN}Kafka:${RESET}            localhost:9092"
echo -e "  ${CYAN}ClickHouse:${RESET}       http://localhost:8123"
echo -e "  ${CYAN}MinIO Console:${RESET}    http://localhost:9002  (admin/minioadmin)"
echo -e "  ${CYAN}Mailhog:${RESET}          http://localhost:8025"
echo -e "  ${CYAN}Jaeger (tracing):${RESET} http://localhost:16686"
echo -e "  ${CYAN}Prometheus:${RESET}       http://localhost:9090"
echo -e "  ${CYAN}Grafana:${RESET}          http://localhost:3100  (admin/admin)"
echo ""
echo -e "${BOLD}Dev portal URLs (after 'pnpm dev' in each app):${RESET}"
echo -e "  ${CYAN}Merchant Portal:${RESET}  http://localhost:3000"
echo -e "  ${CYAN}Admin Portal:${RESET}     http://localhost:3001"
echo -e "  ${CYAN}Partner Portal:${RESET}   http://localhost:3002"
echo -e "  ${CYAN}Customer Portal:${RESET}  http://localhost:3003"
echo ""
echo -e "${BOLD}Optional developer tools (start with: docker compose --profile tools up -d):${RESET}"
echo -e "  ${CYAN}pgAdmin:${RESET}          http://localhost:5050  (admin@logisticos.dev/admin)"
echo -e "  ${CYAN}Kafka UI:${RESET}         http://localhost:9093"
echo -e "  ${CYAN}Redis Insight:${RESET}    http://localhost:5540"
echo ""
echo -e "${BOLD}Useful commands:${RESET}"
echo -e "  ${YELLOW}cargo watch -x 'run --bin identity'${RESET}   # Hot-reload a service"
echo -e "  ${YELLOW}./scripts/seed-dev.sh${RESET}                  # Seed development data"
echo -e "  ${YELLOW}./scripts/migrate.sh all${RESET}               # Re-run all migrations"
echo -e "  ${YELLOW}docker compose logs -f identity${RESET}        # Tail service logs"
echo ""
success "Happy coding! 🚀"
