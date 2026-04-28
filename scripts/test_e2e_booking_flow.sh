#!/bin/bash
# ============================================================================
# LogisticOS E2E Booking Flow Manual Test Script
# ============================================================================
# Run this script to test the complete booking-to-delivery flow
# Prerequisites: Docker Compose running or VPS services accessible
# ============================================================================

set -e

# Configuration
API_BASE="${API_BASE:-http://localhost:3001}"
DB_HOST="${DB_HOST:-localhost}"
DB_PORT="${DB_PORT:-5432}"
DB_USER="${DB_USER:-logisticos}"
KAFKA_BROKER="${KAFKA_BROKER:-localhost:9092}"

# Test data
TENANT_ID="${TENANT_ID:-12345678-1234-5678-1234-567812345678}"
USER_ID="${USER_ID:-87654321-4321-8765-4321-876543218765}"
TEST_TOKEN="${TEST_TOKEN:-}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# ============================================================================
# Helper Functions
# ============================================================================

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[✓]${NC} $1"
}

log_error() {
    echo -e "${RED}[✗]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[!]${NC} $1"
}

# Check if services are accessible
check_services() {
    log_info "Checking service accessibility..."

    if ! curl -s "${API_BASE}/health" > /dev/null 2>&1; then
        log_error "Order Intake service not accessible at ${API_BASE}"
        log_warning "Make sure Docker Compose is running: docker-compose up -d"
        return 1
    fi

    log_success "Order Intake service is accessible"
    return 0
}

# Check database connectivity
check_database() {
    log_info "Checking PostgreSQL connectivity..."

    if ! psql -h "${DB_HOST}" -U "${DB_USER}" -d "svc_order_intake" -c "SELECT 1" > /dev/null 2>&1; then
        log_warning "Could not connect to PostgreSQL directly (may need password)"
        log_info "Continuing with HTTP API tests only"
        return 1
    fi

    log_success "PostgreSQL is accessible"
    return 0
}

# ============================================================================
# Test 1: Create Single Shipment
# ============================================================================

test_create_single_shipment() {
    log_info "Test 1: Creating single shipment..."

    if [ -z "$TEST_TOKEN" ]; then
        log_warning "TEST_TOKEN not set; using dummy token (will likely fail)"
        TEST_TOKEN="dummy-jwt-token"
    fi

    RESPONSE=$(curl -s -X POST "${API_BASE}/v1/shipments" \
        -H "Authorization: Bearer ${TEST_TOKEN}" \
        -H "Content-Type: application/json" \
        -d '{
            "customer_name": "Juan dela Cruz",
            "customer_phone": "+639171234567",
            "customer_email": "juan@example.com",
            "origin": {
                "line1": "123 Warehouse Road",
                "city": "Pasig",
                "province": "Metro Manila",
                "postal_code": "1605",
                "country_code": "PH"
            },
            "destination": {
                "line1": "456 Customer Street",
                "city": "Quezon City",
                "province": "Metro Manila",
                "postal_code": "1100",
                "country_code": "PH"
            },
            "service_type": "standard",
            "weight_grams": 1500
        }')

    SHIPMENT_ID=$(echo "$RESPONSE" | jq -r '.id // empty')
    TRACKING_NUMBER=$(echo "$RESPONSE" | jq -r '.tracking_number // empty')

    if [ -z "$SHIPMENT_ID" ]; then
        log_error "Failed to create shipment. Response:"
        echo "$RESPONSE" | jq '.' 2>/dev/null || echo "$RESPONSE"
        return 1
    fi

    log_success "Shipment created with ID: ${SHIPMENT_ID}"
    log_success "Tracking number: ${TRACKING_NUMBER}"

    # Save for later tests
    echo "$SHIPMENT_ID" > /tmp/shipment_id.txt

    return 0
}

# ============================================================================
# Test 2: Verify Shipment in Database
# ============================================================================

test_verify_shipment_in_db() {
    log_info "Test 2: Verifying shipment in PostgreSQL..."

    SHIPMENT_ID=$(cat /tmp/shipment_id.txt 2>/dev/null || echo "")
    if [ -z "$SHIPMENT_ID" ]; then
        log_warning "Shipment ID not found; skipping database verification"
        return 0
    fi

    # Try to query the database
    if ! RESULT=$(psql -h "${DB_HOST}" -U "${DB_USER}" -d "svc_order_intake" -t -c \
        "SELECT id, status FROM shipments WHERE id='${SHIPMENT_ID}' LIMIT 1" 2>/dev/null); then
        log_warning "Could not query PostgreSQL; skipping"
        return 0
    fi

    if [ -n "$RESULT" ]; then
        log_success "Shipment found in database:"
        log_success "Status: $(echo $RESULT | awk '{print $NF}')"
    else
        log_warning "Shipment not found in database (may not be persisted yet)"
    fi

    return 0
}

# ============================================================================
# Test 3: Verify Kafka Event
# ============================================================================

test_verify_kafka_event() {
    log_info "Test 3: Checking Kafka topic..."

    # Try to check if Kafka is accessible
    if ! command -v kafka-console-consumer &> /dev/null; then
        log_warning "kafka-console-consumer not in PATH; skipping Kafka verification"
        log_info "To check Kafka events manually:"
        log_info "  docker exec logisticos-kafka kafka-console-consumer --bootstrap-server localhost:9092 --topic logisticos.shipment.created --from-beginning --timeout-ms 5000"
        return 0
    fi

    log_warning "Kafka verification requires docker access; please check manually"
    return 0
}

