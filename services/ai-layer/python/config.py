"""Configuration for the LogisticOS AI agent sidecar."""
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_prefix="LOGISTICOS_AI_", env_file=".env")

    # Anthropic
    anthropic_api_key: str
    claude_model: str = "claude-opus-4-6"
    agent_max_turns: int = 20

    # Rust AI layer MCP tool endpoint (called for tool execution)
    rust_ai_layer_url: str = "http://ai-layer:8080"

    # Downstream service URLs (for direct calls when needed)
    dispatch_url: str = "http://dispatch:8080"
    order_intake_url: str = "http://order-intake:8080"
    driver_ops_url: str = "http://driver-ops:8080"
    engagement_url: str = "http://engagement:8080"
    analytics_url: str = "http://analytics:8080"
    payments_url: str = "http://payments:8080"
    cdp_url: str = "http://cdp:8080"
    hub_ops_url: str = "http://hub-ops:8080"
    fleet_url: str = "http://fleet:8080"

    # Kafka
    kafka_brokers: str = "kafka:9092"
    kafka_consumer_group: str = "ai-agents-python"
    kafka_topics: list[str] = [
        "shipment.created",
        "delivery.failed",
        "delivery.completed",
        "driver.idle",
        "merchant.support.request",
        "cod.collection.anomaly",
    ]

    # HTTP server
    host: str = "0.0.0.0"
    port: int = 8090

    # Observability
    otlp_endpoint: str = "http://otel-collector:4317"
    log_level: str = "INFO"

    # Feature flags
    dispatch_agent_enabled: bool = True
    recovery_agent_enabled: bool = True
    support_agent_enabled: bool = True
    reconciliation_agent_enabled: bool = True


settings = Settings()  # type: ignore[call-arg]
