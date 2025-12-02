# Neon Project
resource "neon_project" "main" {
  name       = "y_junctions"
  region_id  = "aws-ap-southeast-1"
  pg_version = 16

  # 無料枠の最大値（6時間）
  history_retention_seconds = 21600

  branch {
    name          = "main"
    database_name = "y_junctions"
    role_name     = "y_junctions_user"
  }

  default_endpoint_settings {
    autoscaling_limit_min_cu = 0.25
    autoscaling_limit_max_cu = 0.25
  }
}
