###############################################################################
# LogisticOS — RDS PostgreSQL Module
# Provisions an AWS RDS PostgreSQL instance with parameter group tuned for
# LogisticOS workloads (TimescaleDB, PostGIS, pg_stat_statements), proper
# security group isolation, and CloudWatch enhanced monitoring.
###############################################################################

terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
    random = {
      source  = "hashicorp/random"
      version = "~> 3.0"
    }
  }
}

###############################################################################
# Variables
###############################################################################

variable "identifier" {
  description = "Unique identifier for the RDS instance."
  type        = string
}

variable "engine_version" {
  description = "PostgreSQL engine version."
  type        = string
  default     = "15.4"
}

variable "instance_class" {
  description = "RDS instance class (e.g., db.t3.medium, db.r6g.xlarge)."
  type        = string
}

variable "vpc_id" {
  description = "VPC ID where the RDS instance will be deployed."
  type        = string
}

variable "subnet_ids" {
  description = "List of subnet IDs for the RDS subnet group (should be private)."
  type        = list(string)
}

variable "allowed_security_group_ids" {
  description = "List of security group IDs allowed to connect on port 5432."
  type        = list(string)
}

variable "db_name" {
  description = "Name of the initial database to create."
  type        = string
}

variable "master_username" {
  description = "Master username for the database."
  type        = string
}

variable "multi_az" {
  description = "Whether to enable Multi-AZ deployment for high availability."
  type        = bool
  default     = false
}

variable "backup_retention_days" {
  description = "Number of days to retain automated backups (0 disables backups)."
  type        = number
  default     = 7
}

variable "enable_performance_insights" {
  description = "Whether to enable RDS Performance Insights."
  type        = bool
  default     = true
}

variable "performance_insights_retention_period" {
  description = "Amount of time (in days) to retain Performance Insights data."
  type        = number
  default     = 7
}

variable "allocated_storage" {
  description = "Initial allocated storage in GiB."
  type        = number
  default     = 100
}

variable "max_allocated_storage" {
  description = "Maximum allocated storage for autoscaling in GiB (0 = disabled)."
  type        = number
  default     = 1000
}

variable "deletion_protection" {
  description = "Whether to enable deletion protection on the RDS instance."
  type        = bool
  default     = true
}

variable "skip_final_snapshot" {
  description = "Whether to skip a final snapshot when deleting the instance."
  type        = bool
  default     = false
}

variable "maintenance_window" {
  description = "Preferred maintenance window (UTC)."
  type        = string
  default     = "sun:03:00-sun:04:00"
}

variable "backup_window" {
  description = "Preferred backup window (UTC). Must not overlap maintenance_window."
  type        = string
  default     = "01:00-02:00"
}

variable "environment" {
  description = "Deployment environment name."
  type        = string
}

variable "tags" {
  description = "Additional tags to apply to all resources."
  type        = map(string)
  default     = {}
}

###############################################################################
# Local values
###############################################################################

locals {
  pg_family = "postgres${split(".", var.engine_version)[0]}"

  common_tags = merge(
    {
      Environment = var.environment
      Project     = "LogisticOS"
      ManagedBy   = "terraform"
      Service     = "rds-postgresql"
    },
    var.tags
  )
}

###############################################################################
# Random password (stored in Secrets Manager — never in TF state plain text)
###############################################################################

resource "random_password" "master" {
  length           = 32
  special          = true
  override_special = "!#$%^&*()-_=+[]{}|;:,.<>?"
}

resource "aws_secretsmanager_secret" "rds_master_password" {
  name                    = "logisticos/${var.environment}/rds/${var.identifier}/master-password"
  description             = "Master password for RDS instance ${var.identifier}"
  recovery_window_in_days = 7

  tags = local.common_tags
}

resource "aws_secretsmanager_secret_version" "rds_master_password" {
  secret_id = aws_secretsmanager_secret.rds_master_password.id
  secret_string = jsonencode({
    username = var.master_username
    password = random_password.master.result
    host     = aws_db_instance.main.address
    port     = aws_db_instance.main.port
    dbname   = var.db_name
    engine   = "postgresql"
  })

  depends_on = [aws_db_instance.main]
}

###############################################################################
# DB Subnet Group
###############################################################################

resource "aws_db_subnet_group" "main" {
  name        = "${var.identifier}-subnet-group"
  description = "Subnet group for RDS instance ${var.identifier}"
  subnet_ids  = var.subnet_ids

  tags = merge(local.common_tags, {
    Name = "${var.identifier}-subnet-group"
  })
}

###############################################################################
# Security Group
###############################################################################

