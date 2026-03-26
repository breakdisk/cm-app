###############################################################################
# LogisticOS — Terraform Module: AWS ElastiCache (Redis)
#
# Purpose  : Provisions a Redis replication group for caching, session
#            management, pub/sub, and rate limiting across all LogisticOS
#            microservices.
#
# Key design decisions:
#   - SASL/AUTH token stored in AWS Secrets Manager — never in state plaintext
#   - Keyspace notifications (Kxg) enabled: required by the Engagement Engine
#     for TTL-based delivery notification scheduling and session expiry events
#   - Multi-AZ with automatic failover for HA
#   - CloudWatch alarms on hit rate, connections, and CPU
#
# Consumers: Identity (sessions), CDP (profile cache), Engagement Engine
#            (rate limiting, pub/sub), Dispatch (driver location cache),
#            API Gateway (JWT cache, rate limiter)
###############################################################################

###############################################################################
# Variables
###############################################################################

variable "cluster_id" {
  description = "Unique identifier for the ElastiCache replication group. Must be 40 chars or fewer."
  type        = string

  validation {
    condition     = length(var.cluster_id) <= 40 && can(regex("^[a-z0-9-]+$", var.cluster_id))
    error_message = "cluster_id must be lowercase alphanumeric with hyphens, max 40 characters."
  }
}

variable "description" {
  description = "Human-readable description of the replication group."
  type        = string
  default     = "LogisticOS ElastiCache Redis cluster"
}

variable "node_type" {
  description = "ElastiCache node type for each cache cluster."
  type        = string
  default     = "cache.t3.medium"
}

variable "num_cache_clusters" {
  description = "Number of cache clusters (nodes) in the replication group. Must be >= 2 for HA."
  type        = number
  default     = 2

  validation {
    condition     = var.num_cache_clusters >= 2
    error_message = "num_cache_clusters must be >= 2 (1 primary + >= 1 replica) for high availability."
  }
}

variable "engine_version" {
  description = "Redis engine version. E.g. '7.1'."
  type        = string
  default     = "7.1"
}

variable "port" {
  description = "Redis port."
  type        = number
  default     = 6379
}

variable "vpc_id" {
  description = "VPC ID where the ElastiCache cluster will be deployed."
  type        = string
}

variable "subnet_ids" {
  description = "List of subnet IDs for the ElastiCache subnet group (should be private/intra subnets)."
  type        = list(string)
}

variable "allowed_security_group_ids" {
  description = "List of security group IDs allowed to access Redis on port 6379."
  type        = list(string)
}

variable "at_rest_encryption" {
  description = "Enable at-rest encryption using AWS managed KMS key."
  type        = bool
  default     = true
}

variable "transit_encryption_enabled" {
  description = "Enable in-transit TLS encryption. Requires AUTH token to be set."
  type        = bool
  default     = true
}

variable "auth_token_secret_id" {
  description = "Secrets Manager secret ID containing the Redis AUTH token. Required when transit_encryption_enabled = true."
  type        = string
  default     = ""
}

variable "snapshot_retention_limit" {
  description = "Number of days to retain automatic Redis snapshots. 0 disables backups."
  type        = number
  default     = 7
}

variable "maintenance_window" {
  description = "Weekly maintenance window in UTC. Format: ddd:hh:mm-ddd:hh:mm"
  type        = string
  default     = "sun:05:00-sun:06:00"
}

variable "snapshot_window" {
  description = "Daily snapshot window in UTC. Format: hh:mm-hh:mm"
  type        = string
  default     = "03:00-04:00"
}

variable "apply_immediately" {
  description = "Apply changes immediately rather than during the next maintenance window."
  type        = bool
  default     = false
}

variable "auto_minor_version_upgrade" {
  description = "Automatically apply minor Redis engine upgrades during the maintenance window."
  type        = bool
  default     = true
}

variable "tags" {
  description = "Additional tags to apply to all resources."
  type        = map(string)
  default     = {}
}

variable "alarm_actions" {
  description = "List of ARNs to notify when CloudWatch alarms fire (e.g. SNS topic ARNs)."
  type        = list(string)
  default     = []
}

variable "ok_actions" {
  description = "List of ARNs to notify when CloudWatch alarms return to OK state."
  type        = list(string)
  default     = []
}

