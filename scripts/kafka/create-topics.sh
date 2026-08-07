#!/usr/bin/env bash
# =============================================================================
# LogisticOS — Kafka Topic Provisioner
# =============================================================================
# Creates all platform Kafka topics for local-dev or Kubernetes environments.
#
# Usage:
#   ./scripts/kafka/create-topics.sh           # auto-detect environment
#   ENV=prod ./scripts/kafka/create-topics.sh  # production replication factor
#
# Environment detection:
#   1. If `kubectl` is in PATH and KUBECONFIG points to a reachable cluster
#      → uses: kubectl exec -n <KAFKA_NAMESPACE> <kafka-pod> -- kafka-topics.sh
#   2. Otherwise
#      → uses: docker exec <KAFKA_CONTAINER> kafka-topics.sh
#
# Tuning via env vars:
#   KAFKA_NAMESPACE    k8s namespace for kafka pod    (default: logisticos)
#   KAFKA_CONTAINER    docker container name/id       (default: logisticos-kafka)
#   BOOTSTRAP_SERVER   kafka bootstrap address        (default: localhost:9092)
#   ENV                "prod" for RF=3, else RF=1     (default: dev)
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

log_info()    { echo -e "${CYAN}[INFO]${RESET}  $*"; }
log_ok()      { echo -e "${GREEN}[OK]${RESET}    $*"; }
log_warn()    { echo -e "${YELLOW}[WARN]${RESET}  $*"; }
log_error()   { echo -e "${RED}[ERROR]${RESET} $*" >&2; }
log_section() { echo -e "\n${BOLD}${PURPLE}▶ $*${RESET}"; }

# ─── Configuration ────────────────────────────────────────────────────────────
KAFKA_NAMESPACE="${KAFKA_NAMESPACE:-logisticos}"
KAFKA_CONTAINER="${KAFKA_CONTAINER:-logisticos-kafka}"
BOOTSTRAP_SERVER="${BOOTSTRAP_SERVER:-localhost:9092}"
ENV="${ENV:-dev}"

if [[ "$ENV" == "prod" ]]; then
  DEFAULT_RF=3
  log_info "Environment: ${BOLD}PRODUCTION${RESET} — replication factor = 3"
else
  DEFAULT_RF=1
  log_info "Environment: ${BOLD}DEV/STAGING${RESET} — replication factor = 1"
fi

RETENTION_7D=$((7 * 24 * 60 * 60 * 1000))      # 7 days in ms
RETENTION_30D=$((30 * 24 * 60 * 60 * 1000))     # 30 days in ms

# ─── Environment detection ────────────────────────────────────────────────────
USE_KUBECTL=false
KAFKA_POD=""

detect_environment() {
  log_section "Detecting execution environment"

  if command -v kubectl &>/dev/null; then
    log_info "kubectl found — checking cluster connectivity…"

    if kubectl cluster-info --request-timeout=3s &>/dev/null; then
      log_info "Cluster reachable. Looking for Kafka pod in namespace '${KAFKA_NAMESPACE}'…"

      KAFKA_POD=$(
        kubectl get pods -n "${KAFKA_NAMESPACE}" \
          --field-selector=status.phase=Running \
          -l app.kubernetes.io/name=kafka \
          -o jsonpath='{.items[0].metadata.name}' 2>/dev/null || true
      )

      # Fallback: any pod with "kafka" in the name
      if [[ -z "$KAFKA_POD" ]]; then
        KAFKA_POD=$(
          kubectl get pods -n "${KAFKA_NAMESPACE}" \
            -o jsonpath='{range .items[*]}{.metadata.name}{"\n"}{end}' 2>/dev/null \
          | grep -i kafka | head -n1 || true
        )
      fi

      if [[ -n "$KAFKA_POD" ]]; then
        USE_KUBECTL=true
        log_ok "Using Kubernetes: pod '${KAFKA_POD}' in namespace '${KAFKA_NAMESPACE}'"
        return
      else
        log_warn "No running Kafka pod found in namespace '${KAFKA_NAMESPACE}'. Falling back to Docker."
      fi
    else
      log_warn "kubectl present but cluster unreachable. Falling back to Docker."
    fi
  fi

  # Docker fallback
  if ! docker inspect "${KAFKA_CONTAINER}" &>/dev/null; then
    log_error "Docker container '${KAFKA_CONTAINER}' not found."
    log_error "Start the stack with: docker-compose up -d kafka"
    exit 1
  fi

  log_ok "Using Docker container: '${KAFKA_CONTAINER}'"
}

# ─── Core exec wrapper ────────────────────────────────────────────────────────
kafka_topics() {
  # Passes all arguments to kafka-topics.sh inside the appropriate runtime
  if [[ "$USE_KUBECTL" == "true" ]]; then
    kubectl exec -n "${KAFKA_NAMESPACE}" "${KAFKA_POD}" -- \
      kafka-topics.sh "$@"
  else
    docker exec "${KAFKA_CONTAINER}" \
      kafka-topics.sh "$@"
  fi
}

# ─── Topic creation helper ────────────────────────────────────────────────────
CREATED_COUNT=0
SKIPPED_COUNT=0
FAILED_TOPICS=()

