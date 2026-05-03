###############################################################################
# Cloud Run Jobs walking-skeleton (issue #229)
#
# Pipeline: download-osm -> extract-three-way -> load-to-cockroach
#
# Workload parameters (dataset / geofabrik url / bbox) are NOT baked into
# the infrastructure. Cloud Run Jobs declare only `command` here; arguments
# are supplied per execution via the Workflow's containerOverrides. Defaults
# live in the Workflow source, and ad-hoc runs can override them via
# `gcloud workflows execute --data='{"dataset": ..., "bbox": ...}'`.
###############################################################################

# ---------- API enablement ---------------------------------------------------

resource "google_project_service" "workflows" {
  service            = "workflows.googleapis.com"
  disable_on_destroy = false
}

resource "google_project_service" "cloudscheduler" {
  service            = "cloudscheduler.googleapis.com"
  disable_on_destroy = false
}

resource "google_project_service" "secretmanager" {
  service            = "secretmanager.googleapis.com"
  disable_on_destroy = false
}

# ---------- Configuration ----------------------------------------------------

locals {
  pipeline_image = "${var.region}-docker.pkg.dev/${var.project_id}/${google_artifact_registry_repository.main.repository_id}/pipeline:latest"

  pipeline_db_uri = "postgresql://${cockroach_sql_user.main.name}:${urlencode(var.cockroachdb_sql_password)}@${cockroach_cluster.main.regions[0].sql_dns}:26257/${cockroach_database.pipeline.name}?sslmode=require"
}

# ---------- GCS buckets ------------------------------------------------------

resource "google_storage_bucket" "yj_raw" {
  name                        = "${var.project_id}-yj-raw"
  location                    = var.region
  uniform_bucket_level_access = true
  public_access_prevention    = "enforced"
  force_destroy               = true

  lifecycle_rule {
    condition {
      age = 90
    }
    action {
      type = "Delete"
    }
  }

  depends_on = [google_project_service.storage]
}

resource "google_storage_bucket" "yj_extracted" {
  name                        = "${var.project_id}-yj-extracted"
  location                    = var.region
  uniform_bucket_level_access = true
  public_access_prevention    = "enforced"
  force_destroy               = true

  lifecycle_rule {
    condition {
      age = 90
    }
    action {
      type = "Delete"
    }
  }

  depends_on = [google_project_service.storage]
}

resource "google_storage_bucket" "yj_serving" {
  name                        = "${var.project_id}-yj-serving"
  location                    = var.region
  uniform_bucket_level_access = true
  public_access_prevention    = "enforced"
  force_destroy               = true

  lifecycle_rule {
    condition {
      age = 90
    }
    action {
      type = "Delete"
    }
  }

  depends_on = [google_project_service.storage]
}

# ---------- CockroachDB pipeline database ------------------------------------
#
# Same cluster as production (asia-southeast1, BASIC plan), separate database.
# Reuses the existing y_junctions_user; consumes from the cluster's
# request-unit quota (request_unit_limit = 50_000_000).

resource "cockroach_database" "pipeline" {
  name       = "y_junctions_pipeline"
  cluster_id = cockroach_cluster.main.id
}

# ---------- Secret Manager: pipeline DB URL ----------------------------------

resource "google_secret_manager_secret" "pipeline_db_url" {
  secret_id = "cockroachdb-pipeline-url"

  replication {
    auto {}
  }

  depends_on = [google_project_service.secretmanager]
}

resource "google_secret_manager_secret_version" "pipeline_db_url_v1" {
  secret      = google_secret_manager_secret.pipeline_db_url.id
  secret_data = local.pipeline_db_uri
}

# ---------- Service accounts (one per job) -----------------------------------

resource "google_service_account" "pipeline_download_osm" {
  account_id   = "sa-pipeline-download-osm"
  display_name = "Pipeline: download-osm"

  depends_on = [google_project_service.iam]
}

resource "google_service_account" "pipeline_extract_three_way" {
  account_id   = "sa-pipeline-extract-three-way"
  display_name = "Pipeline: extract-three-way"

  depends_on = [google_project_service.iam]
}

resource "google_service_account" "pipeline_load_to_cockroach" {
  account_id   = "sa-pipeline-load-to-cockroach"
  display_name = "Pipeline: load-to-cockroach"

  depends_on = [google_project_service.iam]
}

resource "google_service_account" "pipeline_workflow" {
  account_id   = "sa-pipeline-workflow"
  display_name = "Pipeline: workflow orchestrator"

  depends_on = [google_project_service.iam]
}

resource "google_service_account" "pipeline_scheduler" {
  account_id   = "sa-pipeline-scheduler"
  display_name = "Pipeline: scheduler trigger"

  depends_on = [google_project_service.iam]
}