# Keyspace notification parameters — fine-grained control for callers
variable "keyspace_notification_events" {
  description = "Redis notify-keyspace-events config value. Kxg = keyspace + expired + generic. Empty string disables notifications."
  type        = string
  default     = "Kxg"
}

variable "maxmemory_policy" {
  description = "Redis maxmemory eviction policy."
  type        = string
  default     = "allkeys-lru"

  validation {
    condition = contains([
      "noeviction", "allkeys-lru", "volatile-lru",
      "allkeys-random", "volatile-random", "volatile-ttl",
      "allkeys-lfu", "volatile-lfu",
    ], var.maxmemory_policy)
    error_message = "maxmemory_policy must be a valid Redis eviction policy."
  }
}

###############################################################################
# Locals
###############################################################################

locals {
  module_tags = merge(var.tags, {
    Module = "elasticache"
  })

  # Parameter group family derived from major engine version
  # "7.1" → "redis7" | "6.2" → "redis6.x"
  engine_major = split(".", var.engine_version)[0]
  parameter_group_family = local.engine_major == "7" ? "redis7" : "redis${local.engine_major}.x"
}

###############################################################################
# Data Sources
###############################################################################

data "aws_secretsmanager_secret_version" "auth_token" {
  count     = var.auth_token_secret_id != "" ? 1 : 0
  secret_id = var.auth_token_secret_id
}

###############################################################################
# Subnet Group
###############################################################################

resource "aws_elasticache_subnet_group" "main" {
  name        = var.cluster_id
  description = "ElastiCache subnet group for ${var.cluster_id}"
  subnet_ids  = var.subnet_ids

  tags = local.module_tags
}

###############################################################################
# Security Group
###############################################################################

