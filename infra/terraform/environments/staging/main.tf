###############################################################################
# LogisticOS — Staging Environment Root Module
#
# Purpose : Mirror of production topology for integration/UAT/pre-prod testing.
#            Sized between dev and prod: single NAT GW for cost saving but
#            multi-AZ RDS/Redis to validate replication behaviour.
#
# Region  : ap-southeast-1 (Singapore)
# Owner   : Platform Engineering
###############################################################################

terraform {
  required_version = ">= 1.7.0"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.40"
    }
    kubernetes = {
      source  = "hashicorp/kubernetes"
      version = "~> 2.27"
    }
    helm = {
      source  = "hashicorp/helm"
      version = "~> 2.13"
    }
  }

  backend "s3" {
    bucket         = "logisticos-terraform-state"
    key            = "staging/terraform.tfstate"
    region         = "ap-southeast-1"
    encrypt        = true
    dynamodb_table = "logisticos-terraform-locks"
  }
}

###############################################################################
# Provider Configuration
###############################################################################

provider "aws" {
  region = "ap-southeast-1"

  default_tags {
    tags = local.common_tags
  }
}

# Kubernetes provider — wired to the EKS cluster created in this root module.
# Uses the EKS cluster auth data sources so no out-of-band kubeconfig is needed.
provider "kubernetes" {
  host                   = data.aws_eks_cluster.staging.endpoint
  cluster_ca_certificate = base64decode(data.aws_eks_cluster.staging.certificate_authority[0].data)
  token                  = data.aws_eks_cluster_auth.staging.token
}

provider "helm" {
  kubernetes {
    host                   = data.aws_eks_cluster.staging.endpoint
    cluster_ca_certificate = base64decode(data.aws_eks_cluster.staging.certificate_authority[0].data)
    token                  = data.aws_eks_cluster_auth.staging.token
  }
}

###############################################################################
# Locals
###############################################################################

locals {
  environment  = "staging"
  project      = "LogisticOS"
  region       = "ap-southeast-1"
  cluster_name = "logisticos-staging"

  # CIDR allocation — staging is 10.2.0.0/16 (dev=10.0, prod=10.1, staging=10.2)
  vpc_cidr = "10.2.0.0/16"

  availability_zones = [
    "ap-southeast-1a",
    "ap-southeast-1b",
    "ap-southeast-1c",
  ]

  # /19 subnets give 8190 IPs each — sufficient for staging workloads
  private_subnets = ["10.2.0.0/19", "10.2.32.0/19", "10.2.64.0/19"]
  public_subnets  = ["10.2.96.0/24", "10.2.97.0/24", "10.2.98.0/24"]

  # Intra subnets are used for RDS and ElastiCache — no route to internet
  intra_subnets = ["10.2.128.0/24", "10.2.129.0/24", "10.2.130.0/24"]

  common_tags = {
    Environment = "staging"
    Project     = local.project
    ManagedBy   = "terraform"
    Owner       = "platform-engineering"
    CostCenter  = "engineering"
    Region      = local.region
  }
}

###############################################################################
# Data Sources — EKS Cluster Auth
# These are declared here (referencing the EKS module output) so that the
# kubernetes/helm providers can be fully resolved after the EKS module runs.
###############################################################################

data "aws_eks_cluster" "staging" {
  name = module.eks.cluster_name

  depends_on = [module.eks]
}

data "aws_eks_cluster_auth" "staging" {
  name = module.eks.cluster_name

  depends_on = [module.eks]
}

# Retrieve current AWS account ID for IAM role ARN construction
data "aws_caller_identity" "current" {}

# Retrieve the Secrets Manager secret for ElastiCache auth token so it can be
# passed to the elasticache module without being stored in tfstate as plaintext.
data "aws_secretsmanager_secret_version" "redis_auth_token" {
  secret_id = aws_secretsmanager_secret.redis_auth_token.id

  depends_on = [aws_secretsmanager_secret_version.redis_auth_token]
}

###############################################################################
# Secrets — Redis Auth Token
# Generated once and stored in Secrets Manager; rotated via Lambda rotation.
###############################################################################

resource "random_password" "redis_auth_token" {
  length           = 64
  special          = false # Redis AUTH tokens must be alphanumeric only
  override_special = ""
}

resource "aws_secretsmanager_secret" "redis_auth_token" {
  name                    = "/${local.environment}/logisticos/redis/auth-token"
  description             = "Redis AUTH token for LogisticOS staging ElastiCache cluster"
  recovery_window_in_days = 7

  tags = local.common_tags
}