# ---------- IAM: per-bucket access ------------------------------------------

# download-osm: write to yj-raw
resource "google_storage_bucket_iam_member" "download_osm_writes_raw" {
  bucket = google_storage_bucket.yj_raw.name
  role   = "roles/storage.objectAdmin"
  member = "serviceAccount:${google_service_account.pipeline_download_osm.email}"
}

# extract-three-way: read yj-raw, write yj-extracted + yj-serving
resource "google_storage_bucket_iam_member" "extract_reads_raw" {
  bucket = google_storage_bucket.yj_raw.name
  role   = "roles/storage.objectViewer"
  member = "serviceAccount:${google_service_account.pipeline_extract_three_way.email}"
}

resource "google_storage_bucket_iam_member" "extract_writes_extracted" {
  bucket = google_storage_bucket.yj_extracted.name
  role   = "roles/storage.objectAdmin"
  member = "serviceAccount:${google_service_account.pipeline_extract_three_way.email}"
}

resource "google_storage_bucket_iam_member" "extract_writes_serving" {
  bucket = google_storage_bucket.yj_serving.name
  role   = "roles/storage.objectAdmin"
  member = "serviceAccount:${google_service_account.pipeline_extract_three_way.email}"
}

# load-to-cockroach: read yj-serving, read DB secret
resource "google_storage_bucket_iam_member" "load_reads_serving" {
  bucket = google_storage_bucket.yj_serving.name
  role   = "roles/storage.objectViewer"
  member = "serviceAccount:${google_service_account.pipeline_load_to_cockroach.email}"
}

resource "google_secret_manager_secret_iam_member" "load_reads_pipeline_db_secret" {
  secret_id = google_secret_manager_secret.pipeline_db_url.id
  role      = "roles/secretmanager.secretAccessor"
  member    = "serviceAccount:${google_service_account.pipeline_load_to_cockroach.email}"
}

# ---------- Cloud Run Jobs ---------------------------------------------------
#
# `args` is intentionally omitted; the Workflow supplies arguments per
# execution via containerOverrides. This keeps "what to process" out of
# infrastructure-as-code so a different dataset / bbox does not require
# `terraform apply`.

resource "google_cloud_run_v2_job" "pipeline_download_osm" {
  name     = "pipeline-download-osm"
  location = var.region

  deletion_protection = false

  template {
    template {
      service_account = google_service_account.pipeline_download_osm.email
      timeout         = "1800s"
      max_retries     = 1

      containers {
        image   = local.pipeline_image
        command = ["pipeline-download-osm"]

        resources {
          limits = {
            cpu    = "1"
            memory = "2Gi"
          }
        }
      }
    }
  }

  depends_on = [google_project_service.run]
}

resource "google_cloud_run_v2_job" "pipeline_extract_three_way" {
  name     = "pipeline-extract-three-way"
  location = var.region

  template {
    template {
      service_account = google_service_account.pipeline_extract_three_way.email
      timeout         = "1800s"
      max_retries     = 1

      containers {
        image   = local.pipeline_image
        command = ["pipeline-extract-three-way"]

        resources {
          limits = {
            cpu    = "2"
            memory = "8Gi"
          }
        }
      }
    }
  }

  depends_on = [google_project_service.run]
}

resource "google_cloud_run_v2_job" "pipeline_load_to_cockroach" {
  name     = "pipeline-load-to-cockroach"
  location = var.region

  template {
    template {
      service_account = google_service_account.pipeline_load_to_cockroach.email
      timeout         = "1800s"
      max_retries     = 1

      containers {
        image   = local.pipeline_image
        command = ["pipeline-load-to-cockroach"]

        env {
          name = "DATABASE_URL"
          value_source {
            secret_key_ref {
              secret  = google_secret_manager_secret.pipeline_db_url.secret_id
              version = "latest"
            }
          }
        }

        resources {
          limits = {
            cpu    = "1"
            memory = "2Gi"
          }
        }
      }
    }
  }

  depends_on = [
    google_project_service.run,
    google_secret_manager_secret_iam_member.load_reads_pipeline_db_secret,
  ]
}

# ---------- Workflows --------------------------------------------------------

resource "google_project_iam_member" "workflow_runs_jobs" {
  project = var.project_id
  role    = "roles/run.developer"
  member  = "serviceAccount:${google_service_account.pipeline_workflow.email}"
}

# Workflows needs to act-as the Cloud Run Jobs' service accounts to invoke them.
resource "google_service_account_iam_member" "workflow_acts_as_download_osm" {
  service_account_id = google_service_account.pipeline_download_osm.name
  role               = "roles/iam.serviceAccountUser"
  member             = "serviceAccount:${google_service_account.pipeline_workflow.email}"
}

