###############################################################################
# LogisticOS — EKS Module
# Provisions an AWS EKS cluster with managed node groups (system + app pools),
# necessary IAM roles, core add-ons, and IRSA support.
###############################################################################

terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
    tls = {
      source  = "hashicorp/tls"
      version = "~> 4.0"
    }
  }
}

###############################################################################
# Variables
###############################################################################

variable "cluster_name" {
  description = "Name of the EKS cluster."
  type        = string
}

variable "cluster_version" {
  description = "Kubernetes version for the EKS cluster."
  type        = string
  default     = "1.29"
}

variable "vpc_id" {
  description = "VPC ID where the EKS cluster will be deployed."
  type        = string
}

variable "private_subnet_ids" {
  description = "List of private subnet IDs for the EKS node groups."
  type        = list(string)
}

variable "node_instance_types" {
  description = "EC2 instance types for the application node group."
  type        = list(string)
  default     = ["t3.large"]
}

variable "system_node_instance_types" {
  description = "EC2 instance types for the system node group (CoreDNS, Istio etc.)."
  type        = list(string)
  default     = ["t3.medium"]
}

variable "desired_size" {
  description = "Desired number of nodes in the application node group."
  type        = number
  default     = 3
}

variable "min_size" {
  description = "Minimum number of nodes in the application node group."
  type        = number
  default     = 2
}

variable "max_size" {
  description = "Maximum number of nodes in the application node group."
  type        = number
  default     = 10
}

variable "system_desired_size" {
  description = "Desired number of nodes in the system node group."
  type        = number
  default     = 2
}

variable "system_min_size" {
  description = "Minimum number of nodes in the system node group."
  type        = number
  default     = 2
}

variable "system_max_size" {
  description = "Maximum number of nodes in the system node group."
  type        = number
  default     = 4
}

variable "enable_irsa" {
  description = "Enable IAM Roles for Service Accounts (IRSA) via OIDC provider."
  type        = bool
  default     = true
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
  common_tags = merge(
    {
      Environment                                       = var.environment
      Project                                           = "LogisticOS"
      ManagedBy                                         = "terraform"
      "kubernetes.io/cluster/${var.cluster_name}"       = "owned"
    },
    var.tags
  )
}

###############################################################################
# IAM — Cluster Role
###############################################################################

resource "aws_iam_role" "cluster" {
  name = "${var.cluster_name}-cluster-role"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = { Service = "eks.amazonaws.com" }
      Action    = "sts:AssumeRole"
    }]
  })

  tags = local.common_tags
}

resource "aws_iam_role_policy_attachment" "cluster_eks_policy" {
  role       = aws_iam_role.cluster.name
  policy_arn = "arn:aws:iam::aws:policy/AmazonEKSClusterPolicy"
}

resource "aws_iam_role_policy_attachment" "cluster_vpc_resource_controller" {
  role       = aws_iam_role.cluster.name
  policy_arn = "arn:aws:iam::aws:policy/AmazonEKSVPCResourceController"
}

###############################################################################
# IAM — Node Role (shared by both node groups)
###############################################################################

resource "aws_iam_role" "node" {
  name = "${var.cluster_name}-node-role"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = { Service = "ec2.amazonaws.com" }
      Action    = "sts:AssumeRole"
    }]
  })

  tags = local.common_tags
}

resource "aws_iam_role_policy_attachment" "node_worker_policy" {
  role       = aws_iam_role.node.name
  policy_arn = "arn:aws:iam::aws:policy/AmazonEKSWorkerNodePolicy"
}

resource "aws_iam_role_policy_attachment" "node_cni_policy" {
  role       = aws_iam_role.node.name
  policy_arn = "arn:aws:iam::aws:policy/AmazonEKS_CNI_Policy"
}

resource "aws_iam_role_policy_attachment" "node_ecr_readonly" {
  role       = aws_iam_role.node.name
  policy_arn = "arn:aws:iam::aws:policy/AmazonEC2ContainerRegistryReadOnly"
}

resource "aws_iam_role_policy_attachment" "node_ssm_policy" {
  role       = aws_iam_role.node.name
  policy_arn = "arn:aws:iam::aws:policy/AmazonSSMManagedInstanceCore"
}

