###############################################################################
# LogisticOS — S3 Module
# Provisions all AWS S3 buckets required by the platform with enforced
# encryption, public access blocking, versioning, lifecycle policies, and
# (where applicable) CORS and bucket policies.
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

variable "aws_account_id" {
  description = "AWS account ID.  Used in bucket policies to scope principals."
  type        = string
}

variable "aws_region" {
  description = "AWS region where the buckets are created."
  type        = string
  default     = "ap-southeast-1"
}

variable "enable_versioning" {
  description = "Master toggle for bucket versioning.  Individual buckets override this where semantics require it (model-artifacts and terraform-state are always versioned)."
  type        = bool
  default     = false
}

variable "lifecycle_expire_days" {
  description = "Number of days after which POD photo objects expire and are deleted.  Defaults to 365 (1 year) to satisfy audit requirements."
  type        = number
  default     = 365

  validation {
    condition     = var.lifecycle_expire_days >= 90
    error_message = "lifecycle_expire_days must be at least 90 days to meet minimum audit retention requirements."
  }
}

variable "pod_photos_cors_origins" {
  description = "List of allowed origins for POD photo bucket CORS (driver app and merchant portal domains)."
  type        = list(string)
  default     = ["https://*.logisticos.io", "https://*.logisticos.app"]
}

variable "force_destroy" {
  description = "Allow Terraform to destroy non-empty buckets.  Must be false in production."
  type        = bool
  default     = false
}

variable "tags" {
  description = "Additional tags applied to all S3 resources."
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
      Module      = "s3"
    },
    var.tags
  )

  # Bucket names — centralised so other modules can reference them via outputs
  bucket_names = {
    pod_photos        = "logisticos-pod-photos-${var.environment}"
    exports           = "logisticos-exports-${var.environment}"
    model_artifacts   = "logisticos-model-artifacts-${var.environment}"
    backups           = "logisticos-backups-${var.environment}"
    terraform_state   = "logisticos-terraform-state"
  }

  # Terraform state bucket only exists in production
  create_terraform_state = var.environment == "production"
}

###############################################################################
# POD Photos Bucket
# Stores driver-captured delivery evidence: photos, signatures, OTP records.
# Sensitive data — KMS-encrypted, no public access, lifecycle to INTELLIGENT_TIERING.
###############################################################################

resource "aws_s3_bucket" "pod_photos" {
  bucket        = local.bucket_names.pod_photos
  force_destroy = var.force_destroy

  tags = merge(local.common_tags, {
    Purpose      = "proof-of-delivery-photos"
    DataClass    = "sensitive"
    GDPRScope    = "true"
  })
}

resource "aws_s3_bucket_versioning" "pod_photos" {
  bucket = aws_s3_bucket.pod_photos.id

  versioning_configuration {
    status = var.enable_versioning ? "Enabled" : "Suspended"
  }
}

resource "aws_s3_bucket_server_side_encryption_configuration" "pod_photos" {
  bucket = aws_s3_bucket.pod_photos.id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm     = "aws:kms"
      kms_master_key_id = "alias/logisticos-pod-${var.environment}"
    }
    bucket_key_enabled = true
  }
}

resource "aws_s3_bucket_public_access_block" "pod_photos" {
  bucket = aws_s3_bucket.pod_photos.id

  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_bucket_lifecycle_configuration" "pod_photos" {
  bucket = aws_s3_bucket.pod_photos.id

  rule {
    id     = "transition-to-intelligent-tiering"
    status = "Enabled"

    transition {
      days          = 30
      storage_class = "INTELLIGENT_TIERING"
    }

    expiration {
      days = var.lifecycle_expire_days
    }

    # Clean up incomplete multipart uploads (driver app uploads large photo batches)
    abort_incomplete_multipart_upload {
      days_after_initiation = 7
    }
  }

  rule {
    id     = "expire-delete-markers"
    status = "Enabled"

    expiration {
      expired_object_delete_marker = true
    }
  }
}

resource "aws_s3_bucket_cors_configuration" "pod_photos" {
  bucket = aws_s3_bucket.pod_photos.id

  cors_rule {
    allowed_headers = ["Content-Type", "Content-Length", "Authorization", "x-amz-date", "x-amz-content-sha256"]
    allowed_methods = ["GET", "PUT"]
    allowed_origins = var.pod_photos_cors_origins
    expose_headers  = ["ETag", "x-amz-request-id"]
    max_age_seconds = 3600
  }

  # Read-only access for the merchant portal (view POD evidence)
  cors_rule {
    allowed_headers = ["*"]
    allowed_methods = ["GET"]
    allowed_origins = ["https://merchant.logisticos.io", "https://admin.logisticos.io"]
    expose_headers  = ["ETag"]
    max_age_seconds = 86400
  }
}

resource "aws_s3_bucket_intelligent_tiering_configuration" "pod_photos" {
  bucket = aws_s3_bucket.pod_photos.id
  name   = "pod-photos-tiering"
  status = "Enabled"

  tiering {
    access_tier = "ARCHIVE_ACCESS"
    days        = 90
  }

  tiering {
    access_tier = "DEEP_ARCHIVE_ACCESS"
    days        = 180
  }
}