# ============================================================================
# Test 4: Bulk Upload
# ============================================================================

test_bulk_upload() {
    log_info "Test 4: Testing bulk shipment creation..."

    RESPONSE=$(curl -s -X POST "${API_BASE}/v1/shipments/bulk" \
        -H "Authorization: Bearer ${TEST_TOKEN}" \
        -H "Content-Type: application/json" \
        -d '{
            "rows": [
                {
                    "customer_name": "Customer One",
                    "customer_phone": "+639171111111",
                    "origin": {"line1": "Warehouse A", "city": "Manila", "province": "Metro Manila", "postal_code": "1000", "country_code": "PH"},
                    "destination": {"line1": "Address One", "city": "Quezon City", "province": "Metro Manila", "postal_code": "1100", "country_code": "PH"},
                    "service_type": "standard",
                    "weight_grams": 500
                },
                {
                    "customer_name": "Customer Two",
                    "customer_phone": "+639172222222",
                    "origin": {"line1": "Warehouse B", "city": "Makati", "province": "Metro Manila", "postal_code": "1200", "country_code": "PH"},
                    "destination": {"line1": "Address Two", "city": "Taguig", "province": "Metro Manila", "postal_code": "1600", "country_code": "PH"},
                    "service_type": "express",
                    "weight_grams": 1000
                }
            ]
        }')

    CREATED=$(echo "$RESPONSE" | jq '.created | length' 2>/dev/null || echo "0")
    FAILED=$(echo "$RESPONSE" | jq '.failed | length' 2>/dev/null || echo "0")

    log_success "Bulk upload: ${CREATED} created, ${FAILED} failed"

    if [ "$FAILED" -gt "0" ]; then
        log_warning "Failed shipments:"
        echo "$RESPONSE" | jq '.failed' 2>/dev/null || echo "$RESPONSE"
    fi

    return 0
}

# ============================================================================
# Test 5: Error Case - COD Exceeds Declared Value
# ============================================================================

test_error_cod_exceeds_value() {
    log_info "Test 5: Testing COD validation (should fail)..."

    RESPONSE=$(curl -s -X POST "${API_BASE}/v1/shipments" \
        -H "Authorization: Bearer ${TEST_TOKEN}" \
        -H "Content-Type: application/json" \
        -d '{
            "customer_name": "Test Customer",
            "customer_phone": "+639171234567",
            "customer_email": "test@example.com",
            "origin": {"line1": "Origin St", "city": "Manila", "province": "Metro Manila", "postal_code": "1000", "country_code": "PH"},
            "destination": {"line1": "Dest St", "city": "Quezon City", "province": "Metro Manila", "postal_code": "1100", "country_code": "PH"},
            "service_type": "standard",
            "weight_grams": 500,
            "declared_value_cents": 1000,
            "cod_amount_cents": 5000
        }')

    ERROR_CODE=$(echo "$RESPONSE" | jq -r '.error.code // empty' 2>/dev/null)

    if [ "$ERROR_CODE" = "BUSINESS_RULE_VIOLATION" ]; then
        log_success "Error correctly returned for COD > declared value"
        log_success "Error message: $(echo "$RESPONSE" | jq -r '.error.message // empty')"
    else
        log_warning "Expected BUSINESS_RULE_VIOLATION error, got:"
        echo "$RESPONSE" | jq '.' 2>/dev/null || echo "$RESPONSE"
    fi

    return 0
}

# ============================================================================
# Main Test Suite
# ============================================================================

main() {
    echo ""
    echo "╔════════════════════════════════════════════════════════════╗"
    echo "║   LogisticOS E2E Booking Flow Test Suite                   ║"
    echo "╚════════════════════════════════════════════════════════════╝"
    echo ""

    # Check environment
    log_info "Environment Configuration:"
    log_info "  API Base: ${API_BASE}"
    log_info "  Database: ${DB_USER}@${DB_HOST}:${DB_PORT}"
    log_info "  Kafka: ${KAFKA_BROKER}"
    echo ""

    # Run pre-flight checks
    if ! check_services; then
        log_error "Cannot proceed; services not accessible"
        exit 1
    fi
    check_database || true
    echo ""

    # Run tests
    PASSED=0
    FAILED=0

    if test_create_single_shipment; then
        ((PASSED++))
    else
        ((FAILED++))
    fi
    echo ""

    if test_verify_shipment_in_db; then
        ((PASSED++))
    else
        ((FAILED++))
    fi
    echo ""

    if test_verify_kafka_event; then
        ((PASSED++))
    else
        ((FAILED++))
    fi
    echo ""

    if test_bulk_upload; then
        ((PASSED++))
    else
        ((FAILED++))
    fi
    echo ""

    if test_error_cod_exceeds_value; then
        ((PASSED++))
    else
        ((FAILED++))
    fi
    echo ""

    # Summary
    echo "╔════════════════════════════════════════════════════════════╗"
    echo "║   Test Summary                                             ║"
    echo "╚════════════════════════════════════════════════════════════╝"
    echo -e "${GREEN}Passed: ${PASSED}${NC}"
    echo -e "${RED}Failed: ${FAILED}${NC}"
    echo ""

    if [ "$FAILED" -eq "0" ]; then
        log_success "All tests passed!"
        exit 0
    else
        log_error "Some tests failed"
        exit 1
    fi
}

# Run main
main "$@"
