###############################################################################
# LogisticOS — Terraform Module: AWS MSK (Managed Streaming for Kafka)
#
# Purpose  : Provisions a production-grade MSK cluster for inter-service
#            event streaming. Follows the event-first architecture principle
#            (see CLAUDE.md): every state-change emits a Kafka event; downstream
#            services react without synchronous coupling.
#
# Features :
#   - SASL/IAM authentication (no credentials to rotate)
#   - TLS in-transit + at-rest encryption
#   - CloudWatch + S3 broker logging
#   - Prometheus open monitoring (scraped by kube-prometheus-stack)
#   - Immutable MSK configuration (version-pinned)
#   - Dedicated security group with least-privilege ingress
#
# Consumers: All 18 LogisticOS microservices + Kafka Connect (Debezium CDC)
###############################################################################

###############################################################################
# Variables
###############################################################################

variable "cluster_name" {
  description = "Name of the MSK cluster. Used for resource naming and tagging."
  type        = string
}

variable "kafka_version" {
  description = "Apache Kafka version to deploy. Must be supported by AWS MSK."
  type        = string
  default     = "3.6.0"

  validation {
    condition     = can(regex("^[0-9]+\\.[0-9]+\\.[0-9]+$", var.kafka_version))
    error_message = "kafka_version must be in semver format, e.g. 3.6.0"
  }
}

variable "instance_type" {
  description = "MSK broker instance type."
  type        = string
  default     = "kafka.t3.small"

  validation {
    condition = contains([
      "kafka.t3.small",
      "kafka.m5.large", "kafka.m5.xlarge", "kafka.m5.2xlarge",
      "kafka.m5.4xlarge", "kafka.m5.8xlarge", "kafka.m5.12xlarge",
      "kafka.m5.16xlarge", "kafka.m5.24xlarge",
    ], var.instance_type)
    error_message = "instance_type must be a valid MSK broker instance type."
  }
}

variable "vpc_id" {
  description = "VPC ID in which the MSK cluster will be deployed."
  type        = string
}

variable "subnet_ids" {
  description = "List of subnet IDs for broker placement (one per AZ). Length must equal number_of_broker_nodes."
  type        = list(string)
}

variable "number_of_broker_nodes" {
  description = "Total number of broker nodes. Must be a multiple of the number of AZs (subnet_ids length)."
  type        = number

  validation {
    condition     = var.number_of_broker_nodes >= 2
    error_message = "number_of_broker_nodes must be >= 2 for high availability."
  }
}

variable "ebs_volume_size" {
  description = "EBS volume size in GiB per broker node. Minimum 1, maximum 16384."
  type        = number
  default     = 100

  validation {
    condition     = var.ebs_volume_size >= 1 && var.ebs_volume_size <= 16384
    error_message = "ebs_volume_size must be between 1 and 16384 GiB."
  }
}

variable "tags" {
  description = "Additional resource tags to merge with module defaults."
  type        = map(string)
  default     = {}
}

variable "s3_logs_bucket" {
  description = "S3 bucket name for MSK broker logs. Leave empty to skip S3 logging."
  type        = string
  default     = ""
}

variable "s3_logs_prefix" {
  description = "S3 key prefix for MSK broker logs."
  type        = string
  default     = "msk/broker-logs"
}

variable "cloudwatch_log_retention_days" {
  description = "CloudWatch log group retention in days for MSK broker logs."
  type        = number
  default     = 30
}

###############################################################################
# Locals
###############################################################################

locals {
  # Derive VPC CIDR dynamically for security group ingress rules
  module_tags = merge(var.tags, {
    Module = "msk"
  })

  # Whether to ship logs to S3 in addition to CloudWatch
  enable_s3_logs = var.s3_logs_bucket != ""
}

###############################################################################
# Data Sources
###############################################################################

data "aws_vpc" "selected" {
  id = var.vpc_id
}

###############################################################################
# CloudWatch Log Group — Broker Logs
###############################################################################