resource "google_service_account_iam_member" "workflow_acts_as_extract" {
  service_account_id = google_service_account.pipeline_extract_three_way.name
  role               = "roles/iam.serviceAccountUser"
  member             = "serviceAccount:${google_service_account.pipeline_workflow.email}"
}

resource "google_service_account_iam_member" "workflow_acts_as_load" {
  service_account_id = google_service_account.pipeline_load_to_cockroach.name
  role               = "roles/iam.serviceAccountUser"
  member             = "serviceAccount:${google_service_account.pipeline_workflow.email}"
}

# Workflow holds the workload defaults (dataset / geofabrik URL / bbox) and
# constructs per-job containerOverrides. Callers can override any of these by
# passing JSON to `gcloud workflows execute --data`.
resource "google_workflows_workflow" "pipeline" {
  name            = "yj-pipeline"
  region          = var.region
  service_account = google_service_account.pipeline_workflow.id

  source_contents = <<-EOT
    main:
      params: [args]
      steps:
        - init:
            assign:
              - dataset: $${default(map.get(args, "dataset"), "shikoku-latest")}
              - geofabrik_url: $${default(map.get(args, "geofabrik_url"), "https://download.geofabrik.de/asia/japan/shikoku-latest.osm.pbf")}
              - bbox: $${default(map.get(args, "bbox"), "134.0,34.3,134.1,34.4")}
              - raw_uri: $${"gs://${google_storage_bucket.yj_raw.name}/osm/" + dataset + ".osm.pbf"}
              - extracted_uri: $${"gs://${google_storage_bucket.yj_extracted.name}/three-way/" + dataset + ".parquet"}
              - serving_uri: $${"gs://${google_storage_bucket.yj_serving.name}/three-way/" + dataset + ".parquet"}
        - download:
            call: googleapis.run.v2.projects.locations.jobs.run
            args:
              name: projects/${var.project_id}/locations/${var.region}/jobs/${google_cloud_run_v2_job.pipeline_download_osm.name}
              body:
                overrides:
                  containerOverrides:
                    - args:
                        - "--input"
                        - $${geofabrik_url}
                        - "--output"
                        - $${raw_uri}
            result: download_result
        - extract:
            call: googleapis.run.v2.projects.locations.jobs.run
            args:
              name: projects/${var.project_id}/locations/${var.region}/jobs/${google_cloud_run_v2_job.pipeline_extract_three_way.name}
              body:
                overrides:
                  containerOverrides:
                    - args:
                        - "--input"
                        - $${raw_uri}
                        - "--extracted-output"
                        - $${extracted_uri}
                        - "--serving-output"
                        - $${serving_uri}
                        - "--bbox"
                        - $${bbox}
            result: extract_result
        - load:
            call: googleapis.run.v2.projects.locations.jobs.run
            args:
              name: projects/${var.project_id}/locations/${var.region}/jobs/${google_cloud_run_v2_job.pipeline_load_to_cockroach.name}
              body:
                overrides:
                  containerOverrides:
                    - args:
                        - "--input"
                        - $${serving_uri}
            result: load_result
        - done:
            return: $${load_result}
  EOT

  depends_on = [google_project_service.workflows]
}

# ---------- Cloud Scheduler --------------------------------------------------

resource "google_project_iam_member" "scheduler_invokes_workflow" {
  project = var.project_id
  role    = "roles/workflows.invoker"
  member  = "serviceAccount:${google_service_account.pipeline_scheduler.email}"
}

# Scheduled trigger for the pipeline. The cron cadence is data, not part
# of the resource identity — change `schedule` (or pause via `gcloud
# scheduler jobs pause yj-pipeline-trigger`) without renaming the resource.
#
# Initial verification path:
#   gcloud workflows execute yj-pipeline --location=asia-northeast1
resource "google_cloud_scheduler_job" "pipeline_trigger" {
  name        = "yj-pipeline-trigger"
  region      = var.region
  description = "Scheduled trigger for the walking-skeleton pipeline (issue #229)"
  paused      = true        # Manual verification only; unpause once dispatcher (#237) lands.
  schedule    = "0 3 1 * *" # 03:00 JST on the 1st of each month
  time_zone   = "Asia/Tokyo"

  http_target {
    http_method = "POST"
    uri         = "https://workflowexecutions.googleapis.com/v1/${google_workflows_workflow.pipeline.id}/executions"

    oauth_token {
      service_account_email = google_service_account.pipeline_scheduler.email
    }
  }

  depends_on = [google_project_service.cloudscheduler]
}
