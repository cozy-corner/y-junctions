output "neon_connection_uri" {
  description = "Neon database connection URI"
  value       = neon_project.main.connection_uri
  sensitive   = true
}

output "neon_project_id" {
  description = "Neon project ID"
  value       = neon_project.main.id
}

output "backend_url" {
  description = "Backend API URL"
  value       = google_cloud_run_v2_service.backend.uri
}

output "frontend_bucket_url" {
  description = "Frontend public URL"
  value       = "https://storage.googleapis.com/${google_storage_bucket.frontend.name}/index.html"
}

output "frontend_bucket_name" {
  description = "Frontend bucket name for deployment"
  value       = google_storage_bucket.frontend.name
}

output "artifact_registry_repository" {
  description = "Artifact Registry repository URL"
  value       = "${var.region}-docker.pkg.dev/${var.project_id}/${google_artifact_registry_repository.main.repository_id}"
}

output "workload_identity_provider" {
  description = "Workload Identity Provider name for GitHub Actions"
  value       = google_iam_workload_identity_pool_provider.github.name
}

output "service_account_email" {
  description = "Service account email for GitHub Actions"
  value       = google_service_account.github_actions_deployer.email
}

output "cockroachdb_connection_uri" {
  description = "CockroachDB Serverless connection URI"
  value       = "postgresql://${cockroach_sql_user.main.name}:${var.cockroachdb_sql_password}@${cockroach_cluster.main.regions[0].sql_dns}:26257/${cockroach_database.main.name}?sslmode=require"
  sensitive   = true
}
