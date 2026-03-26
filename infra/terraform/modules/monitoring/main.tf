###############################################################################
# LogisticOS — Monitoring Module
# Provisions AWS CloudWatch log groups, metric alarms, SNS topics, and a
# unified CloudWatch Dashboard covering EKS, RDS, MSK, and ElastiCache.
# Slack alerting is delivered via an SNS → Lambda → Slack webhook chain.
###############################################################################

terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }
}

###############################################################################
# Variables
###############################################################################

variable "environment" {
  description = "Deployment environment (dev | staging | production)."
  type        = string

  validation {
    condition     = contains(["dev", "staging", "production"], var.environment)
    error_message = "environment must be one of: dev, staging, production."
  }
}

variable "cluster_name" {
  description = "Name of the EKS cluster being monitored."
  type        = string
}

variable "aws_region" {
  description = "AWS region where resources are deployed."
  type        = string
  default     = "ap-southeast-1"
}

variable "aws_account_id" {
  description = "AWS account ID — used in dashboard ARN references."
  type        = string
}

variable "slack_webhook_url" {
  description = "Slack incoming webhook URL for alert delivery.  Stored in Secrets Manager; this value is passed only at plan/apply time."
  type        = string
  sensitive   = true
}

variable "pagerduty_integration_key" {
  description = "PagerDuty Events API v2 integration key for critical alert escalation."
  type        = string
  sensitive   = true
}

variable "alert_email_addresses" {
  description = "List of email addresses to subscribe to SNS alert topics."
  type        = list(string)
  default     = []
}

variable "rds_instance_id" {
  description = "RDS instance identifier to attach CloudWatch alarms to."
  type        = string
}

variable "msk_cluster_name" {
  description = "MSK (Kafka) cluster name for CloudWatch metric filtering."
  type        = string
}

variable "elasticache_cluster_id" {
  description = "ElastiCache (Redis) cluster ID for CloudWatch metric filtering."
  type        = string
}

variable "log_retention_days" {
  description = "CloudWatch log group retention period in days.  Recommended: 30 for dev, 90 for production."
  type        = number
  default     = 30

  validation {
    condition     = contains([1, 3, 5, 7, 14, 30, 60, 90, 120, 150, 180, 365, 400, 545, 731, 1827, 3653], var.log_retention_days)
    error_message = "log_retention_days must be a value supported by AWS CloudWatch (e.g. 30, 90, 365)."
  }
}

variable "slack_lambda_function_arn" {
  description = "ARN of the Lambda function that forwards SNS messages to Slack.  Deploy the slack-notifier Lambda separately and reference its ARN here."
  type        = string
  default     = ""
}

variable "tags" {
  description = "Additional tags applied to all monitoring resources."
  type        = map(string)
  default     = {}
}

###############################################################################
# Locals
###############################################################################

locals {
  common_tags = merge(
    {
      Project     = "logisticos"
      Environment = var.environment
      ManagedBy   = "terraform"
      Module      = "monitoring"
    },
    var.tags
  )

  # All 17 LogisticOS microservices — used to generate log groups uniformly
  services = [
    "identity",
    "cdp",
    "engagement",
    "order-intake",
    "dispatch",
    "driver-ops",
    "delivery-experience",
    "fleet",
    "hub-ops",
    "carrier",
    "pod",
    "payments",
    "analytics",
    "marketing",
    "business-logic",
    "ai-layer",
    "api-gateway",
  ]

  # Alarm evaluation defaults
  eval_period_seconds = 60
  datapoints_to_alarm = 3

  # Enable Slack Lambda subscription only when ARN is provided
  enable_slack = var.slack_lambda_function_arn != ""
}

###############################################################################
# CloudWatch Log Groups — one per service
###############################################################################

resource "aws_cloudwatch_log_group" "services" {
  for_each = toset(local.services)

  name              = "/logisticos/${var.environment}/${each.value}"
  retention_in_days = var.log_retention_days

  tags = merge(local.common_tags, { Service = each.value })
}