create_topic() {
  local name="$1"
  local partitions="$2"
  local rf="${3:-$DEFAULT_RF}"
  local retention_ms="${4:-$RETENTION_7D}"

  # Check if topic already exists
  local existing
  existing=$(
    kafka_topics \
      --bootstrap-server "${BOOTSTRAP_SERVER}" \
      --list 2>/dev/null \
    | grep -x "${name}" || true
  )

  if [[ -n "$existing" ]]; then
    echo -e "  ${DIM}↩  ${name} (already exists)${RESET}"
    (( SKIPPED_COUNT++ )) || true
    return
  fi

  if kafka_topics \
      --bootstrap-server "${BOOTSTRAP_SERVER}" \
      --create \
      --topic "${name}" \
      --partitions "${partitions}" \
      --replication-factor "${rf}" \
      --config "retention.ms=${retention_ms}" \
      2>/dev/null; then
    echo -e "  ${GREEN}✓${RESET}  ${name}"
    (( CREATED_COUNT++ )) || true
  else
    echo -e "  ${RED}✗${RESET}  ${name} ${RED}(FAILED)${RESET}"
    FAILED_TOPICS+=("$name")
  fi
}

# ─── Main ─────────────────────────────────────────────────────────────────────
detect_environment

# ── Standard topics (3 partitions, 7-day retention) ──────────────────────────
log_section "Standard topics  [3 partitions | ${DEFAULT_RF}x RF | 7d retention]"
create_topic "logisticos.identity.tenant.created"      3 "$DEFAULT_RF" "$RETENTION_7D"
create_topic "logisticos.order.shipment.created"       3 "$DEFAULT_RF" "$RETENTION_7D"
create_topic "logisticos.order.shipment.confirmed"     3 "$DEFAULT_RF" "$RETENTION_7D"
create_topic "logisticos.order.shipment.cancelled"     3 "$DEFAULT_RF" "$RETENTION_7D"
create_topic "logisticos.dispatch.route.created"       3 "$DEFAULT_RF" "$RETENTION_7D"
create_topic "logisticos.dispatch.driver.assigned"     3 "$DEFAULT_RF" "$RETENTION_7D"
create_topic "logisticos.driver.pickup.completed"      3 "$DEFAULT_RF" "$RETENTION_7D"
create_topic "logisticos.driver.delivery.completed"    3 "$DEFAULT_RF" "$RETENTION_7D"
create_topic "logisticos.driver.delivery.failed"       3 "$DEFAULT_RF" "$RETENTION_7D"
create_topic "logisticos.pod.captured"                 3 "$DEFAULT_RF" "$RETENTION_7D"
create_topic "logisticos.payments.cod.collected"       3 "$DEFAULT_RF" "$RETENTION_7D"
create_topic "logisticos.payments.invoice.generated"   3 "$DEFAULT_RF" "$RETENTION_7D"
create_topic "logisticos.hub.parcel.inducted"          3 "$DEFAULT_RF" "$RETENTION_7D"
create_topic "logisticos.carrier.shipment.allocated"   3 "$DEFAULT_RF" "$RETENTION_7D"

# ── High-volume topics (12 partitions, 7-day retention) ──────────────────────
log_section "High-volume topics  [12 partitions | ${DEFAULT_RF}x RF | 7d retention]"
create_topic "logisticos.driver.location.updated"  12 "$DEFAULT_RF" "$RETENTION_7D"
create_topic "logisticos.notification.outbound"    12 "$DEFAULT_RF" "$RETENTION_7D"

# ── Analytics topics (12 partitions, 30-day retention) ───────────────────────
log_section "Analytics topics  [12 partitions | ${DEFAULT_RF}x RF | 30d retention]"
create_topic "logisticos.analytics.events"  12 "$DEFAULT_RF" "$RETENTION_30D"

# ── DLQ topics (3 partitions, 30-day retention) ───────────────────────────────
log_section "Dead Letter Queue topics  [3 partitions | ${DEFAULT_RF}x RF | 30d retention]"
create_topic "logisticos.notification.outbound.dlq"  3 "$DEFAULT_RF" "$RETENTION_30D"
create_topic "logisticos.order.shipment.created.dlq" 3 "$DEFAULT_RF" "$RETENTION_30D"
create_topic "logisticos.payments.cod.collected.dlq" 3 "$DEFAULT_RF" "$RETENTION_30D"

# ── Agent trigger topics (6 partitions, 7-day retention) ─────────────────────
log_section "AI Agent trigger topics  [6 partitions | ${DEFAULT_RF}x RF | 7d retention]"
create_topic "logisticos.agent.triggers"  6 "$DEFAULT_RF" "$RETENTION_7D"

# ─── Summary ──────────────────────────────────────────────────────────────────
echo ""
echo -e "${BOLD}════════════════════════════════════════${RESET}"
echo -e "${BOLD}  Kafka Topic Provisioning — Summary${RESET}"
echo -e "${BOLD}════════════════════════════════════════${RESET}"
echo -e "  ${GREEN}Created :${RESET}  ${CREATED_COUNT}"
echo -e "  ${DIM}Skipped :${RESET}  ${SKIPPED_COUNT} (already existed)"

if [[ ${#FAILED_TOPICS[@]} -gt 0 ]]; then
  echo -e "  ${RED}Failed  :${RESET}  ${#FAILED_TOPICS[@]}"
  for t in "${FAILED_TOPICS[@]}"; do
    echo -e "    ${RED}•${RESET} $t"
  done
  echo ""
  log_error "Some topics failed to create. Check Kafka broker logs."
  exit 1
else
  echo -e "  ${RED}Failed  :${RESET}  0"
  echo ""
  log_ok "All topics provisioned successfully."
fi
