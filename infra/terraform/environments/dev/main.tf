###############################################################################
# LogisticOS — Development Environment
# Root Terraform module for the dev environment (ap-southeast-1).
# Provisions VPC, EKS cluster, and RDS PostgreSQL for development workloads.
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
    key            = "dev/terraform.tfstate"
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
      Environment = "dev"
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
  environment = "dev"

  vpc_cidr = "10.0.0.0/16"

  azs = [
    "ap-southeast-1a",
    "ap-southeast-1b",
    "ap-southeast-1c",
  ]

  public_subnet_cidrs = [
    "10.0.0.0/22",   # ap-southeast-1a — 1022 hosts
    "10.0.4.0/22",   # ap-southeast-1b
    "10.0.8.0/22",   # ap-southeast-1c
  ]

  private_subnet_cidrs = [
    "10.0.32.0/19",  # ap-southeast-1a — 8190 hosts
    "10.0.64.0/19",  # ap-southeast-1b
    "10.0.96.0/19",  # ap-southeast-1c
  ]

  enable_nat_gateway = true
  single_nat_gateway = true  # Single NAT GW to reduce dev costs

  tags = {
    Environment = "dev"
  }
}

###############################################################################
# EKS Module
###############################################################################

module "eks" {
  source = "../../modules/eks"

  cluster_name    = "logisticos-dev"
  cluster_version = "1.29"
  environment     = "dev"

  vpc_id             = module.vpc.vpc_id
  private_subnet_ids = module.vpc.private_subnet_ids

  # Application node group
  node_instance_types = ["t3.large"]
  desired_size        = 3
  min_size            = 2
  max_size            = 10

  # System node group
  system_node_instance_types = ["t3.medium"]
  system_desired_size        = 2
  system_min_size            = 2
  system_max_size            = 3

  enable_irsa = true

  tags = {
    Environment = "dev"
  }
}

###############################################################################
# RDS Module
###############################################################################

module "rds" {
  source = "../../modules/rds"

  identifier     = "logisticos-dev"
  engine_version = "15.4"
  instance_class = "db.t3.medium"
  environment    = "dev"

  vpc_id     = module.vpc.vpc_id
  subnet_ids = module.vpc.private_subnet_ids

  allowed_security_group_ids = [
    module.eks.cluster_security_group_id,
  ]

  db_name         = "logisticos"
  master_username = "logisticos_admin"

  # Dev: single-AZ, shorter backups, no deletion protection
  multi_az              = false
  backup_retention_days = 7
  deletion_protection   = false
  skip_final_snapshot   = true

  allocated_storage     = 50
  max_allocated_storage = 200

  enable_performance_insights           = true
  performance_insights_retention_period = 7

  tags = {
    Environment = "dev"
  }
}

###############################################################################
# Outputs
###############################################################################

output "vpc_id" {
  description = "VPC ID for the dev environment."
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

output "account_id" {
  description = "AWS account ID."
  value       = data.aws_caller_identity.current.account_id
}