# Additional log groups for infrastructure components
resource "aws_cloudwatch_log_group" "eks_control_plane" {
  name              = "/aws/eks/${var.cluster_name}/cluster"
  retention_in_days = var.log_retention_days

  tags = merge(local.common_tags, { Component = "eks-control-plane" })
}

resource "aws_cloudwatch_log_group" "rds" {
  for_each = toset(["postgresql", "upgrade"])

  name              = "/aws/rds/instance/${var.rds_instance_id}/${each.value}"
  retention_in_days = var.log_retention_days

  tags = merge(local.common_tags, { Component = "rds" })
}

###############################################################################
# SNS Topics — critical / warning / info severity tiers
###############################################################################

resource "aws_sns_topic" "critical" {
  name              = "logisticos-alerts-critical-${var.environment}"
  kms_master_key_id = "alias/aws/sns"

  tags = merge(local.common_tags, { AlertSeverity = "critical" })
}

resource "aws_sns_topic" "warning" {
  name              = "logisticos-alerts-warning-${var.environment}"
  kms_master_key_id = "alias/aws/sns"

  tags = merge(local.common_tags, { AlertSeverity = "warning" })
}

resource "aws_sns_topic" "info" {
  name              = "logisticos-alerts-info-${var.environment}"
  kms_master_key_id = "alias/aws/sns"

  tags = merge(local.common_tags, { AlertSeverity = "info" })
}

###############################################################################
# SNS Subscriptions — email
###############################################################################

resource "aws_sns_topic_subscription" "critical_email" {
  for_each = toset(var.alert_email_addresses)

  topic_arn = aws_sns_topic.critical.arn
  protocol  = "email"
  endpoint  = each.value
}

resource "aws_sns_topic_subscription" "warning_email" {
  for_each = toset(var.alert_email_addresses)

  topic_arn = aws_sns_topic.warning.arn
  protocol  = "email"
  endpoint  = each.value
}

###############################################################################
# SNS Subscriptions — Slack (via Lambda)
###############################################################################

resource "aws_sns_topic_subscription" "critical_slack" {
  count = local.enable_slack ? 1 : 0

  topic_arn = aws_sns_topic.critical.arn
  protocol  = "lambda"
  endpoint  = var.slack_lambda_function_arn
}

resource "aws_sns_topic_subscription" "warning_slack" {
  count = local.enable_slack ? 1 : 0

  topic_arn = aws_sns_topic.warning.arn
  protocol  = "lambda"
  endpoint  = var.slack_lambda_function_arn
}

resource "aws_lambda_permission" "sns_invoke_slack_critical" {
  count = local.enable_slack ? 1 : 0

  statement_id  = "AllowSNSInvokeSlackCritical-${var.environment}"
  action        = "lambda:InvokeFunction"
  function_name = var.slack_lambda_function_arn
  principal     = "sns.amazonaws.com"
  source_arn    = aws_sns_topic.critical.arn
}

resource "aws_lambda_permission" "sns_invoke_slack_warning" {
  count = local.enable_slack ? 1 : 0

  statement_id  = "AllowSNSInvokeSlackWarning-${var.environment}"
  action        = "lambda:InvokeFunction"
  function_name = var.slack_lambda_function_arn
  principal     = "sns.amazonaws.com"
  source_arn    = aws_sns_topic.warning.arn
}

###############################################################################
# CloudWatch Alarms — EKS Node Health
###############################################################################

resource "aws_cloudwatch_metric_alarm" "eks_node_memory_high" {
  alarm_name          = "logisticos-${var.environment}-eks-node-memory-high"
  alarm_description   = "EKS node memory utilization exceeds 80%.  Risk of pod eviction and OOMKill events."
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = var.datapoints_to_alarm
  metric_name         = "node_memory_utilization"
  namespace           = "ContainerInsights"
  period              = var.eval_period_seconds
  statistic           = "Average"
  threshold           = 80
  treat_missing_data  = "notBreaching"

  dimensions = {
    ClusterName = var.cluster_name
  }

  alarm_actions             = [aws_sns_topic.warning.arn]
  ok_actions                = [aws_sns_topic.info.arn]
  insufficient_data_actions = []

  tags = merge(local.common_tags, { AlarmGroup = "eks" })
}