resource "aws_security_group" "rds" {
  name        = "${var.identifier}-sg"
  description = "Security group for RDS instance ${var.identifier} — allows PostgreSQL from approved sources"
  vpc_id      = var.vpc_id

  dynamic "ingress" {
    for_each = var.allowed_security_group_ids
    content {
      description     = "PostgreSQL access from approved security group"
      from_port       = 5432
      to_port         = 5432
      protocol        = "tcp"
      security_groups = [ingress.value]
    }
  }

  egress {
    description = "Allow all outbound traffic"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = merge(local.common_tags, {
    Name = "${var.identifier}-sg"
  })
}

###############################################################################
# Parameter Group
# Enables pg_stat_statements, TimescaleDB, and PostGIS extensions.
# Tunes connection limits, WAL settings, and autovacuum for LogisticOS workload.
###############################################################################

resource "aws_db_parameter_group" "main" {
  name        = "${var.identifier}-pg15-params"
  family      = local.pg_family
  description = "LogisticOS optimised parameters for ${var.identifier}"

  # Core extensions
  parameter {
    name  = "shared_preload_libraries"
    value = "pg_stat_statements,timescaledb,postgis-3"
  }

  # pg_stat_statements
  parameter {
    name  = "pg_stat_statements.track"
    value = "all"
  }

  parameter {
    name  = "pg_stat_statements.max"
    value = "10000"
  }

  # Connection & memory
  parameter {
    name  = "max_connections"
    value = "500"
  }

  parameter {
    name  = "shared_buffers"
    value = "{DBInstanceClassMemory/4}"
    apply_method = "pending-reboot"
  }

  parameter {
    name  = "effective_cache_size"
    value = "{DBInstanceClassMemory*3/4}"
  }

  parameter {
    name  = "work_mem"
    value = "16384"  # 16 MB
  }

  parameter {
    name  = "maintenance_work_mem"
    value = "524288"  # 512 MB
    apply_method = "pending-reboot"
  }

  # WAL & replication
  parameter {
    name  = "wal_level"
    value = "logical"
    apply_method = "pending-reboot"
  }

  parameter {
    name  = "max_wal_size"
    value = "4096"  # 4 GB in MB
  }

  parameter {
    name  = "checkpoint_completion_target"
    value = "0.9"
  }

  # Autovacuum tuning for high-write tables
  parameter {
    name  = "autovacuum_vacuum_scale_factor"
    value = "0.05"
  }

  parameter {
    name  = "autovacuum_analyze_scale_factor"
    value = "0.025"
  }

  parameter {
    name  = "autovacuum_max_workers"
    value = "5"
    apply_method = "pending-reboot"
  }

  # Logging
  parameter {
    name  = "log_min_duration_statement"
    value = "1000"  # log queries > 1s
  }

  parameter {
    name  = "log_checkpoints"
    value = "1"
  }

  parameter {
    name  = "log_connections"
    value = "1"
  }

  parameter {
    name  = "log_disconnections"
    value = "1"
  }

  parameter {
    name  = "log_lock_waits"
    value = "1"
  }

  # SSL
  parameter {
    name  = "rds.force_ssl"
    value = "1"
  }

  tags = local.common_tags

  lifecycle {
    create_before_destroy = true
  }
}

###############################################################################
# KMS Key for RDS encryption
###############################################################################

resource "aws_kms_key" "rds" {
  description             = "KMS key for RDS instance ${var.identifier} encryption"
  deletion_window_in_days = 30
  enable_key_rotation     = true

  tags = merge(local.common_tags, {
    Name = "${var.identifier}-rds-kms"
  })
}

resource "aws_kms_alias" "rds" {
  name          = "alias/${var.identifier}-rds"
  target_key_id = aws_kms_key.rds.key_id
}

###############################################################################
# Enhanced Monitoring IAM Role
###############################################################################

resource "aws_iam_role" "rds_monitoring" {
  name = "${var.identifier}-rds-monitoring-role"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = { Service = "monitoring.rds.amazonaws.com" }
      Action    = "sts:AssumeRole"
    }]
  })

  tags = local.common_tags
}

resource "aws_iam_role_policy_attachment" "rds_monitoring" {
  role       = aws_iam_role.rds_monitoring.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonRDSEnhancedMonitoringRole"
}

###############################################################################
# RDS Instance
###############################################################################