resource "aws_secretsmanager_secret_version" "redis_auth_token" {
  secret_id     = aws_secretsmanager_secret.redis_auth_token.id
  secret_string = random_password.redis_auth_token.result
}

###############################################################################
# Module — VPC
###############################################################################

module "vpc" {
  source  = "terraform-aws-modules/vpc/aws"
  version = "~> 5.7"

  name = "logisticos-${local.environment}"
  cidr = local.vpc_cidr

  azs             = local.availability_zones
  private_subnets = local.private_subnets
  public_subnets  = local.public_subnets
  intra_subnets   = local.intra_subnets

  # Single NAT gateway — cost saving for staging vs prod (prod uses one-per-AZ).
  # NOTE: single NAT is an accepted staging trade-off; AZ failure will disrupt
  # private-subnet egress. Do NOT replicate this setting to production.
  enable_nat_gateway   = true
  single_nat_gateway   = true
  enable_dns_hostnames = true
  enable_dns_support   = true

  # EKS-required subnet tags — must be present for ALB controller and IRSA
  public_subnet_tags = {
    "kubernetes.io/cluster/${local.cluster_name}" = "shared"
    "kubernetes.io/role/elb"                      = "1"
  }

  private_subnet_tags = {
    "kubernetes.io/cluster/${local.cluster_name}" = "shared"
    "kubernetes.io/role/internal-elb"             = "1"
    "karpenter.sh/discovery"                      = local.cluster_name
  }

  # VPC Flow Logs — ship to CloudWatch for security & debugging
  enable_flow_log                      = true
  create_flow_log_cloudwatch_log_group = true
  create_flow_log_cloudwatch_iam_role  = true
  flow_log_max_aggregation_interval    = 60

  tags = local.common_tags
}

###############################################################################
# Module — EKS
###############################################################################

module "eks" {
  source  = "terraform-aws-modules/eks/aws"
  version = "~> 20.8"

  cluster_name    = local.cluster_name
  cluster_version = "1.29"

  vpc_id                         = module.vpc.vpc_id
  subnet_ids                     = module.vpc.private_subnets
  cluster_endpoint_public_access = true

  # IRSA — required for service accounts to assume IAM roles (MSK, S3, Secrets)
  enable_irsa = true

  # Cluster add-ons — managed by EKS for automatic updates
  cluster_addons = {
    coredns = {
      most_recent = true
    }
    kube-proxy = {
      most_recent = true
    }
    vpc-cni = {
      most_recent    = true
      before_compute = true
      configuration_values = jsonencode({
        env = {
          ENABLE_PREFIX_DELEGATION = "true"
          WARM_PREFIX_TARGET       = "1"
        }
      })
    }
    aws-ebs-csi-driver = {
      most_recent              = true
      service_account_role_arn = module.ebs_csi_irsa.iam_role_arn
    }
  }

  # Node groups
  eks_managed_node_groups = {
    # System node group — runs platform add-ons (CoreDNS, Karpenter, Istio)
    system = {
      name           = "${local.cluster_name}-system"
      instance_types = ["t3.large"]

      min_size     = 2
      max_size     = 4
      desired_size = 2

      labels = {
        role = "system"
      }

      taints = [
        {
          key    = "CriticalAddonsOnly"
          value  = "true"
          effect = "NO_SCHEDULE"
        }
      ]

      update_config = {
        max_unavailable_percentage = 33
      }
    }

    # Application node group — runs LogisticOS microservices
    app = {
      name           = "${local.cluster_name}-app"
      instance_types = ["t3.xlarge"]

      min_size     = 2
      max_size     = 20
      desired_size = 3

      labels = {
        role        = "app"
        environment = local.environment
      }

      block_device_mappings = {
        xvda = {
          device_name = "/dev/xvda"
          ebs = {
            volume_size           = 100
            volume_type           = "gp3"
            iops                  = 3000
            throughput            = 125
            encrypted             = true
            delete_on_termination = true
          }
        }
      }

      update_config = {
        max_unavailable_percentage = 33
      }
    }
  }

  # Cluster security group additional rules
  cluster_security_group_additional_rules = {
    ingress_nodes_ephemeral_ports_tcp = {
      description                = "Nodes on ephemeral ports"
      protocol                   = "tcp"
      from_port                  = 1025
      to_port                    = 65535
      type                       = "ingress"
      source_node_security_group = true
    }
  }

  node_security_group_additional_rules = {
    ingress_self_all = {
      description = "Node to node all ports/protocols"
      protocol    = "-1"
      from_port   = 0
      to_port     = 0
      type        = "ingress"
      self        = true
    }
  }

