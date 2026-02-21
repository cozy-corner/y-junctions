locals {
  cockroachdb_connection_uri = "postgresql://${cockroach_sql_user.main.name}:${urlencode(var.cockroachdb_sql_password)}@${cockroach_cluster.main.regions[0].sql_dns}:26257/${cockroach_database.main.name}?sslmode=require"
}

resource "cockroach_cluster" "main" {
  name           = "y-junctions"
  cloud_provider = "GCP"
  plan           = "BASIC"

  regions = [
    {
      name    = "asia-southeast1"
      primary = true
    }
  ]

  serverless = {
    usage_limits = {
      request_unit_limit = 50000000
      storage_mib_limit  = 10240
    }
  }
}

resource "cockroach_database" "main" {
  name       = "y_junctions"
  cluster_id = cockroach_cluster.main.id
}

resource "cockroach_sql_user" "main" {
  name       = "y_junctions_user"
  password   = var.cockroachdb_sql_password
  cluster_id = cockroach_cluster.main.id
}