###############################################################################
# Exports Bucket
# CSV/Excel report exports generated by the Analytics service and downloaded
# by merchants via the portal.  Short-lived objects; no versioning needed.
###############################################################################

resource "aws_s3_bucket" "exports" {
  bucket        = local.bucket_names.exports
  force_destroy = var.force_destroy

  tags = merge(local.common_tags, {
    Purpose   = "report-exports"
    DataClass = "internal"
  })
}

resource "aws_s3_bucket_versioning" "exports" {
  bucket = aws_s3_bucket.exports.id

  versioning_configuration {
    status = "Suspended"
  }
}

resource "aws_s3_bucket_server_side_encryption_configuration" "exports" {
  bucket = aws_s3_bucket.exports.id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
    bucket_key_enabled = true
  }
}

resource "aws_s3_bucket_public_access_block" "exports" {
  bucket = aws_s3_bucket.exports.id

  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_bucket_lifecycle_configuration" "exports" {
  bucket = aws_s3_bucket.exports.id

  rule {
    id     = "expire-old-exports"
    status = "Enabled"

    expiration {
      # Export files are ephemeral; merchant downloads within 7 days or regenerates
      days = 7
    }

    abort_incomplete_multipart_upload {
      days_after_initiation = 1
    }
  }
}

###############################################################################
# Model Artifacts Bucket
# Stores ONNX model files consumed by the AI layer for dispatch prediction,
# routing optimization, and churn scoring.  Versioned so rollbacks are safe.
###############################################################################

resource "aws_s3_bucket" "model_artifacts" {
  bucket        = local.bucket_names.model_artifacts
  force_destroy = var.force_destroy

  tags = merge(local.common_tags, {
    Purpose   = "ml-model-artifacts"
    DataClass = "internal"
    AIScope   = "true"
  })
}

resource "aws_s3_bucket_versioning" "model_artifacts" {
  bucket = aws_s3_bucket.model_artifacts.id

  # Always enabled — model rollback is a first-class operation
  versioning_configuration {
    status = "Enabled"
  }
}

resource "aws_s3_bucket_server_side_encryption_configuration" "model_artifacts" {
  bucket = aws_s3_bucket.model_artifacts.id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
    bucket_key_enabled = true
  }
}

resource "aws_s3_bucket_public_access_block" "model_artifacts" {
  bucket = aws_s3_bucket.model_artifacts.id

  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_bucket_lifecycle_configuration" "model_artifacts" {
  bucket = aws_s3_bucket.model_artifacts.id

  rule {
    id     = "expire-old-model-versions"
    status = "Enabled"

    noncurrent_version_expiration {
      # Keep last 10 non-current versions before expiring older ones
      noncurrent_days           = 90
      newer_noncurrent_versions = 10
    }

    abort_incomplete_multipart_upload {
      days_after_initiation = 3
    }
  }
}

###############################################################################
# Backups Bucket
# Database export snapshots (pg_dump, ClickHouse backups).  Long retention,
# no public access, AES256 encryption, GLACIER transition for cost efficiency.
###############################################################################

resource "aws_s3_bucket" "backups" {
  bucket        = local.bucket_names.backups
  force_destroy = var.force_destroy

  tags = merge(local.common_tags, {
    Purpose   = "database-backups"
    DataClass = "critical"
    GDPRScope = "true"
  })
}

resource "aws_s3_bucket_versioning" "backups" {
  bucket = aws_s3_bucket.backups.id

  versioning_configuration {
    status = "Suspended"
  }
}

resource "aws_s3_bucket_server_side_encryption_configuration" "backups" {
  bucket = aws_s3_bucket.backups.id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
    bucket_key_enabled = true
  }
}

resource "aws_s3_bucket_public_access_block" "backups" {
  bucket = aws_s3_bucket.backups.id

  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_bucket_lifecycle_configuration" "backups" {
  bucket = aws_s3_bucket.backups.id

  rule {
    id     = "backup-tiering-and-expiry"
    status = "Enabled"

    transition {
      days          = 30
      storage_class = "STANDARD_IA"
    }

    transition {
      days          = 60
      storage_class = "GLACIER"
    }

    expiration {
      # Retain backups for 2 years (GDPR data retention policy)
      days = 730
    }

    abort_incomplete_multipart_upload {
      days_after_initiation = 3
    }
  }
}

###############################################################################
# Terraform State Bucket (production only)
# Remote backend for all LogisticOS Terraform workspaces.  SSL-only policy,
# versioning always enabled, DynamoDB locking managed externally.
###############################################################################

resource "aws_s3_bucket" "terraform_state" {
  count = local.create_terraform_state ? 1 : 0

  bucket        = local.bucket_names.terraform_state
  force_destroy = false # Never allow accidental deletion of state

  tags = merge(local.common_tags, {
    Purpose   = "terraform-remote-state"
    DataClass = "critical"
  })
}

resource "aws_s3_bucket_versioning" "terraform_state" {
  count = local.create_terraform_state ? 1 : 0

  bucket = aws_s3_bucket.terraform_state[0].id

  # Always enabled — state history is essential for rollback and audit
  versioning_configuration {
    status = "Enabled"
  }
}