resource "aws_cloudwatch_metric_alarm" "eks_node_cpu_high" {
  alarm_name          = "logisticos-${var.environment}-eks-node-cpu-high"
  alarm_description   = "EKS node CPU utilization exceeds 85%.  Cluster autoscaler should be investigated."
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = var.datapoints_to_alarm
  metric_name         = "node_cpu_utilization"
  namespace           = "ContainerInsights"
  period              = var.eval_period_seconds
  statistic           = "Average"
  threshold           = 85
  treat_missing_data  = "notBreaching"

  dimensions = {
    ClusterName = var.cluster_name
  }

  alarm_actions             = [aws_sns_topic.warning.arn]
  ok_actions                = [aws_sns_topic.info.arn]
  insufficient_data_actions = []

  tags = merge(local.common_tags, { AlarmGroup = "eks" })
}

resource "aws_cloudwatch_metric_alarm" "eks_pod_restart_high" {
  alarm_name          = "logisticos-${var.environment}-eks-pod-restarts-high"
  alarm_description   = "Pod restart count exceeded 5 in a 10-minute window.  Indicates crash-looping or liveness probe failures."
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = 1
  metric_name         = "pod_number_of_container_restarts"
  namespace           = "ContainerInsights"
  period              = 600 # 10 minutes
  statistic           = "Sum"
  threshold           = 5
  treat_missing_data  = "notBreaching"

  dimensions = {
    ClusterName = var.cluster_name
  }

  alarm_actions             = [aws_sns_topic.critical.arn]
  ok_actions                = [aws_sns_topic.info.arn]
  insufficient_data_actions = []

  tags = merge(local.common_tags, { AlarmGroup = "eks" })
}

###############################################################################
# CloudWatch Alarms — RDS PostgreSQL
###############################################################################

resource "aws_cloudwatch_metric_alarm" "rds_free_storage_low" {
  alarm_name          = "logisticos-${var.environment}-rds-free-storage-low"
  alarm_description   = "RDS free storage below 10 GB.  Immediate action required to prevent write failures."
  comparison_operator = "LessThanThreshold"
  evaluation_periods  = 2
  metric_name         = "FreeStorageSpace"
  namespace           = "AWS/RDS"
  period              = 300
  statistic           = "Average"
  threshold           = 10737418240 # 10 GB in bytes
  treat_missing_data  = "notBreaching"

  dimensions = {
    DBInstanceIdentifier = var.rds_instance_id
  }

  alarm_actions             = [aws_sns_topic.critical.arn]
  ok_actions                = [aws_sns_topic.info.arn]
  insufficient_data_actions = [aws_sns_topic.warning.arn]

  tags = merge(local.common_tags, { AlarmGroup = "rds" })
}

resource "aws_cloudwatch_metric_alarm" "rds_connections_high" {
  alarm_name          = "logisticos-${var.environment}-rds-connections-high"
  alarm_description   = "RDS connection count exceeds 400.  PgBouncer pool saturation likely; check connection pool config."
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = var.datapoints_to_alarm
  metric_name         = "DatabaseConnections"
  namespace           = "AWS/RDS"
  period              = var.eval_period_seconds
  statistic           = "Average"
  threshold           = 400
  treat_missing_data  = "notBreaching"

  dimensions = {
    DBInstanceIdentifier = var.rds_instance_id
  }

  alarm_actions             = [aws_sns_topic.warning.arn]
  ok_actions                = [aws_sns_topic.info.arn]
  insufficient_data_actions = []

  tags = merge(local.common_tags, { AlarmGroup = "rds" })
}

