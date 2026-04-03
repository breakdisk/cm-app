###############################################################################
# LogisticOS — TimescaleDB Module
# Provisions TimescaleDB on AWS RDS PostgreSQL using the TimescaleDB extension.
# TimescaleDB is used for:
#   - GPS/telemetry time-series data (fleet, driver-ops)
#   - Metrics ingestion (system and business KPIs)
#
# Implementation: RDS PostgreSQL with TimescaleDB extension installed via
# custom parameter group + init script, or dedicated RDS Custom instance.
# For simplicity and cost (dev/staging), we use a separate RDS PostgreSQL
# instance with the timescaledb extension pre-enabled via parameter group.
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
  description = "RDS instance identifier."
  type        = string
}

variable "environment" {
  description = "Deployment environment (dev, staging, production)."
  type        = string
}

variable "vpc_id" {
  description = "VPC ID where TimescaleDB will be deployed."
  type        = string
}

variable "subnet_ids" {
  description = "List of private subnet IDs for the DB subnet group."
  type        = list(string)
}

variable "allowed_security_group_ids" {
  description = "Security group IDs allowed to connect (e.g. EKS nodes)."
  type        = list(string)
  default     = []
}

variable "engine_version" {
  description = "PostgreSQL engine version (TimescaleDB 2.x supports PG 14–16)."
  type        = string
  default     = "15.4"
}

variable "instance_class" {
  description = "RDS instance class."
  type        = string
  default     = "db.t3.medium"
}

variable "allocated_storage" {
  description = "Initial allocated storage in GiB."
  type        = number
  default     = 50
}

variable "max_allocated_storage" {
  description = "Maximum storage autoscaling threshold in GiB."
  type        = number
  default     = 200
}

variable "multi_az" {
  description = "Enable Multi-AZ for HA."
  type        = bool
  default     = false
}

variable "backup_retention_days" {
  description = "Number of days to retain automated backups."
  type        = number
  default     = 7
}

variable "deletion_protection" {
  description = "Enable deletion protection."
  type        = bool
  default     = false
}

variable "skip_final_snapshot" {
  description = "Skip final snapshot on deletion (dev only)."
  type        = bool
  default     = true
}

variable "db_name" {
  description = "Name of the initial database."
  type        = string
  default     = "logisticos_timeseries"
}

variable "master_username" {
  description = "Master database username."
  type        = string
  default     = "logisticos_ts_admin"
}

variable "tags" {
  description = "Tags to apply to all resources."
  type        = map(string)
  default     = {}
}

###############################################################################
# Random password for RDS master user
###############################################################################

resource "random_password" "master_password" {
  length           = 32
  special          = true
  override_special = "!#$%&*()-_=+[]{}<>:?"
}

resource "aws_secretsmanager_secret" "master_password" {
  name        = "logisticos/${var.environment}/timescaledb/master-password"
  description = "TimescaleDB master password for ${var.identifier}"
  tags        = var.tags
}

resource "aws_secretsmanager_secret_version" "master_password" {
  secret_id     = aws_secretsmanager_secret.master_password.id
  secret_string = jsonencode({
    username = var.master_username
    password = random_password.master_password.result
    host     = aws_db_instance.timescaledb.address
    port     = 5432
    dbname   = var.db_name
    url      = "postgres://${var.master_username}:${random_password.master_password.result}@${aws_db_instance.timescaledb.address}:5432/${var.db_name}"
  })
}

###############################################################################
# Parameter Group — enables TimescaleDB extension
###############################################################################

resource "aws_db_parameter_group" "timescaledb" {
  name        = "${var.identifier}-timescaledb-pg15"
  family      = "postgres15"
  description = "Parameter group for TimescaleDB on PostgreSQL 15"
  tags        = var.tags

  parameter {
    name  = "shared_preload_libraries"
    value = "timescaledb,pg_stat_statements"
    apply_method = "pending-reboot"
  }

  parameter {
    name  = "max_connections"
    value = "200"
  }

  parameter {
    name  = "work_mem"
    value = "16384"  # 16MB per query operation
  }

  parameter {
    name  = "maintenance_work_mem"
    value = "262144"  # 256MB for maintenance
  }

  parameter {
    name  = "timescaledb.max_background_workers"
    value = "8"
  }

  parameter {
    name  = "timescaledb.telemetry_level"
    value = "off"
    apply_method = "pending-reboot"
  }

  lifecycle {
    create_before_destroy = true
  }
}

###############################################################################
# DB Subnet Group
###############################################################################

resource "aws_db_subnet_group" "timescaledb" {
  name        = "${var.identifier}-subnet-group"
  subnet_ids  = var.subnet_ids
  description = "TimescaleDB subnet group for ${var.identifier}"
  tags        = var.tags
}

###############################################################################
# Security Group
###############################################################################

resource "aws_security_group" "timescaledb" {
  name        = "${var.identifier}-sg"
  description = "Security group for TimescaleDB instance ${var.identifier}"
  vpc_id      = var.vpc_id
  tags        = merge(var.tags, { Name = "${var.identifier}-sg" })

  ingress {
    description     = "PostgreSQL from EKS nodes"
    from_port       = 5432
    to_port         = 5432
    protocol        = "tcp"
    security_groups = var.allowed_security_group_ids
  }

  egress {
    description = "All outbound"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  lifecycle {
    create_before_destroy = true
  }
}

###############################################################################
# RDS PostgreSQL Instance (TimescaleDB)
###############################################################################

resource "aws_db_instance" "timescaledb" {
  identifier = var.identifier

  engine               = "postgres"
  engine_version       = var.engine_version
  instance_class       = var.instance_class
  parameter_group_name = aws_db_parameter_group.timescaledb.name
  db_subnet_group_name = aws_db_subnet_group.timescaledb.name
  vpc_security_group_ids = [aws_security_group.timescaledb.id]

  db_name  = var.db_name
  username = var.master_username
  password = random_password.master_password.result

  allocated_storage     = var.allocated_storage
  max_allocated_storage = var.max_allocated_storage
  storage_type          = "gp3"
  storage_encrypted     = true

  multi_az               = var.multi_az
  backup_retention_period = var.backup_retention_days
  backup_window           = "03:00-04:00"
  maintenance_window      = "Mon:04:00-Mon:05:00"

  deletion_protection = var.deletion_protection
  skip_final_snapshot = var.skip_final_snapshot
  final_snapshot_identifier = var.skip_final_snapshot ? null : "${var.identifier}-final-snapshot"

  performance_insights_enabled          = true
  performance_insights_retention_period = 7

  enabled_cloudwatch_logs_exports = ["postgresql", "upgrade"]

  auto_minor_version_upgrade = true
  copy_tags_to_snapshot      = true

  tags = merge(var.tags, {
    Name    = var.identifier
    Service = "timescaledb"
    Purpose = "time-series data: GPS telemetry, fleet, metrics"
  })
}

###############################################################################
# Outputs
###############################################################################

output "endpoint" {
  description = "TimescaleDB RDS endpoint."
  value       = aws_db_instance.timescaledb.address
}

output "port" {
  description = "TimescaleDB port."
  value       = aws_db_instance.timescaledb.port
}

output "db_name" {
  description = "TimescaleDB database name."
  value       = var.db_name
}

output "master_password_secret_arn" {
  description = "ARN of the Secrets Manager secret containing the TimescaleDB master credentials."
  value       = aws_secretsmanager_secret.master_password.arn
}

output "connection_url_secret_arn" {
  description = "Same secret ARN — contains full connection URL in the 'url' key."
  value       = aws_secretsmanager_secret.master_password.arn
}