resource "aws_s3_bucket_server_side_encryption_configuration" "terraform_state" {
  count = local.create_terraform_state ? 1 : 0

  bucket = aws_s3_bucket.terraform_state[0].id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
    bucket_key_enabled = true
  }
}

resource "aws_s3_bucket_public_access_block" "terraform_state" {
  count = local.create_terraform_state ? 1 : 0

  bucket = aws_s3_bucket.terraform_state[0].id

  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

data "aws_iam_policy_document" "terraform_state_policy" {
  count = local.create_terraform_state ? 1 : 0

  # Enforce TLS for all requests — deny any HTTP access
  statement {
    sid     = "DenyNonSSL"
    effect  = "Deny"
    actions = ["s3:*"]
    resources = [
      "arn:aws:s3:::${local.bucket_names.terraform_state}",
      "arn:aws:s3:::${local.bucket_names.terraform_state}/*",
    ]

    principals {
      type        = "*"
      identifiers = ["*"]
    }

    condition {
      test     = "Bool"
      variable = "aws:SecureTransport"
      values   = ["false"]
    }
  }

  # Deny deletion of state objects (prevents accidental state loss)
  statement {
    sid    = "DenyStateObjectDeletion"
    effect = "Deny"
    actions = [
      "s3:DeleteObject",
      "s3:DeleteObjectVersion",
    ]
    resources = [
      "arn:aws:s3:::${local.bucket_names.terraform_state}/*",
    ]

    principals {
      type        = "*"
      identifiers = ["*"]
    }

    condition {
      test     = "ArnNotLike"
      variable = "aws:PrincipalArn"
      # Only the Terraform execution role may delete state (break-glass scenario)
      values = [
        "arn:aws:iam::${var.aws_account_id}:role/logisticos-terraform-executor",
      ]
    }
  }
}

resource "aws_s3_bucket_policy" "terraform_state" {
  count = local.create_terraform_state ? 1 : 0

  bucket = aws_s3_bucket.terraform_state[0].id
  policy = data.aws_iam_policy_document.terraform_state_policy[0].json

  depends_on = [aws_s3_bucket_public_access_block.terraform_state]
}

resource "aws_s3_bucket_lifecycle_configuration" "terraform_state" {
  count = local.create_terraform_state ? 1 : 0

  bucket = aws_s3_bucket.terraform_state[0].id

  rule {
    id     = "expire-old-state-versions"
    status = "Enabled"

    noncurrent_version_transition {
      noncurrent_days = 30
      storage_class   = "STANDARD_IA"
    }

    noncurrent_version_expiration {
      # Keep 90 days of state history; older non-current versions are removed
      noncurrent_days           = 90
      newer_noncurrent_versions = 30
    }

    abort_incomplete_multipart_upload {
      days_after_initiation = 1
    }
  }
}

###############################################################################
# Outputs
###############################################################################

output "bucket_names" {
  description = "Map of logical bucket identifier to actual S3 bucket name."
  value = merge(
    {
      pod_photos      = aws_s3_bucket.pod_photos.id
      exports         = aws_s3_bucket.exports.id
      model_artifacts = aws_s3_bucket.model_artifacts.id
      backups         = aws_s3_bucket.backups.id
    },
    local.create_terraform_state ? { terraform_state = aws_s3_bucket.terraform_state[0].id } : {}
  )
}

output "bucket_arns" {
  description = "Map of logical bucket identifier to S3 bucket ARN."
  value = merge(
    {
      pod_photos      = aws_s3_bucket.pod_photos.arn
      exports         = aws_s3_bucket.exports.arn
      model_artifacts = aws_s3_bucket.model_artifacts.arn
      backups         = aws_s3_bucket.backups.arn
    },
    local.create_terraform_state ? { terraform_state = aws_s3_bucket.terraform_state[0].arn } : {}
  )
}

output "pod_photos_bucket_name" {
  description = "POD photos bucket name — convenience output for IAM and application modules."
  value       = aws_s3_bucket.pod_photos.id
}

output "pod_photos_bucket_arn" {
  description = "POD photos bucket ARN."
  value       = aws_s3_bucket.pod_photos.arn
}

output "model_artifacts_bucket_name" {
  description = "Model artifacts bucket name — consumed by the IAM and AI layer modules."
  value       = aws_s3_bucket.model_artifacts.id
}

output "model_artifacts_bucket_arn" {
  description = "Model artifacts bucket ARN."
  value       = aws_s3_bucket.model_artifacts.arn
}

output "exports_bucket_name" {
  description = "Exports bucket name."
  value       = aws_s3_bucket.exports.id
}

output "exports_bucket_arn" {
  description = "Exports bucket ARN."
  value       = aws_s3_bucket.exports.arn
}

output "backups_bucket_name" {
  description = "Backups bucket name."
  value       = aws_s3_bucket.backups.id
}

output "backups_bucket_arn" {
  description = "Backups bucket ARN."
  value       = aws_s3_bucket.backups.arn
}