resource "aws_cloudwatch_metric_alarm" "rds_read_latency_high" {
  alarm_name          = "logisticos-${var.environment}-rds-read-latency-high"
  alarm_description   = "RDS read latency exceeds 100ms average.  Investigate missing indexes or I/O saturation."
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = var.datapoints_to_alarm
  metric_name         = "ReadLatency"
  namespace           = "AWS/RDS"
  period              = var.eval_period_seconds
  statistic           = "Average"
  threshold           = 0.1 # 100ms in seconds
  treat_missing_data  = "notBreaching"

  dimensions = {
    DBInstanceIdentifier = var.rds_instance_id
  }

  alarm_actions             = [aws_sns_topic.warning.arn]
  ok_actions                = [aws_sns_topic.info.arn]
  insufficient_data_actions = []

  tags = merge(local.common_tags, { AlarmGroup = "rds" })
}

resource "aws_cloudwatch_metric_alarm" "rds_write_latency_high" {
  alarm_name          = "logisticos-${var.environment}-rds-write-latency-high"
  alarm_description   = "RDS write latency exceeds 100ms.  May indicate I/O throughput exhaustion or lock contention."
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = var.datapoints_to_alarm
  metric_name         = "WriteLatency"
  namespace           = "AWS/RDS"
  period              = var.eval_period_seconds
  statistic           = "Average"
  threshold           = 0.1
  treat_missing_data  = "notBreaching"

  dimensions = {
    DBInstanceIdentifier = var.rds_instance_id
  }

  alarm_actions             = [aws_sns_topic.warning.arn]
  ok_actions                = [aws_sns_topic.info.arn]
  insufficient_data_actions = []

  tags = merge(local.common_tags, { AlarmGroup = "rds" })
}

###############################################################################
# CloudWatch Alarms — MSK (Kafka)
###############################################################################

resource "aws_cloudwatch_metric_alarm" "msk_under_replicated_partitions" {
  alarm_name          = "logisticos-${var.environment}-msk-under-replicated-partitions"
  alarm_description   = "MSK has under-replicated partitions.  Data durability is at risk; investigate broker health immediately."
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = 1
  metric_name         = "UnderReplicatedPartitions"
  namespace           = "AWS/Kafka"
  period              = 60
  statistic           = "Maximum"
  threshold           = 0
  treat_missing_data  = "notBreaching"

  dimensions = {
    "Cluster Name" = var.msk_cluster_name
  }

  alarm_actions             = [aws_sns_topic.critical.arn]
  ok_actions                = [aws_sns_topic.info.arn]
  insufficient_data_actions = [aws_sns_topic.warning.arn]

  tags = merge(local.common_tags, { AlarmGroup = "msk" })
}

resource "aws_cloudwatch_metric_alarm" "msk_offline_partitions" {
  alarm_name          = "logisticos-${var.environment}-msk-offline-partitions"
  alarm_description   = "MSK has offline partitions.  Producers and consumers are blocked; immediate broker restart required."
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = 1
  metric_name         = "OfflinePartitionsCount"
  namespace           = "AWS/Kafka"
  period              = 60
  statistic           = "Maximum"
  threshold           = 0
  treat_missing_data  = "notBreaching"

  dimensions = {
    "Cluster Name" = var.msk_cluster_name
  }

  alarm_actions             = [aws_sns_topic.critical.arn]
  ok_actions                = [aws_sns_topic.info.arn]
  insufficient_data_actions = [aws_sns_topic.warning.arn]

  tags = merge(local.common_tags, { AlarmGroup = "msk" })
}

resource "aws_cloudwatch_metric_alarm" "msk_disk_used_high" {
  alarm_name          = "logisticos-${var.environment}-msk-disk-used-high"
  alarm_description   = "MSK data log disk utilization exceeds 75%.  Increase storage or adjust retention before broker becomes read-only."
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = var.datapoints_to_alarm
  metric_name         = "KafkaDataLogsDiskUsed"
  namespace           = "AWS/Kafka"
  period              = 300
  statistic           = "Maximum"
  threshold           = 75
  treat_missing_data  = "notBreaching"

  dimensions = {
    "Cluster Name" = var.msk_cluster_name
  }

  alarm_actions             = [aws_sns_topic.warning.arn]
  ok_actions                = [aws_sns_topic.info.arn]
  insufficient_data_actions = []

  tags = merge(local.common_tags, { AlarmGroup = "msk" })
}