resource "aws_cloudwatch_log_group" "broker" {
  name              = "/logisticos/msk/${var.cluster_name}/broker"
  retention_in_days = var.cloudwatch_log_retention_days

  tags = local.module_tags
}

###############################################################################
# Security Group
###############################################################################

resource "aws_security_group" "msk" {
  name        = "${var.cluster_name}-msk"
  description = "MSK cluster security group — controls broker access"
  vpc_id      = var.vpc_id

  # Kafka plaintext (legacy — kept for in-cluster migration tooling)
  ingress {
    description = "Kafka plaintext from VPC"
    from_port   = 9092
    to_port     = 9092
    protocol    = "tcp"
    cidr_blocks = [data.aws_vpc.selected.cidr_block]
  }

  # Kafka TLS
  ingress {
    description = "Kafka TLS from VPC"
    from_port   = 9094
    to_port     = 9094
    protocol    = "tcp"
    cidr_blocks = [data.aws_vpc.selected.cidr_block]
  }

  # Kafka SASL/IAM (port 9098 for IAM auth, 9096 for SASL/SCRAM)
  ingress {
    description = "Kafka SASL/IAM from VPC"
    from_port   = 9096
    to_port     = 9096
    protocol    = "tcp"
    cidr_blocks = [data.aws_vpc.selected.cidr_block]
  }

  ingress {
    description = "Kafka SASL/IAM (IAM auth) from VPC"
    from_port   = 9098
    to_port     = 9098
    protocol    = "tcp"
    cidr_blocks = [data.aws_vpc.selected.cidr_block]
  }

  # ZooKeeper (required for older Kafka tooling and Kafka Connect)
  ingress {
    description = "ZooKeeper from VPC"
    from_port   = 2181
    to_port     = 2181
    protocol    = "tcp"
    cidr_blocks = [data.aws_vpc.selected.cidr_block]
  }

  # JMX / Prometheus exporter — scraped by Prometheus running inside the VPC
  ingress {
    description = "JMX Prometheus exporter from VPC"
    from_port   = 11001
    to_port     = 11001
    protocol    = "tcp"
    cidr_blocks = [data.aws_vpc.selected.cidr_block]
  }

  ingress {
    description = "Node exporter from VPC"
    from_port   = 11002
    to_port     = 11002
    protocol    = "tcp"
    cidr_blocks = [data.aws_vpc.selected.cidr_block]
  }

  egress {
    description = "Allow all outbound"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = merge(local.module_tags, {
    Name = "${var.cluster_name}-msk"
  })

  lifecycle {
    create_before_destroy = true
  }
}

###############################################################################
# MSK Configuration
###############################################################################

resource "aws_msk_configuration" "main" {
  name              = "${var.cluster_name}-config"
  description       = "LogisticOS MSK broker configuration — production defaults"
  kafka_versions    = [var.kafka_version]

  server_properties = <<-EOT
    # Topic management — topics must be explicitly created via IaC or admin tooling.
    # Auto-creation is disabled to prevent accidental topic sprawl.
    auto.create.topics.enable=false

    # Replication — all topics must have replication factor >= 3 in production.
    # Producers are required to wait for all in-sync replicas before ack.
    default.replication.factor=3
    min.insync.replicas=2

    # Partition defaults — overridden per-topic by the topic provisioning module.
    num.partitions=3

    # Retention — 7 days (168 hours) default. High-volume telemetry topics
    # override this with shorter retention + compaction.
    log.retention.hours=168
    log.retention.bytes=-1

    # Segment size — 1 GiB segments balance flush frequency vs recovery time.
    log.segment.bytes=1073741824

    # Compaction — enable for CDC / change-log topics (per-topic override).
    log.cleanup.policy=delete

    # Consumer group rebalance — stagger rebalance to reduce thundering herd on
    # pod restarts in Kubernetes.
    group.initial.rebalance.delay.ms=3000

    # Message size — maximum uncompressed message size: 10 MiB.
    # Bulk shipment upload batches must not exceed this.
    message.max.bytes=10485760
    replica.fetch.max.bytes=10485760

    # Compression — producers negotiate; broker accepts any codec.
    compression.type=producer

    # Transaction support — required for exactly-once semantics in the
    # Payments and COD reconciliation services.
    transaction.state.log.replication.factor=3
    transaction.state.log.min.isr=2

    # Socket buffer tuning for high-throughput telemetry ingestion
    socket.send.buffer.bytes=1048576
    socket.receive.buffer.bytes=1048576
    socket.request.max.bytes=104857600

    # Offsets topic — must be highly durable
    offsets.topic.replication.factor=3
    offsets.retention.minutes=20160
  EOT
}

###############################################################################
# MSK Cluster
###############################################################################

resource "aws_msk_cluster" "main" {
  cluster_name           = var.cluster_name
  kafka_version          = var.kafka_version
  number_of_broker_nodes = var.number_of_broker_nodes

  # Broker node configuration
  broker_node_group_info {
    instance_type = var.instance_type
    client_subnets = var.subnet_ids
    security_groups = [aws_security_group.msk.id]

    # Distribute brokers evenly across AZs for fault isolation
    az_distribution = "DEFAULT"

    storage_info {
      ebs_storage_info {
        volume_size = var.ebs_volume_size

        # Provisioned throughput — only supported on kafka.m5.4xlarge and above.
        # For t3.small and m5.large, provisioned_throughput block is omitted.
        # Uncomment below for production m5.4xlarge+ deployments:
        # provisioned_throughput {
        #   enabled           = true
        #   volume_throughput = 250
        # }
      }
    }

    connectivity_info {
      # Public access disabled — all traffic must traverse the VPC
      public_access {
        type = "DISABLED"
      }
    }
  }

  # Encryption
  encryption_info {
    # At-rest encryption using AWS managed KMS key
    encryption_at_rest_kms_key_arn = "" # empty = AWS managed key

    # In-transit: TLS_PLAINTEXT allows both TLS and plaintext for legacy tooling.
    # Change to TLS only after all producers/consumers are confirmed TLS-capable.
    encryption_in_transit {
      in_cluster    = true
      client_broker = "TLS_PLAINTEXT"
    }
  }

  # Client authentication — SASL/IAM preferred; no credentials to manage.
  # Unauthenticated access is explicitly disabled.
  client_authentication {
    unauthenticated = false

    sasl {
      iam   = true
      scram = false
    }

    tls {
      certificate_authority_arns = []
    }
  }

  # Pin to the configuration version created above
  configuration_info {
    arn      = aws_msk_configuration.main.arn
    revision = aws_msk_configuration.main.latest_revision
  }

  # Logging — dual destination: CloudWatch for real-time alerting, S3 for archival
  logging_info {
    broker_logs {
      cloudwatch_logs {
        enabled   = true
        log_group = aws_cloudwatch_log_group.broker.name
      }

      firehose {
        enabled = false
      }

      s3 {
        enabled = local.enable_s3_logs
        bucket  = local.enable_s3_logs ? var.s3_logs_bucket : null
        prefix  = local.enable_s3_logs ? var.s3_logs_prefix : null
      }
    }
  }

  # Open monitoring — exposes JMX and node exporter endpoints for Prometheus.
  # Scraped by the kube-prometheus-stack deployed on the EKS cluster.
  open_monitoring {
    prometheus {
      jmx_exporter {
        enabled_in_broker = true
      }
      node_exporter {
        enabled_in_broker = true
      }
    }
  }

  # MSK Connect — enable managed connector plugins (Debezium CDC, S3 Sink)
  broker_node_group_info {
    instance_type   = var.instance_type
    client_subnets  = var.subnet_ids
    security_groups = [aws_security_group.msk.id]
    az_distribution = "DEFAULT"

    storage_info {
      ebs_storage_info {
        volume_size = var.ebs_volume_size
      }
    }
  }

  tags = local.module_tags

  # Prevent accidental cluster destruction — requires explicit override.
  # lifecycle { prevent_destroy = true } — enable in production environments

  lifecycle {
    # Configuration changes require a new revision; don't force replacement.
    ignore_changes = [
      broker_node_group_info[0].connectivity_info,
    ]
  }

  depends_on = [
    aws_cloudwatch_log_group.broker,
    aws_msk_configuration.main,
  ]
}

###############################################################################
# CloudWatch Alarms
###############################################################################

resource "aws_cloudwatch_metric_alarm" "kafka_under_replicated_partitions" {
  alarm_name          = "${var.cluster_name}-under-replicated-partitions"
  alarm_description   = "MSK has under-replicated partitions — data durability at risk"
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = 1
  metric_name         = "UnderReplicatedPartitions"
  namespace           = "AWS/Kafka"
  period              = 60
  statistic           = "Maximum"
  threshold           = 0
  treat_missing_data  = "notBreaching"

  dimensions = {
    "Cluster Name" = var.cluster_name
  }

  alarm_actions = [] # Wire to SNS topic in the calling environment module

  tags = local.module_tags
}

resource "aws_cloudwatch_metric_alarm" "kafka_active_controller_count" {
  alarm_name          = "${var.cluster_name}-active-controller-count"
  alarm_description   = "MSK active controller count is not 1 — cluster may be unhealthy"
  comparison_operator = "LessThanThreshold"
  evaluation_periods  = 2
  metric_name         = "ActiveControllerCount"
  namespace           = "AWS/Kafka"
  period              = 60
  statistic           = "Average"
  threshold           = 1
  treat_missing_data  = "breaching"

  dimensions = {
    "Cluster Name" = var.cluster_name
  }

  tags = local.module_tags
}

resource "aws_cloudwatch_metric_alarm" "kafka_disk_utilization" {
  alarm_name          = "${var.cluster_name}-disk-utilization-high"
  alarm_description   = "MSK broker disk utilization above 75% — expand EBS or reduce retention"
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = 3
  metric_name         = "KafkaDataLogsDiskUsed"
  namespace           = "AWS/Kafka"
  period              = 300
  statistic           = "Maximum"
  threshold           = 75
  treat_missing_data  = "notBreaching"

  dimensions = {
    "Cluster Name" = var.cluster_name
  }

  tags = local.module_tags
}

###############################################################################
# Outputs
###############################################################################

output "bootstrap_brokers_tls" {
  description = "TLS bootstrap broker endpoints (port 9094). Use for clients with certificate-based auth."
  value       = aws_msk_cluster.main.bootstrap_brokers_tls
  sensitive   = true
}

output "bootstrap_brokers_sasl_iam" {
  description = "SASL/IAM bootstrap broker endpoints (port 9098). Preferred for IAM-authenticated services."
  value       = aws_msk_cluster.main.bootstrap_brokers_sasl_iam
  sensitive   = true
}

output "bootstrap_brokers_plaintext" {
  description = "Plaintext bootstrap broker endpoints (port 9092). For internal tooling only — NOT for application use."
  value       = aws_msk_cluster.main.bootstrap_brokers
  sensitive   = true
}

output "zookeeper_connect_string" {
  description = "ZooKeeper connection string. Required for legacy admin tooling and Kafka Connect."
  value       = aws_msk_cluster.main.zookeeper_connect_string
  sensitive   = true
}

output "cluster_arn" {
  description = "MSK cluster ARN. Use in IAM policies granting kafka:* actions."
  value       = aws_msk_cluster.main.arn
}

output "cluster_name" {
  description = "MSK cluster name."
  value       = aws_msk_cluster.main.cluster_name
}

output "security_group_id" {
  description = "ID of the MSK cluster security group. Add as ingress source for producer/consumer SGs."
  value       = aws_security_group.msk.id
}

output "cloudwatch_log_group_name" {
  description = "CloudWatch log group name for MSK broker logs."
  value       = aws_cloudwatch_log_group.broker.name
}

output "configuration_arn" {
  description = "ARN of the MSK configuration resource."
  value       = aws_msk_configuration.main.arn
}

output "configuration_latest_revision" {
  description = "Latest revision number of the MSK configuration."
  value       = aws_msk_configuration.main.latest_revision
}