  tags = merge(local.common_tags, {
    "karpenter.sh/discovery" = local.cluster_name
  })
}

# IRSA role for EBS CSI driver
module "ebs_csi_irsa" {
  source  = "terraform-aws-modules/iam/aws//modules/iam-role-for-service-accounts-eks"
  version = "~> 5.37"

  role_name             = "${local.cluster_name}-ebs-csi"
  attach_ebs_csi_policy = true

  oidc_providers = {
    main = {
      provider_arn               = module.eks.oidc_provider_arn
      namespace_service_accounts = ["kube-system:ebs-csi-controller-sa"]
    }
  }

  tags = local.common_tags
}

###############################################################################
# Module — RDS (PostgreSQL — primary relational store)
###############################################################################

module "rds" {
  source  = "terraform-aws-modules/rds/aws"
  version = "~> 6.5"

  identifier = "logisticos-staging"

  engine               = "postgres"
  engine_version       = "16.2"
  family               = "postgres16"
  major_engine_version = "16"
  instance_class       = "db.t3.large"

  allocated_storage     = 100
  max_allocated_storage = 500
  storage_type          = "gp3"
  storage_encrypted     = true

  db_name  = "logisticos"
  username = "logisticos_admin"
  port     = 5432

  # Staging mirrors prod: Multi-AZ for replication and failover testing
  multi_az = true

  db_subnet_group_name   = aws_db_subnet_group.main.name
  vpc_security_group_ids = [aws_security_group.rds.id]

  # Parameter group — enable logical replication for CDC (Debezium → Kafka)
  create_db_parameter_group    = true
  parameter_group_name         = "logisticos-staging-postgres16"
  parameter_group_use_name_prefix = false
  parameters = [
    { name = "wal_level", value = "logical", apply_method = "pending-reboot" },
    { name = "max_wal_senders", value = "10", apply_method = "pending-reboot" },
    { name = "max_replication_slots", value = "10", apply_method = "pending-reboot" },
    { name = "shared_preload_libraries", value = "pg_stat_statements,pgaudit", apply_method = "pending-reboot" },
    { name = "log_statement", value = "ddl", apply_method = "immediate" },
    { name = "log_min_duration_statement", value = "1000", apply_method = "immediate" }, # log queries > 1s
    { name = "idle_in_transaction_session_timeout", value = "30000", apply_method = "immediate" },
  ]

  backup_retention_period  = 14
  backup_window            = "02:00-03:00"
  maintenance_window       = "Mon:03:00-Mon:04:00"
  deletion_protection      = false # staging — intentional

  enabled_cloudwatch_logs_exports = ["postgresql", "upgrade"]
  monitoring_interval             = 60
  monitoring_role_name            = "logisticos-staging-rds-monitoring"
  create_monitoring_role          = true

  skip_final_snapshot = true

  tags = local.common_tags
}

# Dedicated DB subnet group using intra subnets (no internet route)
resource "aws_db_subnet_group" "main" {
  name       = "logisticos-${local.environment}"
  subnet_ids = module.vpc.intra_subnets

  tags = merge(local.common_tags, {
    Name = "logisticos-${local.environment}"
  })
}