###############################################################################
# Security Group — Cluster (additional rules beyond EKS-managed SG)
###############################################################################

resource "aws_security_group" "cluster_additional" {
  name        = "${var.cluster_name}-cluster-sg-additional"
  description = "Additional security group rules for ${var.cluster_name} EKS cluster"
  vpc_id      = var.vpc_id

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
    description = "Allow all outbound traffic"
  }

  tags = merge(local.common_tags, {
    Name = "${var.cluster_name}-cluster-sg-additional"
  })
}

###############################################################################
# EKS Cluster
###############################################################################

resource "aws_eks_cluster" "main" {
  name     = var.cluster_name
  version  = var.cluster_version
  role_arn = aws_iam_role.cluster.arn

  vpc_config {
    subnet_ids              = var.private_subnet_ids
    endpoint_private_access = true
    endpoint_public_access  = true
    security_group_ids      = [aws_security_group.cluster_additional.id]

    public_access_cidrs = ["0.0.0.0/0"]
  }

  enabled_cluster_log_types = [
    "api",
    "audit",
    "authenticator",
    "controllerManager",
    "scheduler",
  ]

  encryption_config {
    resources = ["secrets"]
    provider {
      key_arn = aws_kms_key.eks.arn
    }
  }

  tags = local.common_tags

  depends_on = [
    aws_iam_role_policy_attachment.cluster_eks_policy,
    aws_iam_role_policy_attachment.cluster_vpc_resource_controller,
    aws_cloudwatch_log_group.eks_cluster,
  ]
}

###############################################################################
# KMS Key for EKS secret encryption
###############################################################################

resource "aws_kms_key" "eks" {
  description             = "KMS key for EKS cluster ${var.cluster_name} secret encryption"
  deletion_window_in_days = 30
  enable_key_rotation     = true

  tags = merge(local.common_tags, {
    Name = "${var.cluster_name}-eks-kms"
  })
}

resource "aws_kms_alias" "eks" {
  name          = "alias/${var.cluster_name}-eks"
  target_key_id = aws_kms_key.eks.key_id
}

###############################################################################
# CloudWatch Log Group for EKS control plane logs
###############################################################################

resource "aws_cloudwatch_log_group" "eks_cluster" {
  name              = "/aws/eks/${var.cluster_name}/cluster"
  retention_in_days = 90

  tags = local.common_tags
}

###############################################################################
# EKS Managed Node Group — System Pool
# Runs kube-system workloads: CoreDNS, kube-proxy, Istio control plane, etc.
###############################################################################

resource "aws_eks_node_group" "system" {
  cluster_name    = aws_eks_cluster.main.name
  node_group_name = "${var.cluster_name}-system"
  node_role_arn   = aws_iam_role.node.arn
  subnet_ids      = var.private_subnet_ids
  instance_types  = var.system_node_instance_types

  capacity_type = "ON_DEMAND"

  scaling_config {
    desired_size = var.system_desired_size
    min_size     = var.system_min_size
    max_size     = var.system_max_size
  }

  update_config {
    max_unavailable = 1
  }

  labels = {
    role                           = "system"
    "node.logisticos.io/pool"     = "system"
  }

  taint {
    key    = "CriticalAddonsOnly"
    value  = "true"
    effect = "NO_SCHEDULE"
  }

  launch_template {
    id      = aws_launch_template.system_nodes.id
    version = aws_launch_template.system_nodes.latest_version
  }

  tags = merge(local.common_tags, {
    Name = "${var.cluster_name}-system-node"
  })

  depends_on = [
    aws_iam_role_policy_attachment.node_worker_policy,
    aws_iam_role_policy_attachment.node_cni_policy,
    aws_iam_role_policy_attachment.node_ecr_readonly,
  ]

  lifecycle {
    ignore_changes = [scaling_config[0].desired_size]
  }
}

###############################################################################
# EKS Managed Node Group — Application Pool
# Runs all LogisticOS microservice workloads.
###############################################################################