###############################################################################
# CloudWatch Alarms — ElastiCache (Redis)
###############################################################################

resource "aws_cloudwatch_metric_alarm" "elasticache_cpu_high" {
  alarm_name          = "logisticos-${var.environment}-elasticache-cpu-high"
  alarm_description   = "ElastiCache engine CPU utilization exceeds 70%.  Redis is single-threaded; high CPU indicates hot keys or slow commands."
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = var.datapoints_to_alarm
  metric_name         = "EngineCPUUtilization"
  namespace           = "AWS/ElastiCache"
  period              = var.eval_period_seconds
  statistic           = "Average"
  threshold           = 70
  treat_missing_data  = "notBreaching"

  dimensions = {
    CacheClusterId = var.elasticache_cluster_id
  }

  alarm_actions             = [aws_sns_topic.warning.arn]
  ok_actions                = [aws_sns_topic.info.arn]
  insufficient_data_actions = []

  tags = merge(local.common_tags, { AlarmGroup = "elasticache" })
}

resource "aws_cloudwatch_metric_alarm" "elasticache_connections_high" {
  alarm_name          = "logisticos-${var.environment}-elasticache-connections-high"
  alarm_description   = "ElastiCache connection count exceeds 5000.  Connection pool exhaustion may cause service degradation."
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = var.datapoints_to_alarm
  metric_name         = "CurrConnections"
  namespace           = "AWS/ElastiCache"
  period              = var.eval_period_seconds
  statistic           = "Average"
  threshold           = 5000
  treat_missing_data  = "notBreaching"

  dimensions = {
    CacheClusterId = var.elasticache_cluster_id
  }

  alarm_actions             = [aws_sns_topic.warning.arn]
  ok_actions                = [aws_sns_topic.info.arn]
  insufficient_data_actions = []

  tags = merge(local.common_tags, { AlarmGroup = "elasticache" })
}

resource "aws_cloudwatch_metric_alarm" "elasticache_evictions_high" {
  alarm_name          = "logisticos-${var.environment}-elasticache-evictions-high"
  alarm_description   = "ElastiCache evictions are elevated.  Memory pressure is causing key eviction; increase node size or reduce TTLs."
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = 2
  metric_name         = "Evictions"
  namespace           = "AWS/ElastiCache"
  period              = 300
  statistic           = "Sum"
  threshold           = 100
  treat_missing_data  = "notBreaching"

  dimensions = {
    CacheClusterId = var.elasticache_cluster_id
  }

  alarm_actions             = [aws_sns_topic.warning.arn]
  ok_actions                = [aws_sns_topic.info.arn]
  insufficient_data_actions = []

  tags = merge(local.common_tags, { AlarmGroup = "elasticache" })
}

###############################################################################
# CloudWatch Dashboard — LogisticOS Overview
###############################################################################

