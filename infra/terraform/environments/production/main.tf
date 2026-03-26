###############################################################################
# LogisticOS — Production Environment
# Root Terraform module for the production environment (ap-southeast-1).
# Provisions VPC, EKS cluster, RDS PostgreSQL (Multi-AZ), and Redis cluster.
# All resources are hardened: multi-AZ, deletion protection, longer retention.
###############################################################################

terraform {
  required_version = ">= 1.6.0"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
    kubernetes = {
      source  = "hashicorp/kubernetes"
      version = "~> 2.0"
    }
    helm = {
      source  = "hashicorp/helm"
      version = "~> 2.0"
    }
  }

  backend "s3" {
    bucket         = "logisticos-terraform-state"
    key            = "production/terraform.tfstate"
    region         = "ap-southeast-1"
    encrypt        = true
    dynamodb_table = "logisticos-terraform-state-lock"
  }
}

###############################################################################
# Provider Configuration
###############################################################################

provider "aws" {
  region = "ap-southeast-1"

  default_tags {
    tags = {
      Environment = "production"
      Project     = "LogisticOS"
      ManagedBy   = "terraform"
    }
  }
}

provider "kubernetes" {
  host                   = module.eks.cluster_endpoint
  cluster_ca_certificate = base64decode(module.eks.cluster_certificate_authority_data)

  exec {
    api_version = "client.authentication.k8s.io/v1beta1"
    command     = "aws"
    args        = ["eks", "get-token", "--cluster-name", module.eks.cluster_name]
  }
}

provider "helm" {
  kubernetes {
    host                   = module.eks.cluster_endpoint
    cluster_ca_certificate = base64decode(module.eks.cluster_certificate_authority_data)

    exec {
      api_version = "client.authentication.k8s.io/v1beta1"
      command     = "aws"
      args        = ["eks", "get-token", "--cluster-name", module.eks.cluster_name]
    }
  }
}

###############################################################################
# Data Sources
###############################################################################

data "aws_caller_identity" "current" {}
data "aws_region" "current" {}

###############################################################################
# VPC Module
###############################################################################

module "vpc" {
  source = "../../modules/vpc"

  region      = "ap-southeast-1"
  environment = "production"

  vpc_cidr = "10.1.0.0/16"

  azs = [
    "ap-southeast-1a",
    "ap-southeast-1b",
    "ap-southeast-1c",
  ]

  public_subnet_cidrs = [
    "10.1.0.0/22",   # ap-southeast-1a — 1022 hosts
    "10.1.4.0/22",   # ap-southeast-1b
    "10.1.8.0/22",   # ap-southeast-1c
  ]

  private_subnet_cidrs = [
    "10.1.32.0/19",  # ap-southeast-1a — 8190 hosts
    "10.1.64.0/19",  # ap-southeast-1b
    "10.1.96.0/19",  # ap-southeast-1c
  ]

  enable_nat_gateway = true
  single_nat_gateway = false  # One NAT GW per AZ for full HA

  tags = {
    Environment = "production"
  }
}

###############################################################################
# EKS Module
###############################################################################

module "eks" {
  source = "../../modules/eks"

  cluster_name    = "logisticos-prod"
  cluster_version = "1.29"
  environment     = "production"

  vpc_id             = module.vpc.vpc_id
  private_subnet_ids = module.vpc.private_subnet_ids

  # Application node group — memory-optimised for Rust services
  node_instance_types = ["m5.xlarge"]
  desired_size        = 5
  min_size            = 3
  max_size            = 50

  # System node group — slightly larger for Istio + observability stack
  system_node_instance_types = ["m5.large"]
  system_desired_size        = 3
  system_min_size            = 3
  system_max_size            = 6

  enable_irsa = true

  tags = {
    Environment = "production"
  }
}

###############################################################################
# RDS Module
###############################################################################

module "rds" {
  source = "../../modules/rds"

  identifier     = "logisticos-prod"
  engine_version = "15.4"
  instance_class = "db.r6g.xlarge"
  environment    = "production"

  vpc_id     = module.vpc.vpc_id
  subnet_ids = module.vpc.private_subnet_ids

  allowed_security_group_ids = [
    module.eks.cluster_security_group_id,
  ]

  db_name         = "logisticos"
  master_username = "logisticos_admin"

  # Production: Multi-AZ, long retention, full protection
  multi_az              = true
  backup_retention_days = 30
  deletion_protection   = true
  skip_final_snapshot   = false

  allocated_storage     = 500
  max_allocated_storage = 5000

  enable_performance_insights           = true
  performance_insights_retention_period = 731  # 2 years (maximum)

  maintenance_window = "sun:02:00-sun:03:00"
  backup_window      = "00:00-01:00"