resource "aws_eks_node_group" "app" {
  cluster_name    = aws_eks_cluster.main.name
  node_group_name = "${var.cluster_name}-app"
  node_role_arn   = aws_iam_role.node.arn
  subnet_ids      = var.private_subnet_ids
  instance_types  = var.node_instance_types

  capacity_type = "ON_DEMAND"

  scaling_config {
    desired_size = var.desired_size
    min_size     = var.min_size
    max_size     = var.max_size
  }

  update_config {
    max_unavailable_percentage = 25
  }

  labels = {
    role                           = "app"
    "node.logisticos.io/pool"     = "app"
  }

  launch_template {
    id      = aws_launch_template.app_nodes.id
    version = aws_launch_template.app_nodes.latest_version
  }

  tags = merge(local.common_tags, {
    Name = "${var.cluster_name}-app-node"
  })

  depends_on = [
    aws_iam_role_policy_attachment.node_worker_policy,
    aws_iam_role_policy_attachment.node_cni_policy,
    aws_iam_role_policy_attachment.node_ecr_readonly,
  ]

  lifecycle {
    ignore_changes = [scaling_config[0].desired_size]
  }
}

###############################################################################
# Launch Templates
###############################################################################

resource "aws_launch_template" "system_nodes" {
  name_prefix = "${var.cluster_name}-system-nodes-"
  description = "Launch template for ${var.cluster_name} system node group"

  block_device_mappings {
    device_name = "/dev/xvda"
    ebs {
      volume_size           = 50
      volume_type           = "gp3"
      iops                  = 3000
      throughput            = 125
      encrypted             = true
      kms_key_id            = aws_kms_key.eks.arn
      delete_on_termination = true
    }
  }

  metadata_options {
    http_endpoint               = "enabled"
    http_tokens                 = "required"
    http_put_response_hop_limit = 1
    instance_metadata_tags      = "enabled"
  }

  monitoring {
    enabled = true
  }

  tag_specifications {
    resource_type = "instance"
    tags = merge(local.common_tags, {
      Name = "${var.cluster_name}-system-node"
    })
  }

  tag_specifications {
    resource_type = "volume"
    tags = merge(local.common_tags, {
      Name = "${var.cluster_name}-system-node-volume"
    })
  }

  tags = local.common_tags

  lifecycle {
    create_before_destroy = true
  }
}

resource "aws_launch_template" "app_nodes" {
  name_prefix = "${var.cluster_name}-app-nodes-"
  description = "Launch template for ${var.cluster_name} application node group"

  block_device_mappings {
    device_name = "/dev/xvda"
    ebs {
      volume_size           = 100
      volume_type           = "gp3"
      iops                  = 3000
      throughput            = 125
      encrypted             = true
      kms_key_id            = aws_kms_key.eks.arn
      delete_on_termination = true
    }
  }

  metadata_options {
    http_endpoint               = "enabled"
    http_tokens                 = "required"
    http_put_response_hop_limit = 1
    instance_metadata_tags      = "enabled"
  }

  monitoring {
    enabled = true
  }

  tag_specifications {
    resource_type = "instance"
    tags = merge(local.common_tags, {
      Name = "${var.cluster_name}-app-node"
    })
  }

  tag_specifications {
    resource_type = "volume"
    tags = merge(local.common_tags, {
      Name = "${var.cluster_name}-app-node-volume"
    })
  }

  tags = local.common_tags

  lifecycle {
    create_before_destroy = true
  }
}

###############################################################################
# EKS Add-ons
###############################################################################

resource "aws_eks_addon" "vpc_cni" {
  cluster_name             = aws_eks_cluster.main.name
  addon_name               = "vpc-cni"
  resolve_conflicts_on_update = "OVERWRITE"

  tags = local.common_tags

  depends_on = [aws_eks_node_group.system]
}

resource "aws_eks_addon" "coredns" {
  cluster_name             = aws_eks_cluster.main.name
  addon_name               = "coredns"
  resolve_conflicts_on_update = "OVERWRITE"

  tags = local.common_tags

  depends_on = [aws_eks_node_group.system]
}