resource "aws_db_instance" "main" {
  identifier = var.identifier

  # Engine
  engine         = "postgres"
  engine_version = var.engine_version

  # Instance
  instance_class        = var.instance_class
  multi_az              = var.multi_az
  publicly_accessible   = false

  # Storage
  storage_type          = "gp3"
  allocated_storage     = var.allocated_storage
  max_allocated_storage = var.max_allocated_storage
  storage_encrypted     = true
  kms_key_id            = aws_kms_key.rds.arn
  iops                  = 3000
  storage_throughput    = 125

  # Database
  db_name  = var.db_name
  username = var.master_username
  password = random_password.master.result

  # Network
  db_subnet_group_name   = aws_db_subnet_group.main.name
  vpc_security_group_ids = [aws_security_group.rds.id]

  # Parameter & option groups
  parameter_group_name = aws_db_parameter_group.main.name

  # Backup & maintenance
  backup_retention_period = var.backup_retention_days
  backup_window           = var.backup_window
  maintenance_window      = var.maintenance_window
  copy_tags_to_snapshot   = true
  delete_automated_backups = false

  # Final snapshot
  skip_final_snapshot       = var.skip_final_snapshot
  final_snapshot_identifier = var.skip_final_snapshot ? null : "${var.identifier}-final-snapshot"

  # Protection
  deletion_protection = var.deletion_protection

  # Monitoring
  monitoring_interval = 60
  monitoring_role_arn = aws_iam_role.rds_monitoring.arn

  # Performance Insights
  performance_insights_enabled          = var.enable_performance_insights
  performance_insights_kms_key_id       = var.enable_performance_insights ? aws_kms_key.rds.arn : null
  performance_insights_retention_period = var.enable_performance_insights ? var.performance_insights_retention_period : null

  # CloudWatch logs export
  enabled_cloudwatch_logs_exports = ["postgresql", "upgrade"]

  # Auto minor version upgrade
  auto_minor_version_upgrade = true
  apply_immediately          = false

  tags = merge(local.common_tags, {
    Name = var.identifier
  })

  depends_on = [
    aws_db_subnet_group.main,
    aws_db_parameter_group.main,
    aws_iam_role_policy_attachment.rds_monitoring,
  ]
}

###############################################################################
# CloudWatch Alarms
###############################################################################

resource "aws_cloudwatch_metric_alarm" "cpu_high" {
  alarm_name          = "${var.identifier}-cpu-high"
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = 3
  metric_name         = "CPUUtilization"
  namespace           = "AWS/RDS"
  period              = 300
  statistic           = "Average"
  threshold           = 80
  alarm_description   = "RDS CPU utilization exceeds 80% for ${var.identifier}"
  treat_missing_data  = "notBreaching"

  dimensions = {
    DBInstanceIdentifier = aws_db_instance.main.id
  }

  tags = local.common_tags
}

resource "aws_cloudwatch_metric_alarm" "freeable_memory_low" {
  alarm_name          = "${var.identifier}-freeable-memory-low"
  comparison_operator = "LessThanThreshold"
  evaluation_periods  = 3
  metric_name         = "FreeableMemory"
  namespace           = "AWS/RDS"
  period              = 300
  statistic           = "Average"
  threshold           = 536870912  # 512 MB in bytes
  alarm_description   = "RDS freeable memory below 512 MB for ${var.identifier}"
  treat_missing_data  = "notBreaching"

  dimensions = {
    DBInstanceIdentifier = aws_db_instance.main.id
  }

  tags = local.common_tags
}

resource "aws_cloudwatch_metric_alarm" "connections_high" {
  alarm_name          = "${var.identifier}-connections-high"
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = 2
  metric_name         = "DatabaseConnections"
  namespace           = "AWS/RDS"
  period              = 300
  statistic           = "Average"
  threshold           = 400
  alarm_description   = "RDS connection count exceeds 400 for ${var.identifier}"
  treat_missing_data  = "notBreaching"

  dimensions = {
    DBInstanceIdentifier = aws_db_instance.main.id
  }

  tags = local.common_tags
}

###############################################################################
# Outputs
###############################################################################

output "endpoint" {
  description = "Connection endpoint for the RDS instance."
  value       = aws_db_instance.main.address
}

output "port" {
  description = "Port number for the RDS instance."
  value       = aws_db_instance.main.port
}

output "db_name" {
  description = "Name of the initial database."
  value       = aws_db_instance.main.db_name
}

output "security_group_id" {
  description = "ID of the security group attached to the RDS instance."
  value       = aws_security_group.rds.id
}

output "instance_id" {
  description = "Identifier of the RDS instance."
  value       = aws_db_instance.main.id
}

output "instance_arn" {
  description = "ARN of the RDS instance."
  value       = aws_db_instance.main.arn
}

output "master_password_secret_arn" {
  description = "ARN of the Secrets Manager secret containing the master password."
  value       = aws_secretsmanager_secret.rds_master_password.arn
}

output "kms_key_arn" {
  description = "ARN of the KMS key used for RDS encryption."
  value       = aws_kms_key.rds.arn
}