  tags = {
    Environment = "production"
    Criticality = "high"
  }
}

###############################################################################
# Redis — ElastiCache Replication Group
# Used for: session management, rate limiting, pub/sub, caching, Kafka offset
# tracking, and driver location cache across all LogisticOS microservices.
###############################################################################

resource "aws_security_group" "redis" {
  name        = "logisticos-prod-redis-sg"
  description = "Security group for LogisticOS production Redis cluster — restricts access to EKS nodes only"
  vpc_id      = module.vpc.vpc_id

  ingress {
    description     = "Redis access from EKS cluster"
    from_port       = 6379
    to_port         = 6379
    protocol        = "tcp"
    security_groups = [module.eks.cluster_security_group_id]
  }

  egress {
    description = "Allow all outbound traffic"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = {
    Name        = "logisticos-prod-redis-sg"
    Environment = "production"
    Project     = "LogisticOS"
    ManagedBy   = "terraform"
  }
}

resource "aws_elasticache_subnet_group" "redis" {
  name        = "logisticos-prod-redis-subnet-group"
  description = "Subnet group for LogisticOS production Redis cluster"
  subnet_ids  = module.vpc.private_subnet_ids

  tags = {
    Environment = "production"
    Project     = "LogisticOS"
    ManagedBy   = "terraform"
  }
}

resource "aws_kms_key" "redis" {
  description             = "KMS key for LogisticOS production Redis at-rest encryption"
  deletion_window_in_days = 30
  enable_key_rotation     = true

  tags = {
    Name        = "logisticos-prod-redis-kms"
    Environment = "production"
    Project     = "LogisticOS"
    ManagedBy   = "terraform"
  }
}

resource "aws_kms_alias" "redis" {
  name          = "alias/logisticos-prod-redis"
  target_key_id = aws_kms_key.redis.key_id
}

resource "aws_elasticache_replication_group" "redis" {
  replication_group_id = "logisticos-prod-redis"
  description          = "LogisticOS production Redis cluster — session, cache, pub/sub, rate limiting"

  # Engine
  engine               = "redis"
  engine_version       = "7.1"
  node_type            = "cache.r6g.large"
  port                 = 6379

  # Cluster topology — 1 primary shard + 2 read replicas across AZs
  num_cache_clusters   = 3
  multi_az_enabled     = true
  automatic_failover_enabled = true

  # Network
  subnet_group_name  = aws_elasticache_subnet_group.redis.name
  security_group_ids = [aws_security_group.redis.id]

  # Parameter group
  parameter_group_name = aws_elasticache_parameter_group.redis.name

  # Encryption
  at_rest_encryption_enabled = true
  transit_encryption_enabled = true
  kms_key_id                 = aws_kms_key.redis.arn
  auth_token                 = random_password.redis_auth.result

  # Maintenance & snapshots
  snapshot_retention_limit = 7
  snapshot_window          = "00:30-01:30"
  maintenance_window       = "sun:02:00-sun:03:00"

  # Notifications
  notification_topic_arn = aws_sns_topic.redis_notifications.arn

  # Auto minor version upgrade
  auto_minor_version_upgrade = true
  apply_immediately          = false

  tags = {
    Name        = "logisticos-prod-redis"
    Environment = "production"
    Project     = "LogisticOS"
    ManagedBy   = "terraform"
    Criticality = "high"
  }

  depends_on = [
    aws_elasticache_subnet_group.redis,
    aws_elasticache_parameter_group.redis,
  ]
}

resource "aws_elasticache_parameter_group" "redis" {
  name        = "logisticos-prod-redis7-params"
  family      = "redis7"
  description = "LogisticOS optimised Redis 7 parameters for production"

  # Disable dangerous commands
  parameter {
    name  = "close-on-slave-write"
    value = "yes"
  }

  # Memory eviction policy — allkeys-lru suits cache + session mix
  parameter {
    name  = "maxmemory-policy"
    value = "allkeys-lru"
  }

  # Notify on key space events (for TTL-based triggers in engagement engine)
  parameter {
    name  = "notify-keyspace-events"
    value = "Ex"
  }

  # Slow log
  parameter {
    name  = "slowlog-log-slower-than"
    value = "10000"  # 10ms
  }

  parameter {
    name  = "slowlog-max-len"
    value = "128"
  }

  # Active defrag
  parameter {
    name  = "activedefrag"
    value = "yes"
  }

  tags = {
    Environment = "production"
    Project     = "LogisticOS"
    ManagedBy   = "terraform"
  }
}

resource "random_password" "redis_auth" {
  length           = 64
  special          = false  # Redis AUTH token must be alphanumeric only
}

resource "aws_secretsmanager_secret" "redis_auth" {
  name                    = "logisticos/production/redis/auth-token"
  description             = "AUTH token for LogisticOS production Redis cluster"
  recovery_window_in_days = 7

  tags = {
    Environment = "production"
    Project     = "LogisticOS"
    ManagedBy   = "terraform"
  }
}

resource "aws_secretsmanager_secret_version" "redis_auth" {
  secret_id = aws_secretsmanager_secret.redis_auth.id
  secret_string = jsonencode({
    auth_token         = random_password.redis_auth.result
    primary_endpoint   = aws_elasticache_replication_group.redis.primary_endpoint_address
    reader_endpoint    = aws_elasticache_replication_group.redis.reader_endpoint_address
    port               = 6379
  })
}

###############################################################################
# SNS Topic for Redis event notifications
###############################################################################

resource "aws_sns_topic" "redis_notifications" {
  name              = "logisticos-prod-redis-notifications"
  kms_master_key_id = "alias/aws/sns"

  tags = {
    Environment = "production"
    Project     = "LogisticOS"
    ManagedBy   = "terraform"
  }
}

###############################################################################
# CloudWatch Alarms — Redis
###############################################################################

resource "aws_cloudwatch_metric_alarm" "redis_cpu_high" {
  alarm_name          = "logisticos-prod-redis-cpu-high"
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = 3
  metric_name         = "EngineCPUUtilization"
  namespace           = "AWS/ElastiCache"
  period              = 300
  statistic           = "Average"
  threshold           = 70
  alarm_description   = "Redis engine CPU utilization exceeds 70% in production"
  treat_missing_data  = "notBreaching"

  dimensions = {
    ReplicationGroupId = aws_elasticache_replication_group.redis.id
  }

  tags = {
    Environment = "production"
    Project     = "LogisticOS"
    ManagedBy   = "terraform"
  }
}

resource "aws_cloudwatch_metric_alarm" "redis_memory_high" {
  alarm_name          = "logisticos-prod-redis-memory-high"
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = 2
  metric_name         = "DatabaseMemoryUsagePercentage"
  namespace           = "AWS/ElastiCache"
  period              = 300
  statistic           = "Average"
  threshold           = 75
  alarm_description   = "Redis memory usage exceeds 75% in production"
  treat_missing_data  = "notBreaching"

  dimensions = {
    ReplicationGroupId = aws_elasticache_replication_group.redis.id
  }

  tags = {
    Environment = "production"
    Project     = "LogisticOS"
    ManagedBy   = "terraform"
  }
}

resource "aws_cloudwatch_metric_alarm" "redis_evictions_high" {
  alarm_name          = "logisticos-prod-redis-evictions-high"
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = 2
  metric_name         = "Evictions"
  namespace           = "AWS/ElastiCache"
  period              = 300
  statistic           = "Sum"
  threshold           = 1000
  alarm_description   = "Redis eviction count exceeds threshold — consider scaling in production"
  treat_missing_data  = "notBreaching"

  dimensions = {
    ReplicationGroupId = aws_elasticache_replication_group.redis.id
  }

  tags = {
    Environment = "production"
    Project     = "LogisticOS"
    ManagedBy   = "terraform"
  }
}

###############################################################################
# Outputs
###############################################################################

output "vpc_id" {
  description = "VPC ID for the production environment."
  value       = module.vpc.vpc_id
}

output "eks_cluster_endpoint" {
  description = "EKS cluster API server endpoint."
  value       = module.eks.cluster_endpoint
}

output "eks_cluster_name" {
  description = "EKS cluster name."
  value       = module.eks.cluster_name
}

output "rds_endpoint" {
  description = "RDS PostgreSQL endpoint."
  value       = module.rds.endpoint
  sensitive   = true
}

output "rds_master_password_secret_arn" {
  description = "ARN of the Secrets Manager secret containing the RDS master password."
  value       = module.rds.master_password_secret_arn
}

output "redis_primary_endpoint" {
  description = "Primary endpoint for the Redis replication group."
  value       = aws_elasticache_replication_group.redis.primary_endpoint_address
  sensitive   = true
}

output "redis_reader_endpoint" {
  description = "Reader endpoint for the Redis replication group (for read replicas)."
  value       = aws_elasticache_replication_group.redis.reader_endpoint_address
  sensitive   = true
}

output "redis_auth_secret_arn" {
  description = "ARN of the Secrets Manager secret containing the Redis AUTH token."
  value       = aws_secretsmanager_secret.redis_auth.arn
}

output "account_id" {
  description = "AWS account ID."
  value       = data.aws_caller_identity.current.account_id
}
