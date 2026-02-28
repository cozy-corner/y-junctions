resource "google_bigquery_dataset" "cloud_run_logs" {
  dataset_id    = "cloud_run_logs"
  friendly_name = "Cloud Run Logs"
  description   = "Logs from Cloud Run services for analytics"
  location      = var.region

  default_partition_expiration_ms = 90 * 24 * 60 * 60 * 1000
}

resource "google_logging_project_sink" "cloud_run_requests" {
  name        = "cloud-run-requests-to-bigquery"
  destination = "bigquery.googleapis.com/projects/${var.project_id}/datasets/${google_bigquery_dataset.cloud_run_logs.dataset_id}"

  filter = <<-EOT
    resource.type="cloud_run_revision"
    resource.labels.service_name="${var.backend_service_name}"
    httpRequest.requestMethod!=""
    NOT httpRequest.requestUrl=~"/health"
  EOT

  bigquery_options {
    use_partitioned_tables = true
  }

  unique_writer_identity = true
}

resource "google_bigquery_dataset_iam_member" "logging_sink_bigquery" {
  dataset_id = google_bigquery_dataset.cloud_run_logs.dataset_id
  role       = "roles/bigquery.dataEditor"
  member     = google_logging_project_sink.cloud_run_requests.writer_identity
}