resource "aws_cloudwatch_dashboard" "logisticos_overview" {
  dashboard_name = "LogisticOS-${var.environment}-Overview"

  dashboard_body = jsonencode({
    widgets = [
      # ── Header ─────────────────────────────────────────────────────────────
      {
        type   = "text"
        x      = 0
        y      = 0
        width  = 24
        height = 1
        properties = {
          markdown = "# LogisticOS — ${upper(var.environment)} Infrastructure Overview  |  Cluster: `${var.cluster_name}`  |  RDS: `${var.rds_instance_id}`  |  MSK: `${var.msk_cluster_name}`"
        }
      },

      # ── EKS Node CPU ────────────────────────────────────────────────────────
      {
        type   = "metric"
        x      = 0
        y      = 1
        width  = 8
        height = 6
        properties = {
          title  = "EKS — Node CPU Utilization (%)"
          view   = "timeSeries"
          stacked = false
          period = 60
          stat   = "Average"
          metrics = [
            ["ContainerInsights", "node_cpu_utilization", "ClusterName", var.cluster_name, { label = "CPU %", color = "#00E5FF" }]
          ]
          annotations = {
            horizontal = [{ label = "Warning (85%)", value = 85, color = "#FFAB00" }]
          }
          yAxis = { left = { min = 0, max = 100 } }
        }
      },

      # ── EKS Node Memory ─────────────────────────────────────────────────────
      {
        type   = "metric"
        x      = 8
        y      = 1
        width  = 8
        height = 6
        properties = {
          title  = "EKS — Node Memory Utilization (%)"
          view   = "timeSeries"
          stacked = false
          period = 60
          stat   = "Average"
          metrics = [
            ["ContainerInsights", "node_memory_utilization", "ClusterName", var.cluster_name, { label = "Memory %", color = "#A855F7" }]
          ]
          annotations = {
            horizontal = [{ label = "Warning (80%)", value = 80, color = "#FFAB00" }]
          }
          yAxis = { left = { min = 0, max = 100 } }
        }
      },

      # ── EKS Pod Restarts ────────────────────────────────────────────────────
      {
        type   = "metric"
        x      = 16
        y      = 1
        width  = 8
        height = 6
        properties = {
          title  = "EKS — Pod Container Restarts (10m sum)"
          view   = "timeSeries"
          stacked = false
          period = 600
          stat   = "Sum"
          metrics = [
            ["ContainerInsights", "pod_number_of_container_restarts", "ClusterName", var.cluster_name, { label = "Restarts", color = "#FF6B6B" }]
          ]
          annotations = {
            horizontal = [{ label = "Alert threshold (5)", value = 5, color = "#FF0000" }]
          }
        }
      },

      # ── RDS Section Header ───────────────────────────────────────────────────
      {
        type   = "text"
        x      = 0
        y      = 7
        width  = 24
        height = 1
        properties = {
          markdown = "## RDS PostgreSQL — `${var.rds_instance_id}`"
        }
      },

      # ── RDS CPU ─────────────────────────────────────────────────────────────
      {
        type   = "metric"
        x      = 0
        y      = 8
        width  = 6
        height = 6
        properties = {
          title  = "RDS — CPU Utilization (%)"
          view   = "timeSeries"
          period = 60
          stat   = "Average"
          metrics = [
            ["AWS/RDS", "CPUUtilization", "DBInstanceIdentifier", var.rds_instance_id, { label = "CPU %", color = "#00E5FF" }]
          ]
          yAxis = { left = { min = 0, max = 100 } }
        }
      },

      # ── RDS Connections ─────────────────────────────────────────────────────
      {
        type   = "metric"
        x      = 6
        y      = 8
        width  = 6
        height = 6
        properties = {
          title  = "RDS — Database Connections"
          view   = "timeSeries"
          period = 60
          stat   = "Average"
          metrics = [
            ["AWS/RDS", "DatabaseConnections", "DBInstanceIdentifier", var.rds_instance_id, { label = "Connections", color = "#A855F7" }]
          ]
          annotations = {
            horizontal = [{ label = "Warning (400)", value = 400, color = "#FFAB00" }]
          }
        }
      },

      # ── RDS Free Storage ─────────────────────────────────────────────────────
      {
        type   = "metric"
        x      = 12
        y      = 8
        width  = 6
        height = 6
        properties = {
          title  = "RDS — Free Storage (bytes)"
          view   = "timeSeries"
          period = 300
          stat   = "Average"
          metrics = [
            ["AWS/RDS", "FreeStorageSpace", "DBInstanceIdentifier", var.rds_instance_id, { label = "Free Storage", color = "#00FF88" }]
          ]
          annotations = {
            horizontal = [{ label = "Critical (10 GB)", value = 10737418240, color = "#FF0000" }]
          }
        }
      },

      # ── RDS Read Latency ─────────────────────────────────────────────────────
      {
        type   = "metric"
        x      = 18
        y      = 8
        width  = 6
        height = 6
        properties = {
          title  = "RDS — Read/Write Latency (s)"
          view   = "timeSeries"
          period = 60
          stat   = "Average"
          metrics = [
            ["AWS/RDS", "ReadLatency", "DBInstanceIdentifier", var.rds_instance_id, { label = "Read Latency", color = "#00E5FF" }],
            ["AWS/RDS", "WriteLatency", "DBInstanceIdentifier", var.rds_instance_id, { label = "Write Latency", color = "#FF6B6B" }]
          ]
          annotations = {
            horizontal = [{ label = "100ms", value = 0.1, color = "#FFAB00" }]
          }
        }
      },

      # ── MSK Section Header ───────────────────────────────────────────────────
      {
        type   = "text"
        x      = 0
        y      = 14
        width  = 24
        height = 1
        properties = {
          markdown = "## MSK (Kafka) — `${var.msk_cluster_name}`"
        }
      },

      # ── MSK Under-Replicated Partitions ──────────────────────────────────────
      {
        type   = "metric"
        x      = 0
        y      = 15
        width  = 8
        height = 6
        properties = {
          title  = "MSK — Under-Replicated Partitions"
          view   = "timeSeries"
          period = 60
          stat   = "Maximum"
          metrics = [
            ["AWS/Kafka", "UnderReplicatedPartitions", "Cluster Name", var.msk_cluster_name, { label = "Under-Replicated", color = "#FF6B6B" }]
          ]
          annotations = {
            horizontal = [{ label = "Alert (> 0)", value = 0, color = "#FF0000" }]
          }
        }
      },

      # ── MSK Offline Partitions ───────────────────────────────────────────────
      {
        type   = "metric"
        x      = 8
        y      = 15
        width  = 8
        height = 6
        properties = {
          title  = "MSK — Offline Partitions"
          view   = "timeSeries"
          period = 60
          stat   = "Maximum"
          metrics = [
            ["AWS/Kafka", "OfflinePartitionsCount", "Cluster Name", var.msk_cluster_name, { label = "Offline Partitions", color = "#FF0000" }]
          ]
          annotations = {
            horizontal = [{ label = "Alert (> 0)", value = 0, color = "#FF0000" }]
          }
        }
      },

      # ── MSK Disk Used ────────────────────────────────────────────────────────
      {
        type   = "metric"
        x      = 16
        y      = 15
        width  = 8
        height = 6
        properties = {
          title  = "MSK — Data Log Disk Used (%)"
          view   = "timeSeries"
          period = 300
          stat   = "Maximum"
          metrics = [
            ["AWS/Kafka", "KafkaDataLogsDiskUsed", "Cluster Name", var.msk_cluster_name, { label = "Disk Used %", color = "#FFAB00" }]
          ]
          annotations = {
            horizontal = [{ label = "Warning (75%)", value = 75, color = "#FFAB00" }]
          }
          yAxis = { left = { min = 0, max = 100 } }
        }
      },

      # ── ElastiCache Section Header ────────────────────────────────────────────
      {
        type   = "text"
        x      = 0
        y      = 21
        width  = 24
        height = 1
        properties = {
          markdown = "## ElastiCache (Redis) — `${var.elasticache_cluster_id}`"
        }
      },

      # ── ElastiCache CPU ───────────────────────────────────────────────────────
      {
        type   = "metric"
        x      = 0
        y      = 22
        width  = 8
        height = 6
        properties = {
          title  = "ElastiCache — Engine CPU Utilization (%)"
          view   = "timeSeries"
          period = 60
          stat   = "Average"
          metrics = [
            ["AWS/ElastiCache", "EngineCPUUtilization", "CacheClusterId", var.elasticache_cluster_id, { label = "CPU %", color = "#00E5FF" }]
          ]
          annotations = {
            horizontal = [{ label = "Warning (70%)", value = 70, color = "#FFAB00" }]
          }
          yAxis = { left = { min = 0, max = 100 } }
        }
      },

      # ── ElastiCache Connections ───────────────────────────────────────────────
      {
        type   = "metric"
        x      = 8
        y      = 22
        width  = 8
        height = 6
        properties = {
          title  = "ElastiCache — Current Connections"
          view   = "timeSeries"
          period = 60
          stat   = "Average"
          metrics = [
            ["AWS/ElastiCache", "CurrConnections", "CacheClusterId", var.elasticache_cluster_id, { label = "Connections", color = "#A855F7" }]
          ]
          annotations = {
            horizontal = [{ label = "Warning (5000)", value = 5000, color = "#FFAB00" }]
          }
        }
      },

      # ── ElastiCache Memory + Evictions ───────────────────────────────────────
      {
        type   = "metric"
        x      = 16
        y      = 22
        width  = 8
        height = 6
        properties = {
          title  = "ElastiCache — Memory Used (bytes) & Evictions"
          view   = "timeSeries"
          period = 60
          stat   = "Average"
          metrics = [
            ["AWS/ElastiCache", "BytesUsedForCache", "CacheClusterId", var.elasticache_cluster_id, { label = "Memory Used", yAxis = "left", color = "#00FF88" }],
            ["AWS/ElastiCache", "Evictions", "CacheClusterId", var.elasticache_cluster_id, { label = "Evictions", yAxis = "right", color = "#FF6B6B", stat = "Sum" }]
          ]
        }
      }
    ]
  })
}