resource "aws_eks_addon" "kube_proxy" {
  cluster_name             = aws_eks_cluster.main.name
  addon_name               = "kube-proxy"
  resolve_conflicts_on_update = "OVERWRITE"

  tags = local.common_tags

  depends_on = [aws_eks_node_group.system]
}

resource "aws_eks_addon" "ebs_csi_driver" {
  cluster_name             = aws_eks_cluster.main.name
  addon_name               = "aws-ebs-csi-driver"
  resolve_conflicts_on_update = "OVERWRITE"
  service_account_role_arn = var.enable_irsa ? aws_iam_role.ebs_csi[0].arn : null

  tags = local.common_tags

  depends_on = [aws_eks_node_group.app]
}

###############################################################################
# IRSA — OIDC Provider
###############################################################################

data "tls_certificate" "eks_oidc" {
  url = aws_eks_cluster.main.identity[0].oidc[0].issuer
}

resource "aws_iam_openid_connect_provider" "eks" {
  count = var.enable_irsa ? 1 : 0

  client_id_list  = ["sts.amazonaws.com"]
  thumbprint_list = [data.tls_certificate.eks_oidc.certificates[0].sha1_fingerprint]
  url             = aws_eks_cluster.main.identity[0].oidc[0].issuer

  tags = local.common_tags
}

###############################################################################
# IRSA — EBS CSI Driver Role
###############################################################################

data "aws_iam_policy_document" "ebs_csi_assume" {
  count = var.enable_irsa ? 1 : 0

  statement {
    effect  = "Allow"
    actions = ["sts:AssumeRoleWithWebIdentity"]

    principals {
      type        = "Federated"
      identifiers = [aws_iam_openid_connect_provider.eks[0].arn]
    }

    condition {
      test     = "StringEquals"
      variable = "${replace(aws_eks_cluster.main.identity[0].oidc[0].issuer, "https://", "")}:sub"
      values   = ["system:serviceaccount:kube-system:ebs-csi-controller-sa"]
    }

    condition {
      test     = "StringEquals"
      variable = "${replace(aws_eks_cluster.main.identity[0].oidc[0].issuer, "https://", "")}:aud"
      values   = ["sts.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "ebs_csi" {
  count = var.enable_irsa ? 1 : 0

  name               = "${var.cluster_name}-ebs-csi-role"
  assume_role_policy = data.aws_iam_policy_document.ebs_csi_assume[0].json

  tags = local.common_tags
}

resource "aws_iam_role_policy_attachment" "ebs_csi" {
  count = var.enable_irsa ? 1 : 0

  role       = aws_iam_role.ebs_csi[0].name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonEBSCSIDriverPolicy"
}

###############################################################################
# Outputs
###############################################################################

output "cluster_endpoint" {
  description = "Endpoint URL of the EKS API server."
  value       = aws_eks_cluster.main.endpoint
}

output "cluster_name" {
  description = "Name of the EKS cluster."
  value       = aws_eks_cluster.main.name
}

output "cluster_version" {
  description = "Kubernetes version of the EKS cluster."
  value       = aws_eks_cluster.main.version
}

output "cluster_certificate_authority_data" {
  description = "Base64-encoded certificate authority data for the cluster."
  value       = aws_eks_cluster.main.certificate_authority[0].data
  sensitive   = true
}

output "cluster_security_group_id" {
  description = "ID of the EKS cluster security group (managed by EKS)."
  value       = aws_eks_cluster.main.vpc_config[0].cluster_security_group_id
}

output "oidc_provider_arn" {
  description = "ARN of the OIDC provider (for IRSA)."
  value       = var.enable_irsa ? aws_iam_openid_connect_provider.eks[0].arn : null
}

output "oidc_provider_url" {
  description = "URL of the OIDC provider."
  value       = var.enable_irsa ? aws_iam_openid_connect_provider.eks[0].url : null
}

output "node_role_arn" {
  description = "ARN of the IAM role used by EKS worker nodes."
  value       = aws_iam_role.node.arn
}

output "kms_key_arn" {
  description = "ARN of the KMS key used for EKS secret encryption."
  value       = aws_kms_key.eks.arn
}
