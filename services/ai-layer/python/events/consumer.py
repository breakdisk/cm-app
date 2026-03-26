"""
Kafka consumer for the LogisticOS AI agent sidecar.

Consumes events from logistics topics and dispatches them to agent handlers.
Uses aiokafka for non-blocking async consumption alongside the FastAPI server.

Consumer group: ai-agents-python
Offset strategy: earliest (reprocess on restart, agents are idempotent via session_id)
"""
from __future__ import annotations

import asyncio
import json
from typing import Any

import structlog
from aiokafka import AIOKafkaConsumer
from aiokafka.errors import KafkaError

from config import settings
from events.handlers import route_event

logger = structlog.get_logger(__name__)


class KafkaAgentConsumer:
    """Async Kafka consumer that routes events to AI agents."""

    def __init__(self) -> None:
        self._consumer: AIOKafkaConsumer | None = None
        self._running = False

    async def start(self) -> None:
        """Start the Kafka consumer. Called on FastAPI startup."""
        self._consumer = AIOKafkaConsumer(
            *settings.kafka_topics,
            bootstrap_servers=settings.kafka_brokers,
            group_id=settings.kafka_consumer_group,
            auto_offset_reset="earliest",
            enable_auto_commit=True,
            value_deserializer=lambda b: json.loads(b.decode("utf-8")),
            # Process one message at a time to prevent overloading the agent
            # with concurrent Claude API calls. Increase max_poll_records for
            # higher throughput once rate limits allow.
            max_poll_records=1,
        )
        await self._consumer.start()
        self._running = True
        logger.info("kafka_consumer_started", topics=settings.kafka_topics)
        asyncio.create_task(self._consume_loop())

    async def stop(self) -> None:
        """Graceful shutdown — called on FastAPI shutdown."""
        self._running = False
        if self._consumer:
            await self._consumer.stop()
            logger.info("kafka_consumer_stopped")

    async def _consume_loop(self) -> None:
        """Main consumption loop. Runs until stopped."""
        assert self._consumer is not None

        while self._running:
            try:
                async for message in self._consumer:
                    if not self._running:
                        break

                    log = logger.bind(
                        topic=message.topic,
                        partition=message.partition,
                        offset=message.offset,
                    )
                    log.info("kafka_message_received")

                    try:
                        payload: dict[str, Any] = message.value
                        result = await route_event(message.topic, payload)
                        if result:
                            log.info(
                                "agent_result",
                                agent=result.agent_type,
                                status=result.status,
                                confidence=result.confidence,
                                actions=result.actions_taken,
                            )
                    except Exception as exc:
                        log.error("event_processing_failed", error=str(exc))
                        # Do not re-raise — commit the offset and continue.
                        # Failed events should be investigated via logs/traces.

            except KafkaError as exc:
                logger.error("kafka_error", error=str(exc))
                if self._running:
                    # Brief back-off before reconnect attempt.
                    await asyncio.sleep(5)
            except Exception as exc:
                logger.error("consumer_loop_exception", error=str(exc))
                if self._running:
                    await asyncio.sleep(5)