###############################################################################
# Outputs
###############################################################################

output "sns_topic_arns" {
  description = "Map of alert severity to SNS topic ARN."
  value = {
    critical = aws_sns_topic.critical.arn
    warning  = aws_sns_topic.warning.arn
    info     = aws_sns_topic.info.arn
  }
}

output "log_group_names" {
  description = "Map of service name to CloudWatch log group name."
  value = {
    for service, lg in aws_cloudwatch_log_group.services :
    service => lg.name
  }
}

output "log_group_arns" {
  description = "Map of service name to CloudWatch log group ARN."
  value = {
    for service, lg in aws_cloudwatch_log_group.services :
    service => lg.arn
  }
}

output "dashboard_name" {
  description = "Name of the CloudWatch overview dashboard."
  value       = aws_cloudwatch_dashboard.logisticos_overview.dashboard_name
}

output "dashboard_arn" {
  description = "ARN of the CloudWatch overview dashboard."
  value       = aws_cloudwatch_dashboard.logisticos_overview.dashboard_arn
}

output "alarm_arns" {
  description = "Map of alarm name to ARN for all alarms created by this module."
  value = {
    eks_node_memory_high             = aws_cloudwatch_metric_alarm.eks_node_memory_high.arn
    eks_node_cpu_high                = aws_cloudwatch_metric_alarm.eks_node_cpu_high.arn
    eks_pod_restart_high             = aws_cloudwatch_metric_alarm.eks_pod_restart_high.arn
    rds_free_storage_low             = aws_cloudwatch_metric_alarm.rds_free_storage_low.arn
    rds_connections_high             = aws_cloudwatch_metric_alarm.rds_connections_high.arn
    rds_read_latency_high            = aws_cloudwatch_metric_alarm.rds_read_latency_high.arn
    rds_write_latency_high           = aws_cloudwatch_metric_alarm.rds_write_latency_high.arn
    msk_under_replicated_partitions  = aws_cloudwatch_metric_alarm.msk_under_replicated_partitions.arn
    msk_offline_partitions           = aws_cloudwatch_metric_alarm.msk_offline_partitions.arn
    msk_disk_used_high               = aws_cloudwatch_metric_alarm.msk_disk_used_high.arn
    elasticache_cpu_high             = aws_cloudwatch_metric_alarm.elasticache_cpu_high.arn
    elasticache_connections_high     = aws_cloudwatch_metric_alarm.elasticache_connections_high.arn
    elasticache_evictions_high       = aws_cloudwatch_metric_alarm.elasticache_evictions_high.arn
  }
}
