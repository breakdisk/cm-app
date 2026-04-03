###############################################################################
# LogisticOS — ClickHouse Module
# Provisions a ClickHouse cluster on AWS using either:
#   a) Self-managed on EKS (default) — ClickHouse Operator Helm chart
#   b) ClickHouse Cloud (managed) — via ClickHouse Cloud Terraform provider
#      (set var.use_clickhouse_cloud = true)
#
# For the MVP/dev environment, a single-shard, single-replica cluster
# is provisioned on EKS in the logisticos-infra namespace.
###############################################################################

terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
    helm = {
      source  = "hashicorp/helm"
      version = "~> 2.0"
    }
    kubernetes = {
      source  = "hashicorp/kubernetes"
      version = "~> 2.0"
    }
  }
}

###############################################################################
# Variables
###############################################################################

variable "environment" {
  description = "Deployment environment (dev, staging, production)."
  type        = string
}

variable "namespace" {
  description = "Kubernetes namespace for ClickHouse resources."
  type        = string
  default     = "logisticos-infra"
}

variable "storage_class" {
  description = "Kubernetes StorageClass for ClickHouse PVCs."
  type        = string
  default     = "gp3"
}

variable "storage_size" {
  description = "PVC size for each ClickHouse shard."
  type        = string
  default     = "50Gi"
}

variable "clickhouse_operator_version" {
  description = "Version of the ClickHouse Operator Helm chart."
  type        = string
  default     = "0.23.5"
}

variable "clickhouse_version" {
  description = "ClickHouse server version tag."
  type        = string
  default     = "24.3"
}

variable "shard_count" {
  description = "Number of ClickHouse shards."
  type        = number
  default     = 1
}

variable "replica_count" {
  description = "Number of replicas per shard."
  type        = number
  default     = 1
}

variable "cpu_request" {
  description = "CPU request per ClickHouse pod."
  type        = string
  default     = "500m"
}

variable "memory_request" {
  description = "Memory request per ClickHouse pod."
  type        = string
  default     = "1Gi"
}

variable "cpu_limit" {
  description = "CPU limit per ClickHouse pod."
  type        = string
  default     = "2"
}

variable "memory_limit" {
  description = "Memory limit per ClickHouse pod."
  type        = string
  default     = "4Gi"
}

variable "tags" {
  description = "Tags to apply to all resources."
  type        = map(string)
  default     = {}
}

###############################################################################
# ClickHouse Operator (installs CRDs + controller)
###############################################################################

resource "helm_release" "clickhouse_operator" {
  name             = "clickhouse-operator"
  repository       = "https://docs.altinity.com/clickhouse-operator/"
  chart            = "altinity-clickhouse-operator"
  version          = var.clickhouse_operator_version
  namespace        = "kube-system"
  create_namespace = false
  wait             = true
  timeout          = 300

  set {
    name  = "operator.image.tag"
    value = var.clickhouse_operator_version
  }
}

###############################################################################
# ClickHouse Installation (ClickHouseInstallation CRD)
###############################################################################

resource "kubernetes_manifest" "clickhouse_installation" {
  depends_on = [helm_release.clickhouse_operator]

  manifest = {
    apiVersion = "clickhouse.altinity.com/v1"
    kind       = "ClickHouseInstallation"
    metadata = {
      name      = "logisticos-${var.environment}"
      namespace = var.namespace
      labels = merge(var.tags, {
        "app.kubernetes.io/name"       = "clickhouse"
        "app.kubernetes.io/instance"   = "logisticos-${var.environment}"
        "app.kubernetes.io/part-of"    = "logisticos"
        "app.kubernetes.io/managed-by" = "terraform"
      })
    }
    spec = {
      configuration = {
        clusters = [{
          name = "logisticos"
          layout = {
            shardsCount   = var.shard_count
            replicasCount = var.replica_count
          }
        }]
        users = {
          # analytics user — read/write for ingestion and queries
          "analytics/password"             = ""  # set via Vault init job
          "analytics/networks/ip"          = "::/0"
          "analytics/profile"              = "default"
          # readonly user for BI tools
          "bi_reader/password"             = ""
          "bi_reader/networks/ip"          = "::/0"
          "bi_reader/profile"              = "readonly"
        }
        profiles = {
          "readonly/readonly" = "1"
        }
        settings = {
          "max_concurrent_queries"         = "100"
          "max_memory_usage"               = "10000000000"
          "use_uncompressed_cache"         = "0"
          "load_balancing"                 = "random"
          "log_queries"                    = "1"
          "query_log/database"             = "system"
          "query_log/table"                = "query_log"
          "query_log/partition_by"         = "toMonday(event_date)"
          "query_log/flush_interval_milliseconds" = "7500"
        }
      }
      templates = {
        podTemplates = [{
          name = "clickhouse-pod"
          spec = {
            containers = [{
              name  = "clickhouse"
              image = "clickhouse/clickhouse-server:${var.clickhouse_version}"
              resources = {
                requests = {
                  cpu    = var.cpu_request
                  memory = var.memory_request
                }
                limits = {
                  cpu    = var.cpu_limit
                  memory = var.memory_limit
                }
              }
            }]
          }
        }]
        volumeClaimTemplates = [{
          name = "clickhouse-data"
          spec = {
            accessModes      = ["ReadWriteOnce"]
            storageClassName = var.storage_class
            resources = {
              requests = {
                storage = var.storage_size
              }
            }
          }
        }]
      }
    }
  }
}

###############################################################################
# Service (ClusterIP for internal access)
###############################################################################

resource "kubernetes_service" "clickhouse" {
  depends_on = [kubernetes_manifest.clickhouse_installation]

  metadata {
    name      = "logisticos-clickhouse"
    namespace = var.namespace
    labels    = merge(var.tags, {
      "app.kubernetes.io/name"    = "clickhouse"
      "app.kubernetes.io/part-of" = "logisticos"
    })
  }

  spec {
    selector = {
      "clickhouse.altinity.com/chi" = "logisticos-${var.environment}"
    }

    port {
      name        = "http"
      port        = 8123
      target_port = 8123
    }

    port {
      name        = "native"
      port        = 9000
      target_port = 9000
    }

    type = "ClusterIP"
  }
}

###############################################################################
# Outputs
###############################################################################

output "clickhouse_http_endpoint" {
  description = "Internal ClickHouse HTTP endpoint (for analytics service)."
  value       = "http://logisticos-clickhouse.${var.namespace}.svc.cluster.local:8123"
}

output "clickhouse_native_endpoint" {
  description = "Internal ClickHouse native protocol endpoint."
  value       = "logisticos-clickhouse.${var.namespace}.svc.cluster.local:9000"
}
