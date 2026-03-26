###############################################################################
# LogisticOS — IAM Module
# Provisions AWS IAM roles and least-privilege policies for every LogisticOS
# microservice using IRSA (IAM Roles for Service Accounts).  Each role trusts
# the EKS OIDC provider so that only the designated Kubernetes Service Account
# in the designated namespace can assume it — no node-level credentials.
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
  description = "Name of the EKS cluster.  Used for tagging and resource naming."
  type        = string
}

variable "cluster_oidc_provider_arn" {
  description = "ARN of the IAM OIDC provider associated with the EKS cluster.  Required for IRSA trust policies."
  type        = string
}

variable "aws_account_id" {
  description = "AWS account ID.  Used to construct ARNs in policy documents."
  type        = string
}

variable "aws_region" {
  description = "AWS region where resources are deployed."
  type        = string
  default     = "ap-southeast-1"
}

variable "pod_photos_bucket_name" {
  description = "Name of the S3 bucket used for POD (Proof of Delivery) photos."
  type        = string
  default     = ""
}

variable "model_artifacts_bucket_name" {
  description = "Name of the S3 bucket used for ONNX model artifacts consumed by the AI layer."
  type        = string
  default     = ""
}

variable "analytics_bucket_name" {
  description = "Name of the S3 bucket used for analytics exports and Athena query results."
  type        = string
  default     = ""
}

variable "tags" {
  description = "Additional tags to apply to all IAM resources."
  type        = map(string)
  default     = {}
}

###############################################################################
# Locals
###############################################################################

locals {
  # Strip the scheme prefix from the OIDC issuer URL to form the condition key
  oidc_provider_id = replace(var.cluster_oidc_provider_arn, "/^arn:aws:iam::[0-9]+:oidc-provider\\//", "")

  common_tags = merge(
    {
      Project     = "logisticos"
      Environment = var.environment
      ManagedBy   = "terraform"
      Module      = "iam"
    },
    var.tags
  )

  # Effective bucket names: fall back to deterministic defaults when not provided
  pod_photos_bucket = coalesce(
    var.pod_photos_bucket_name,
    "logisticos-pod-photos-${var.environment}"
  )
  model_artifacts_bucket = coalesce(
    var.model_artifacts_bucket_name,
    "logisticos-model-artifacts-${var.environment}"
  )
  analytics_bucket = coalesce(
    var.analytics_bucket_name,
    "logisticos-exports-${var.environment}"
  )

  # POD KMS key alias — referenced by both pod and payments policies
  pod_kms_alias = "alias/logisticos-pod-${var.environment}"

  # Payments KMS key alias (PCI scope)
  payments_kms_alias = "alias/logisticos-payments-${var.environment}"

  # Identity KMS key alias (JWT signing key encryption)
  identity_kms_alias = "alias/logisticos-identity-${var.environment}"
}

###############################################################################
# Helper: reusable IRSA trust policy document
# Each service overrides sub (SA name) and namespace.
###############################################################################

# ── API Gateway ──────────────────────────────────────────────────────────────