resource "aws_security_group" "rds" {
  name        = "logisticos-${local.environment}-rds"
  description = "Allow PostgreSQL access from EKS nodes"
  vpc_id      = module.vpc.vpc_id

  ingress {
    description     = "PostgreSQL from EKS nodes"
    from_port       = 5432
    to_port         = 5432
    protocol        = "tcp"
    security_groups = [module.eks.node_security_group_id]
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = merge(local.common_tags, {
    Name = "logisticos-${local.environment}-rds"
  })
}

###############################################################################
# ElastiCache — Redis 7.1 Replication Group
# Inline here (not a module) for the staging environment.
# Multi-AZ with one replica for replication path testing.
###############################################################################

resource "aws_elasticache_subnet_group" "main" {
  name       = "logisticos-${local.environment}"
  subnet_ids = module.vpc.intra_subnets

  tags = local.common_tags
}

resource "aws_security_group" "redis" {
  name        = "logisticos-${local.environment}-redis"
  description = "Allow Redis access from EKS nodes"
  vpc_id      = module.vpc.vpc_id

  ingress {
    description     = "Redis from EKS nodes"
    from_port       = 6379
    to_port         = 6379
    protocol        = "tcp"
    security_groups = [module.eks.node_security_group_id]
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = merge(local.common_tags, {
    Name = "logisticos-${local.environment}-redis"
  })
}

resource "aws_elasticache_parameter_group" "redis71" {
  name   = "logisticos-${local.environment}-redis71"
  family = "redis7"

  # Keyspace notifications: K=keyspace, x=expired, g=generic — required by
  # the Engagement Engine for TTL-based session and token expiry handling.
  parameter {
    name  = "notify-keyspace-events"
    value = "Kxg"
  }

  parameter {
    name  = "maxmemory-policy"
    value = "allkeys-lru"
  }

  parameter {
    name  = "tcp-keepalive"
    value = "300"
  }

  tags = local.common_tags
}

resource "aws_elasticache_replication_group" "main" {
  replication_group_id = "logisticos-${local.environment}"
  description          = "LogisticOS staging Redis — session cache, pub/sub, rate limiting"

  node_type            = "cache.t3.medium"
  num_cache_clusters   = 2 # 1 primary + 1 replica
  engine_version       = "7.1"
  port                 = 6379
  parameter_group_name = aws_elasticache_parameter_group.redis71.name
  subnet_group_name    = aws_elasticache_subnet_group.main.name
  security_group_ids   = [aws_security_group.redis.id]

  automatic_failover_enabled = true
  multi_az_enabled           = true

  at_rest_encryption_enabled = true
  transit_encryption_enabled = true
  auth_token                 = data.aws_secretsmanager_secret_version.redis_auth_token.secret_string

  # Maintenance and snapshots
  maintenance_window       = "tue:04:00-tue:05:00"
  snapshot_window          = "03:00-04:00"
  snapshot_retention_limit = 7

  # Auto minor version upgrades in staging to catch compatibility issues early
  auto_minor_version_upgrade = true

  apply_immediately = true # OK for staging

  log_delivery_configuration {
    destination      = aws_cloudwatch_log_group.redis_slow_log.name
    destination_type = "cloudwatch-logs"
    log_format       = "json"
    log_type         = "slow-log"
  }

  log_delivery_configuration {
    destination      = aws_cloudwatch_log_group.redis_engine_log.name
    destination_type = "cloudwatch-logs"
    log_format       = "json"
    log_type         = "engine-log"
  }

  tags = local.common_tags
}

resource "aws_cloudwatch_log_group" "redis_slow_log" {
  name              = "/logisticos/${local.environment}/elasticache/slow-log"
  retention_in_days = 14

  tags = local.common_tags
}

resource "aws_cloudwatch_log_group" "redis_engine_log" {
  name              = "/logisticos/${local.environment}/elasticache/engine-log"
  retention_in_days = 14

  tags = local.common_tags
}

###############################################################################
# Module — MSK (Kafka)
###############################################################################

module "msk" {
  source = "../../modules/msk"

  cluster_name          = "logisticos-${local.environment}"
  kafka_version         = "3.6.0"
  instance_type         = "kafka.t3.small"
  vpc_id                = module.vpc.vpc_id
  subnet_ids            = module.vpc.private_subnets
  number_of_broker_nodes = 3
  ebs_volume_size       = 100

  tags = local.common_tags
}

###############################################################################
# Outputs
###############################################################################

output "vpc_id" {
  description = "Staging VPC ID"
  value       = module.vpc.vpc_id
}

output "private_subnets" {
  description = "Private subnet IDs"
  value       = module.vpc.private_subnets
}

output "eks_cluster_name" {
  description = "EKS cluster name"
  value       = module.eks.cluster_name
}

output "eks_cluster_endpoint" {
  description = "EKS API server endpoint"
  value       = module.eks.cluster_endpoint
  sensitive   = true
}

output "eks_oidc_provider_arn" {
  description = "OIDC provider ARN for IRSA"
  value       = module.eks.oidc_provider_arn
}

output "rds_endpoint" {
  description = "RDS primary endpoint"
  value       = module.rds.db_instance_endpoint
  sensitive   = true
}

output "redis_primary_endpoint" {
  description = "ElastiCache primary endpoint"
  value       = aws_elasticache_replication_group.main.primary_endpoint_address
  sensitive   = true
}

output "redis_reader_endpoint" {
  description = "ElastiCache reader endpoint"
  value       = aws_elasticache_replication_group.main.reader_endpoint_address
  sensitive   = true
}

output "redis_auth_token_secret_arn" {
  description = "ARN of the Secrets Manager secret containing the Redis auth token"
  value       = aws_secretsmanager_secret.redis_auth_token.arn
}

output "msk_bootstrap_brokers_sasl_iam" {
  description = "MSK SASL/IAM bootstrap broker string"
  value       = module.msk.bootstrap_brokers_sasl_iam
  sensitive   = true
}

output "msk_cluster_arn" {
  description = "MSK cluster ARN"
  value       = module.msk.cluster_arn
}
