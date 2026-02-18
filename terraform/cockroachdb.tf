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