data "aws_iam_policy_document" "api_gateway_trust" {
  statement {
    sid     = "EKSIRSATrust"
    effect  = "Allow"
    actions = ["sts:AssumeRoleWithWebIdentity"]

    principals {
      type        = "Federated"
      identifiers = [var.cluster_oidc_provider_arn]
    }

    condition {
      test     = "StringEquals"
      variable = "${local.oidc_provider_id}:sub"
      values   = ["system:serviceaccount:logisticos-core:logisticos-api-gateway"]
    }

    condition {
      test     = "StringEquals"
      variable = "${local.oidc_provider_id}:aud"
      values   = ["sts.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "api_gateway" {
  name               = "logisticos-api-gateway-${var.environment}"
  assume_role_policy = data.aws_iam_policy_document.api_gateway_trust.json
  description        = "IRSA role for the LogisticOS API Gateway service (logisticos-core namespace)."

  tags = merge(local.common_tags, { Service = "api-gateway" })
}

data "aws_iam_policy_document" "api_gateway_permissions" {
  # SSM Parameter Store — service discovery config (read-only, scoped to prefix)
  statement {
    sid    = "SSMParameterStoreRead"
    effect = "Allow"
    actions = [
      "ssm:GetParameter",
      "ssm:GetParameters",
      "ssm:GetParametersByPath",
    ]
    resources = [
      "arn:aws:ssm:${var.aws_region}:${var.aws_account_id}:parameter/logisticos/${var.environment}/api-gateway/*",
      "arn:aws:ssm:${var.aws_region}:${var.aws_account_id}:parameter/logisticos/${var.environment}/common/*",
    ]
  }

  # CloudWatch Logs — structured log delivery
  statement {
    sid    = "CloudWatchLogsWrite"
    effect = "Allow"
    actions = [
      "logs:CreateLogGroup",
      "logs:CreateLogStream",
      "logs:PutLogEvents",
      "logs:DescribeLogStreams",
    ]
    resources = [
      "arn:aws:logs:${var.aws_region}:${var.aws_account_id}:log-group:/logisticos/${var.environment}/api-gateway:*",
    ]
  }

  # WAF — allow reading WebACL associations for self-inspection
  statement {
    sid    = "WAFReadOnly"
    effect = "Allow"
    actions = [
      "wafv2:GetWebACL",
      "wafv2:GetWebACLForResource",
    ]
    resources = ["*"]
  }

  # Explicit deny: prevent any attempt to access data-tier resources
  statement {
    sid    = "DenyDataTierAccess"
    effect = "Deny"
    actions = [
      "s3:*",
      "rds:*",
      "dynamodb:*",
    ]
    resources = ["*"]
  }
}

resource "aws_iam_policy" "api_gateway" {
  name        = "logisticos-api-gateway-${var.environment}"
  description = "Least-privilege policy for the LogisticOS API Gateway IRSA role."
  policy      = data.aws_iam_policy_document.api_gateway_permissions.json

  tags = merge(local.common_tags, { Service = "api-gateway" })
}

resource "aws_iam_role_policy_attachment" "api_gateway" {
  role       = aws_iam_role.api_gateway.name
  policy_arn = aws_iam_policy.api_gateway.arn
}

# ── Identity Service ──────────────────────────────────────────────────────────

data "aws_iam_policy_document" "identity_trust" {
  statement {
    sid     = "EKSIRSATrust"
    effect  = "Allow"
    actions = ["sts:AssumeRoleWithWebIdentity"]

    principals {
      type        = "Federated"
      identifiers = [var.cluster_oidc_provider_arn]
    }

    condition {
      test     = "StringEquals"
      variable = "${local.oidc_provider_id}:sub"
      values   = ["system:serviceaccount:logisticos-core:logisticos-identity"]
    }

    condition {
      test     = "StringEquals"
      variable = "${local.oidc_provider_id}:aud"
      values   = ["sts.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "identity" {
  name               = "logisticos-identity-${var.environment}"
  assume_role_policy = data.aws_iam_policy_document.identity_trust.json
  description        = "IRSA role for the LogisticOS Identity & Tenant Management service."

  tags = merge(local.common_tags, { Service = "identity" })
}

data "aws_iam_policy_document" "identity_permissions" {
  # Secrets Manager — JWT signing keys, OIDC client secrets
  statement {
    sid    = "SecretsManagerRead"
    effect = "Allow"
    actions = [
      "secretsmanager:GetSecretValue",
      "secretsmanager:DescribeSecret",
    ]
    resources = [
      "arn:aws:secretsmanager:${var.aws_region}:${var.aws_account_id}:secret:logisticos/${var.environment}/identity/*",
    ]
  }

  # KMS — decrypt JWT signing key material stored in Secrets Manager
  statement {
    sid    = "KMSDecrypt"
    effect = "Allow"
    actions = [
      "kms:Decrypt",
      "kms:DescribeKey",
      "kms:GenerateDataKey",
    ]
    resources = [
      "arn:aws:kms:${var.aws_region}:${var.aws_account_id}:${local.identity_kms_alias}",
    ]
  }

  # CloudWatch Logs
  statement {
    sid    = "CloudWatchLogsWrite"
    effect = "Allow"
    actions = [
      "logs:CreateLogGroup",
      "logs:CreateLogStream",
      "logs:PutLogEvents",
      "logs:DescribeLogStreams",
    ]
    resources = [
      "arn:aws:logs:${var.aws_region}:${var.aws_account_id}:log-group:/logisticos/${var.environment}/identity:*",
    ]
  }

  # Deny access to anything outside identity scope
  statement {
    sid    = "DenyNonIdentityResources"
    effect = "Deny"
    actions = [
      "s3:*",
      "ec2:*",
    ]
    resources = ["*"]
  }
}

resource "aws_iam_policy" "identity" {
  name        = "logisticos-identity-${var.environment}"
  description = "Least-privilege policy for the LogisticOS Identity service IRSA role."
  policy      = data.aws_iam_policy_document.identity_permissions.json

  tags = merge(local.common_tags, { Service = "identity" })
}

resource "aws_iam_role_policy_attachment" "identity" {
  role       = aws_iam_role.identity.name
  policy_arn = aws_iam_policy.identity.arn
}

# ── Dispatch Service ──────────────────────────────────────────────────────────

data "aws_iam_policy_document" "dispatch_trust" {
  statement {
    sid     = "EKSIRSATrust"
    effect  = "Allow"
    actions = ["sts:AssumeRoleWithWebIdentity"]

    principals {
      type        = "Federated"
      identifiers = [var.cluster_oidc_provider_arn]
    }

    condition {
      test     = "StringEquals"
      variable = "${local.oidc_provider_id}:sub"
      values   = ["system:serviceaccount:logisticos-logistics:logisticos-dispatch"]
    }

    condition {
      test     = "StringEquals"
      variable = "${local.oidc_provider_id}:aud"
      values   = ["sts.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "dispatch" {
  name               = "logisticos-dispatch-${var.environment}"
  assume_role_policy = data.aws_iam_policy_document.dispatch_trust.json
  description        = "IRSA role for the LogisticOS Dispatch & Routing service (logisticos-logistics namespace)."

  tags = merge(local.common_tags, { Service = "dispatch" })
}

data "aws_iam_policy_document" "dispatch_permissions" {
  # SQS — dispatch job queue (VRP optimization tasks, driver assignment events)
  statement {
    sid    = "SQSJobQueueRead"
    effect = "Allow"
    actions = [
      "sqs:ReceiveMessage",
      "sqs:DeleteMessage",
      "sqs:GetQueueAttributes",
      "sqs:GetQueueUrl",
      "sqs:ChangeMessageVisibility",
    ]
    resources = [
      "arn:aws:sqs:${var.aws_region}:${var.aws_account_id}:logisticos-dispatch-${var.environment}",
      "arn:aws:sqs:${var.aws_region}:${var.aws_account_id}:logisticos-dispatch-dlq-${var.environment}",
    ]
  }

  # SQS — allow sending to the routing service queue (async route optimization requests)
  statement {
    sid    = "SQSRoutingQueueSend"
    effect = "Allow"
    actions = [
      "sqs:SendMessage",
      "sqs:GetQueueUrl",
    ]
    resources = [
      "arn:aws:sqs:${var.aws_region}:${var.aws_account_id}:logisticos-routing-${var.environment}",
    ]
  }

  # CloudWatch Logs
  statement {
    sid    = "CloudWatchLogsWrite"
    effect = "Allow"
    actions = [
      "logs:CreateLogGroup",
      "logs:CreateLogStream",
      "logs:PutLogEvents",
      "logs:DescribeLogStreams",
    ]
    resources = [
      "arn:aws:logs:${var.aws_region}:${var.aws_account_id}:log-group:/logisticos/${var.environment}/dispatch:*",
    ]
  }

  # CloudWatch Metrics — custom dispatch latency / SLA metrics
  statement {
    sid    = "CloudWatchMetricsPut"
    effect = "Allow"
    actions = [
      "cloudwatch:PutMetricData",
    ]
    resources = ["*"]
    condition {
      test     = "StringEquals"
      variable = "cloudwatch:namespace"
      values   = ["LogisticOS/Dispatch"]
    }
  }
}

resource "aws_iam_policy" "dispatch" {
  name        = "logisticos-dispatch-${var.environment}"
  description = "Least-privilege policy for the LogisticOS Dispatch service IRSA role."
  policy      = data.aws_iam_policy_document.dispatch_permissions.json

  tags = merge(local.common_tags, { Service = "dispatch" })
}

resource "aws_iam_role_policy_attachment" "dispatch" {
  role       = aws_iam_role.dispatch.name
  policy_arn = aws_iam_policy.dispatch.arn
}

# ── Payments Service (PCI-scoped) ─────────────────────────────────────────────

data "aws_iam_policy_document" "payments_trust" {
  statement {
    sid     = "EKSIRSATrust"
    effect  = "Allow"
    actions = ["sts:AssumeRoleWithWebIdentity"]

    principals {
      type        = "Federated"
      identifiers = [var.cluster_oidc_provider_arn]
    }

    condition {
      test     = "StringEquals"
      variable = "${local.oidc_provider_id}:sub"
      values   = ["system:serviceaccount:logisticos-data:logisticos-payments"]
    }

    condition {
      test     = "StringEquals"
      variable = "${local.oidc_provider_id}:aud"
      values   = ["sts.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "payments" {
  name               = "logisticos-payments-${var.environment}"
  assume_role_policy = data.aws_iam_policy_document.payments_trust.json
  description        = "IRSA role for the LogisticOS Payments & Billing service (PCI-scoped, logisticos-data namespace)."

  # Permissions boundary prevents privilege escalation even if policy is misconfigured
  permissions_boundary = "arn:aws:iam::${var.aws_account_id}:policy/logisticos-pci-boundary-${var.environment}"

  tags = merge(local.common_tags, { Service = "payments", PCIScope = "true" })
}

data "aws_iam_policy_document" "payments_permissions" {
  # Secrets Manager — payment gateway API keys (Stripe, PayMongo), COD reconciliation creds
  statement {
    sid    = "SecretsManagerPaymentKeys"
    effect = "Allow"
    actions = [
      "secretsmanager:GetSecretValue",
      "secretsmanager:DescribeSecret",
    ]
    resources = [
      "arn:aws:secretsmanager:${var.aws_region}:${var.aws_account_id}:secret:logisticos/${var.environment}/payments/*",
    ]
  }

  # KMS — PCI-scoped key for encrypting card-related data at rest
  statement {
    sid    = "KMSPaymentsEncryptDecrypt"
    effect = "Allow"
    actions = [
      "kms:Encrypt",
      "kms:Decrypt",
      "kms:ReEncrypt*",
      "kms:GenerateDataKey",
      "kms:DescribeKey",
    ]
    resources = [
      "arn:aws:kms:${var.aws_region}:${var.aws_account_id}:${local.payments_kms_alias}",
    ]
  }

  # CloudWatch Logs — PCI audit trail requirement
  statement {
    sid    = "CloudWatchLogsWrite"
    effect = "Allow"
    actions = [
      "logs:CreateLogGroup",
      "logs:CreateLogStream",
      "logs:PutLogEvents",
      "logs:DescribeLogStreams",
    ]
    resources = [
      "arn:aws:logs:${var.aws_region}:${var.aws_account_id}:log-group:/logisticos/${var.environment}/payments:*",
    ]
  }

  # CloudWatch Metrics — billing anomaly detection metrics
  statement {
    sid    = "CloudWatchMetricsPut"
    effect = "Allow"
    actions = [
      "cloudwatch:PutMetricData",
    ]
    resources = ["*"]
    condition {
      test     = "StringEquals"
      variable = "cloudwatch:namespace"
      values   = ["LogisticOS/Payments"]
    }
  }

  # PCI explicit deny: payments service must never touch general S3 or EC2
  statement {
    sid    = "DenyS3Access"
    effect = "Deny"
    actions = [
      "s3:*",
    ]
    resources = ["*"]
  }

  statement {
    sid    = "DenyEC2Access"
    effect = "Deny"
    actions = [
      "ec2:*",
    ]
    resources = ["*"]
  }

  # PCI explicit deny: no IAM self-modification
  statement {
    sid    = "DenyIAMModification"
    effect = "Deny"
    actions = [
      "iam:CreatePolicy",
      "iam:AttachRolePolicy",
      "iam:PutRolePolicy",
      "iam:CreateRole",
    ]
    resources = ["*"]
  }
}

resource "aws_iam_policy" "payments" {
  name        = "logisticos-payments-${var.environment}"
  description = "PCI-scoped least-privilege policy for the LogisticOS Payments service IRSA role."
  policy      = data.aws_iam_policy_document.payments_permissions.json

  tags = merge(local.common_tags, { Service = "payments", PCIScope = "true" })
}

resource "aws_iam_role_policy_attachment" "payments" {
  role       = aws_iam_role.payments.name
  policy_arn = aws_iam_policy.payments.arn
}

# ── Analytics Service ─────────────────────────────────────────────────────────

data "aws_iam_policy_document" "analytics_trust" {
  statement {
    sid     = "EKSIRSATrust"
    effect  = "Allow"
    actions = ["sts:AssumeRoleWithWebIdentity"]

    principals {
      type        = "Federated"
      identifiers = [var.cluster_oidc_provider_arn]
    }

    condition {
      test     = "StringEquals"
      variable = "${local.oidc_provider_id}:sub"
      values   = ["system:serviceaccount:logisticos-data:logisticos-analytics"]
    }

    condition {
      test     = "StringEquals"
      variable = "${local.oidc_provider_id}:aud"
      values   = ["sts.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "analytics" {
  name               = "logisticos-analytics-${var.environment}"
  assume_role_policy = data.aws_iam_policy_document.analytics_trust.json
  description        = "IRSA role for the LogisticOS Analytics & BI service (logisticos-data namespace)."

  tags = merge(local.common_tags, { Service = "analytics" })
}

data "aws_iam_policy_document" "analytics_permissions" {
  # Athena — execute queries against the analytics data lake
  statement {
    sid    = "AthenaQueryExecution"
    effect = "Allow"
    actions = [
      "athena:StartQueryExecution",
      "athena:StopQueryExecution",
      "athena:GetQueryExecution",
      "athena:GetQueryResults",
      "athena:GetWorkGroup",
      "athena:ListWorkGroups",
      "athena:GetDataCatalog",
    ]
    resources = [
      "arn:aws:athena:${var.aws_region}:${var.aws_account_id}:workgroup/logisticos-${var.environment}",
      "arn:aws:athena:${var.aws_region}:${var.aws_account_id}:datacatalog/*",
    ]
  }

  # Glue — Athena uses Glue Data Catalog for table metadata
  statement {
    sid    = "GlueDataCatalogRead"
    effect = "Allow"
    actions = [
      "glue:GetDatabase",
      "glue:GetDatabases",
      "glue:GetTable",
      "glue:GetTables",
      "glue:GetPartition",
      "glue:GetPartitions",
    ]
    resources = [
      "arn:aws:glue:${var.aws_region}:${var.aws_account_id}:catalog",
      "arn:aws:glue:${var.aws_region}:${var.aws_account_id}:database/logisticos_${var.environment}*",
      "arn:aws:glue:${var.aws_region}:${var.aws_account_id}:table/logisticos_${var.environment}*/*",
    ]
  }

  # S3 — read analytics/export bucket; write Athena query results
  statement {
    sid    = "S3AnalyticsBucketReadWrite"
    effect = "Allow"
    actions = [
      "s3:GetObject",
      "s3:PutObject",
      "s3:DeleteObject",
      "s3:ListBucket",
      "s3:GetBucketLocation",
    ]
    resources = [
      "arn:aws:s3:::${local.analytics_bucket}",
      "arn:aws:s3:::${local.analytics_bucket}/*",
    ]
  }

  # CloudWatch Logs
  statement {
    sid    = "CloudWatchLogsWrite"
    effect = "Allow"
    actions = [
      "logs:CreateLogGroup",
      "logs:CreateLogStream",
      "logs:PutLogEvents",
      "logs:DescribeLogStreams",
    ]
    resources = [
      "arn:aws:logs:${var.aws_region}:${var.aws_account_id}:log-group:/logisticos/${var.environment}/analytics:*",
    ]
  }
}

resource "aws_iam_policy" "analytics" {
  name        = "logisticos-analytics-${var.environment}"
  description = "Least-privilege policy for the LogisticOS Analytics service IRSA role."
  policy      = data.aws_iam_policy_document.analytics_permissions.json

  tags = merge(local.common_tags, { Service = "analytics" })
}

resource "aws_iam_role_policy_attachment" "analytics" {
  role       = aws_iam_role.analytics.name
  policy_arn = aws_iam_policy.analytics.arn
}

# ── AI Layer ──────────────────────────────────────────────────────────────────

data "aws_iam_policy_document" "ai_layer_trust" {
  statement {
    sid     = "EKSIRSATrust"
    effect  = "Allow"
    actions = ["sts:AssumeRoleWithWebIdentity"]

    principals {
      type        = "Federated"
      identifiers = [var.cluster_oidc_provider_arn]
    }

    condition {
      test     = "StringEquals"
      variable = "${local.oidc_provider_id}:sub"
      values   = ["system:serviceaccount:logisticos-ai:logisticos-ai-layer"]
    }

    condition {
      test     = "StringEquals"
      variable = "${local.oidc_provider_id}:aud"
      values   = ["sts.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "ai_layer" {
  name               = "logisticos-ai-layer-${var.environment}"
  assume_role_policy = data.aws_iam_policy_document.ai_layer_trust.json
  description        = "IRSA role for the LogisticOS AI Intelligence Layer (logisticos-ai namespace)."

  tags = merge(local.common_tags, { Service = "ai-layer" })
}

data "aws_iam_policy_document" "ai_layer_permissions" {
  # Secrets Manager — Anthropic Claude API key, OpenAI API key, embedding service keys
  statement {
    sid    = "SecretsManagerAIKeys"
    effect = "Allow"
    actions = [
      "secretsmanager:GetSecretValue",
      "secretsmanager:DescribeSecret",
    ]
    resources = [
      "arn:aws:secretsmanager:${var.aws_region}:${var.aws_account_id}:secret:logisticos/${var.environment}/ai-layer/*",
    ]
  }

  # S3 — read ONNX model artifacts (model serving within Rust services via ONNX Runtime)
  statement {
    sid    = "S3ModelArtifactsRead"
    effect = "Allow"
    actions = [
      "s3:GetObject",
      "s3:ListBucket",
      "s3:GetBucketLocation",
      "s3:GetObjectVersion",
    ]
    resources = [
      "arn:aws:s3:::${local.model_artifacts_bucket}",
      "arn:aws:s3:::${local.model_artifacts_bucket}/*",
    ]
  }

  # CloudWatch Logs — model inference logs, agent traces
  statement {
    sid    = "CloudWatchLogsWrite"
    effect = "Allow"
    actions = [
      "logs:CreateLogGroup",
      "logs:CreateLogStream",
      "logs:PutLogEvents",
      "logs:DescribeLogStreams",
    ]
    resources = [
      "arn:aws:logs:${var.aws_region}:${var.aws_account_id}:log-group:/logisticos/${var.environment}/ai-layer:*",
    ]
  }

  # CloudWatch Metrics — model latency, token usage, agent action counters
  statement {
    sid    = "CloudWatchMetricsPut"
    effect = "Allow"
    actions = [
      "cloudwatch:PutMetricData",
    ]
    resources = ["*"]
    condition {
      test     = "StringEquals"
      variable = "cloudwatch:namespace"
      values   = ["LogisticOS/AILayer"]
    }
  }

  # Deny write to model artifacts from runtime (prevents model tampering)
  statement {
    sid    = "DenyModelArtifactWrite"
    effect = "Deny"
    actions = [
      "s3:PutObject",
      "s3:DeleteObject",
    ]
    resources = [
      "arn:aws:s3:::${local.model_artifacts_bucket}/*",
    ]
  }
}

resource "aws_iam_policy" "ai_layer" {
  name        = "logisticos-ai-layer-${var.environment}"
  description = "Least-privilege policy for the LogisticOS AI Layer IRSA role."
  policy      = data.aws_iam_policy_document.ai_layer_permissions.json

  tags = merge(local.common_tags, { Service = "ai-layer" })
}

resource "aws_iam_role_policy_attachment" "ai_layer" {
  role       = aws_iam_role.ai_layer.name
  policy_arn = aws_iam_policy.ai_layer.arn
}

# ── POD Service (Proof of Delivery) ───────────────────────────────────────────

data "aws_iam_policy_document" "pod_trust" {
  statement {
    sid     = "EKSIRSATrust"
    effect  = "Allow"
    actions = ["sts:AssumeRoleWithWebIdentity"]

    principals {
      type        = "Federated"
      identifiers = [var.cluster_oidc_provider_arn]
    }

    condition {
      test     = "StringEquals"
      variable = "${local.oidc_provider_id}:sub"
      values   = ["system:serviceaccount:logisticos-logistics:logisticos-pod"]
    }

    condition {
      test     = "StringEquals"
      variable = "${local.oidc_provider_id}:aud"
      values   = ["sts.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "pod" {
  name               = "logisticos-pod-${var.environment}"
  assume_role_policy = data.aws_iam_policy_document.pod_trust.json
  description        = "IRSA role for the LogisticOS Proof of Delivery service (logisticos-logistics namespace)."

  tags = merge(local.common_tags, { Service = "pod" })
}

data "aws_iam_policy_document" "pod_permissions" {
  # S3 — POD photos (driver captures signature/photo at delivery point)
  statement {
    sid    = "S3PODPhotosReadWrite"
    effect = "Allow"
    actions = [
      "s3:PutObject",
      "s3:GetObject",
      "s3:DeleteObject",
      "s3:ListBucket",
      "s3:GetBucketLocation",
    ]
    resources = [
      "arn:aws:s3:::${local.pod_photos_bucket}",
      "arn:aws:s3:::${local.pod_photos_bucket}/*",
    ]
  }

  # KMS — encrypt/decrypt POD photos at rest (customer biometric data protection)
  statement {
    sid    = "KMSPODEncryptDecrypt"
    effect = "Allow"
    actions = [
      "kms:Encrypt",
      "kms:Decrypt",
      "kms:ReEncrypt*",
      "kms:GenerateDataKey",
      "kms:DescribeKey",
    ]
    resources = [
      "arn:aws:kms:${var.aws_region}:${var.aws_account_id}:${local.pod_kms_alias}",
    ]
  }

  # CloudWatch Logs
  statement {
    sid    = "CloudWatchLogsWrite"
    effect = "Allow"
    actions = [
      "logs:CreateLogGroup",
      "logs:CreateLogStream",
      "logs:PutLogEvents",
      "logs:DescribeLogStreams",
    ]
    resources = [
      "arn:aws:logs:${var.aws_region}:${var.aws_account_id}:log-group:/logisticos/${var.environment}/pod:*",
    ]
  }

  # Deny any S3 access outside the POD bucket
  statement {
    sid    = "DenyNonPODS3"
    effect = "Deny"
    actions = [
      "s3:*",
    ]
    not_resources = [
      "arn:aws:s3:::${local.pod_photos_bucket}",
      "arn:aws:s3:::${local.pod_photos_bucket}/*",
    ]
  }
}

resource "aws_iam_policy" "pod" {
  name        = "logisticos-pod-${var.environment}"
  description = "Least-privilege policy for the LogisticOS POD service IRSA role (S3 + KMS for delivery photos)."
  policy      = data.aws_iam_policy_document.pod_permissions.json

  tags = merge(local.common_tags, { Service = "pod" })
}

resource "aws_iam_role_policy_attachment" "pod" {
  role       = aws_iam_role.pod.name
  policy_arn = aws_iam_policy.pod.arn
}

###############################################################################
# Outputs
###############################################################################

output "service_role_arns" {
  description = "Map of service name to IAM role ARN for IRSA annotation in Kubernetes manifests."
  value = {
    api_gateway = aws_iam_role.api_gateway.arn
    identity    = aws_iam_role.identity.arn
    dispatch    = aws_iam_role.dispatch.arn
    payments    = aws_iam_role.payments.arn
    analytics   = aws_iam_role.analytics.arn
    ai_layer    = aws_iam_role.ai_layer.arn
    pod         = aws_iam_role.pod.arn
  }
}

output "service_role_names" {
  description = "Map of service name to IAM role name (useful for policy attachment in other modules)."
  value = {
    api_gateway = aws_iam_role.api_gateway.name
    identity    = aws_iam_role.identity.name
    dispatch    = aws_iam_role.dispatch.name
    payments    = aws_iam_role.payments.name
    analytics   = aws_iam_role.analytics.name
    ai_layer    = aws_iam_role.ai_layer.name
    pod         = aws_iam_role.pod.name
  }
}

output "service_policy_arns" {
  description = "Map of service name to IAM managed policy ARN."
  value = {
    api_gateway = aws_iam_policy.api_gateway.arn
    identity    = aws_iam_policy.identity.arn
    dispatch    = aws_iam_policy.dispatch.arn
    payments    = aws_iam_policy.payments.arn
    analytics   = aws_iam_policy.analytics.arn
    ai_layer    = aws_iam_policy.ai_layer.arn
    pod         = aws_iam_policy.pod.arn
  }
}