resource "aws_security_group" "redis" {
  name        = "${var.cluster_id}-redis"
  description = "ElastiCache Redis security group — controls inbound access on port ${var.port}"
  vpc_id      = var.vpc_id

  dynamic "ingress" {
    for_each = var.allowed_security_group_ids
    content {
      description     = "Redis from allowed security group"
      from_port       = var.port
      to_port         = var.port
      protocol        = "tcp"
      security_groups = [ingress.value]
    }
  }

  egress {
    description = "Allow all outbound (required for ElastiCache internal replication)"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = merge(local.module_tags, {
    Name = "${var.cluster_id}-redis"
  })

  lifecycle {
    create_before_destroy = true
  }
}

###############################################################################
# Parameter Group
###############################################################################

resource "aws_elasticache_parameter_group" "main" {
  name   = "${var.cluster_id}-params"
  family = local.parameter_group_family

  description = "LogisticOS Redis parameter group for ${var.cluster_id}"

  # Keyspace notifications — K=keyspace events, x=expired, g=generic commands.
  # Critical for the Engagement Engine: TTL expiry on OTP tokens, delivery
  # notification locks, and webhook deduplication keys fires a keyspace event
  # that the Rust listener (via SUBSCRIBE __keyevent@*__:expired) acts on.
  parameter {
    name  = "notify-keyspace-events"
    value = var.keyspace_notification_events
  }

  # LRU eviction — cache-aside pattern used throughout LogisticOS; evict least
  # recently used keys when memory pressure occurs rather than returning OOM.
  parameter {
    name  = "maxmemory-policy"
    value = var.maxmemory_policy
  }

  # Keepalive — prevent idle TCP connections from being silently dropped by
  # intermediate network devices (common in AWS VPCs with NAT GW).
  parameter {
    name  = "tcp-keepalive"
    value = "300"
  }

  # Slow log — record commands taking longer than 10ms for performance tuning
  parameter {
    name  = "slowlog-log-slower-than"
    value = "10000"
  }

  parameter {
    name  = "slowlog-max-len"
    value = "128"
  }

  # Lazy freeing — free memory asynchronously to avoid blocking the event loop
  # during large key deletions (e.g. session purge, cache invalidation floods).
  parameter {
    name  = "lazyfree-lazy-eviction"
    value = "yes"
  }

  parameter {
    name  = "lazyfree-lazy-expire"
    value = "yes"
  }

  parameter {
    name  = "lazyfree-lazy-server-del"
    value = "yes"
  }

  tags = local.module_tags
}

###############################################################################
# Replication Group
###############################################################################

resource "aws_elasticache_replication_group" "main" {
  replication_group_id = var.cluster_id
  description          = var.description

  node_type            = var.node_type
  num_cache_clusters   = var.num_cache_clusters
  engine_version       = var.engine_version
  port                 = var.port
  parameter_group_name = aws_elasticache_parameter_group.main.name
  subnet_group_name    = aws_elasticache_subnet_group.main.name
  security_group_ids   = [aws_security_group.redis.id]

  # HA configuration
  automatic_failover_enabled = true
  multi_az_enabled           = true

  # Encryption
  at_rest_encryption_enabled = var.at_rest_encryption
  transit_encryption_enabled = var.transit_encryption_enabled

  # AUTH token — sourced from Secrets Manager so it never appears in plan output
  # as a sensitive string in the resource diff (still sensitive in state file).
  auth_token = (
    var.transit_encryption_enabled && var.auth_token_secret_id != ""
    ? data.aws_secretsmanager_secret_version.auth_token[0].secret_string
    : null
  )

  # Maintenance and backup
  maintenance_window       = var.maintenance_window
  snapshot_window          = var.snapshot_window
  snapshot_retention_limit = var.snapshot_retention_limit

  auto_minor_version_upgrade = var.auto_minor_version_upgrade
  apply_immediately          = var.apply_immediately

  log_delivery_configuration {
    destination      = aws_cloudwatch_log_group.slow_log.name
    destination_type = "cloudwatch-logs"
    log_format       = "json"
    log_type         = "slow-log"
  }

  log_delivery_configuration {
    destination      = aws_cloudwatch_log_group.engine_log.name
    destination_type = "cloudwatch-logs"
    log_format       = "json"
    log_type         = "engine-log"
  }

  tags = local.module_tags

  lifecycle {
    # AUTH token changes must be applied deliberately — prevent unintended
    # in-place replacement caused by token rotation via Secrets Manager.
    ignore_changes = [auth_token]
  }
}

###############################################################################
# CloudWatch Log Groups
###############################################################################

resource "aws_cloudwatch_log_group" "slow_log" {
  name              = "/logisticos/elasticache/${var.cluster_id}/slow-log"
  retention_in_days = 30

  tags = local.module_tags
}

resource "aws_cloudwatch_log_group" "engine_log" {
  name              = "/logisticos/elasticache/${var.cluster_id}/engine-log"
  retention_in_days = 30

  tags = local.module_tags
}

###############################################################################
# CloudWatch Metric Alarms
###############################################################################

# Low cache hit rate indicates ineffective caching strategy or cold cache.
# A sustained rate below 80% in steady-state deserves investigation.
resource "aws_cloudwatch_metric_alarm" "cache_hit_rate_low" {
  alarm_name          = "${var.cluster_id}-cache-hit-rate-low"
  alarm_description   = "ElastiCache hit rate dropped below 80% — possible cache thrashing or key expiry storm"
  comparison_operator = "LessThanThreshold"
  evaluation_periods  = 5
  metric_name         = "CacheHits"
  namespace           = "AWS/ElastiCache"
  period              = 60
  extended_statistic  = "p50"
  threshold           = 0.8
  treat_missing_data  = "notBreaching"

  # Use a metric math expression for hit rate %
  metric_query {
    id          = "hit_rate"
    expression  = "hits / (hits + misses) * 100"
    label       = "Cache Hit Rate %"
    return_data = true
  }

  metric_query {
    id = "hits"
    metric {
      metric_name = "CacheHits"
      namespace   = "AWS/ElastiCache"
      period      = 60
      stat        = "Sum"
      dimensions = {
        ReplicationGroupId = var.cluster_id
      }
    }
  }

  metric_query {
    id = "misses"
    metric {
      metric_name = "CacheMisses"
      namespace   = "AWS/ElastiCache"
      period      = 60
      stat        = "Sum"
      dimensions = {
        ReplicationGroupId = var.cluster_id
      }
    }
  }

  alarm_actions = var.alarm_actions
  ok_actions    = var.ok_actions

  tags = local.module_tags
}

# High connection count — sustained near the node limit causes connection refused errors.
resource "aws_cloudwatch_metric_alarm" "curr_connections_high" {
  alarm_name          = "${var.cluster_id}-curr-connections-high"
  alarm_description   = "ElastiCache connection count is high — check for connection leaks in services"
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = 3
  metric_name         = "CurrConnections"
  namespace           = "AWS/ElastiCache"
  period              = 60
  statistic           = "Maximum"
  threshold           = 5000
  treat_missing_data  = "notBreaching"

  dimensions = {
    ReplicationGroupId = var.cluster_id
  }

  alarm_actions = var.alarm_actions
  ok_actions    = var.ok_actions

  tags = local.module_tags
}

# Engine CPU — Redis is single-threaded for command processing; sustained high
# CPU on the engine thread causes command latency to spike across all services.
resource "aws_cloudwatch_metric_alarm" "engine_cpu_high" {
  alarm_name          = "${var.cluster_id}-engine-cpu-high"
  alarm_description   = "ElastiCache Redis engine CPU > 75% — review slow commands, add read replicas or scale up"
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = 3
  metric_name         = "EngineCPUUtilization"
  namespace           = "AWS/ElastiCache"
  period              = 60
  statistic           = "Maximum"
  threshold           = 75
  treat_missing_data  = "notBreaching"

  dimensions = {
    ReplicationGroupId = var.cluster_id
  }

  alarm_actions = var.alarm_actions
  ok_actions    = var.ok_actions

  tags = local.module_tags
}

# Freeable memory — if near zero, ElastiCache begins evicting keys.
resource "aws_cloudwatch_metric_alarm" "freeable_memory_low" {
  alarm_name          = "${var.cluster_id}-freeable-memory-low"
  alarm_description   = "ElastiCache freeable memory < 256 MiB — imminent eviction or OOM risk"
  comparison_operator = "LessThanThreshold"
  evaluation_periods  = 3
  metric_name         = "FreeableMemory"
  namespace           = "AWS/ElastiCache"
  period              = 60
  statistic           = "Minimum"
  threshold           = 268435456 # 256 MiB in bytes
  treat_missing_data  = "notBreaching"

  dimensions = {
    ReplicationGroupId = var.cluster_id
  }

  alarm_actions = var.alarm_actions
  ok_actions    = var.ok_actions

  tags = local.module_tags
}

# Replication lag — a lagging replica means reads from the reader endpoint
# return stale data; relevant for CDP profile reads and session validation.
resource "aws_cloudwatch_metric_alarm" "replication_lag_high" {
  alarm_name          = "${var.cluster_id}-replication-lag-high"
  alarm_description   = "ElastiCache replication lag > 30 seconds — replica serving stale data"
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = 2
  metric_name         = "ReplicationLag"
  namespace           = "AWS/ElastiCache"
  period              = 60
  statistic           = "Maximum"
  threshold           = 30
  treat_missing_data  = "notBreaching"

  dimensions = {
    ReplicationGroupId = var.cluster_id
  }

  alarm_actions = var.alarm_actions
  ok_actions    = var.ok_actions

  tags = local.module_tags
}

###############################################################################
# Outputs
###############################################################################

output "primary_endpoint" {
  description = "Primary endpoint address for write operations."
  value       = aws_elasticache_replication_group.main.primary_endpoint_address
  sensitive   = true
}

output "reader_endpoint" {
  description = "Reader endpoint for read-heavy operations (e.g. CDP profile lookups, rate limit checks)."
  value       = aws_elasticache_replication_group.main.reader_endpoint_address
  sensitive   = true
}

output "port" {
  description = "Redis port (default 6379)."
  value       = var.port
}

output "security_group_id" {
  description = "ID of the Redis security group. Add as allowed source for EKS node SGs."
  value       = aws_security_group.redis.id
}

output "auth_token_secret_arn" {
  description = "Secrets Manager ARN of the Redis AUTH token. Grant secretsmanager:GetSecretValue to service IAM roles."
  value       = var.auth_token_secret_id != "" ? data.aws_secretsmanager_secret_version.auth_token[0].arn : ""
}

output "replication_group_id" {
  description = "ElastiCache replication group ID."
  value       = aws_elasticache_replication_group.main.id
}

output "parameter_group_name" {
  description = "Parameter group name in use by this replication group."
  value       = aws_elasticache_parameter_group.main.name
}

output "subnet_group_name" {
  description = "ElastiCache subnet group name."
  value       = aws_elasticache_subnet_group.main.name
}

output "slow_log_group_name" {
  description = "CloudWatch log group for Redis slow log entries."
  value       = aws_cloudwatch_log_group.slow_log.name
}
