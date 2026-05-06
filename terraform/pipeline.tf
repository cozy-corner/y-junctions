###############################################################################
# Cloud Run Jobs walking-skeleton (issue #229)
#
# Pipeline: download-osm -> [extract-three-way || extract-two-way] -> prepare-serving -> load-to-cockroach
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

# Catalog bucket for pipeline dataset definitions (issue #237). Object content
# is operator-managed via `gsutil cp pipeline/datasets.json gs://...-yj-config/`;
# Terraform owns the bucket only, not the contents. Versioning enabled so a
# bad edit can be rolled back with `gsutil cp gs://...?generation=...`.
resource "google_storage_bucket" "yj_config" {
  name                        = "${var.project_id}-yj-config"
  location                    = var.region
  uniform_bucket_level_access = true
  public_access_prevention    = "enforced"
  force_destroy               = false

  versioning {
    enabled = true
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

resource "google_service_account" "pipeline_extract_two_way" {
  account_id   = "sa-pipeline-extract-two-way"
  display_name = "Pipeline: extract-two-way"

  depends_on = [google_project_service.iam]
}

resource "google_service_account" "pipeline_prepare_serving" {
  account_id   = "sa-pipeline-prepare-serving"
  display_name = "Pipeline: prepare-serving"

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

resource "google_service_account" "pipeline_dispatcher" {
  account_id   = "sa-pipeline-dispatcher"
  display_name = "Pipeline: catalog dispatcher (issue #237)"

  depends_on = [google_project_service.iam]
}

# ---------- IAM: per-bucket access ------------------------------------------

# download-osm: write to yj-raw
resource "google_storage_bucket_iam_member" "download_osm_writes_raw" {
  bucket = google_storage_bucket.yj_raw.name
  role   = "roles/storage.objectAdmin"
  member = "serviceAccount:${google_service_account.pipeline_download_osm.email}"
}

# extract-three-way: read yj-raw, write yj-extracted
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

# extract-two-way: read yj-raw, write yj-extracted
resource "google_storage_bucket_iam_member" "extract_two_way_reads_raw" {
  bucket = google_storage_bucket.yj_raw.name
  role   = "roles/storage.objectViewer"
  member = "serviceAccount:${google_service_account.pipeline_extract_two_way.email}"
}

resource "google_storage_bucket_iam_member" "extract_two_way_writes_extracted" {
  bucket = google_storage_bucket.yj_extracted.name
  role   = "roles/storage.objectAdmin"
  member = "serviceAccount:${google_service_account.pipeline_extract_two_way.email}"
}

# prepare-serving: read yj-extracted, write yj-serving
resource "google_storage_bucket_iam_member" "prepare_serving_reads_extracted" {
  bucket = google_storage_bucket.yj_extracted.name
  role   = "roles/storage.objectViewer"
  member = "serviceAccount:${google_service_account.pipeline_prepare_serving.email}"
}

resource "google_storage_bucket_iam_member" "prepare_serving_writes_serving" {
  bucket = google_storage_bucket.yj_serving.name
  role   = "roles/storage.objectAdmin"
  member = "serviceAccount:${google_service_account.pipeline_prepare_serving.email}"
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

resource "google_cloud_run_v2_job" "pipeline_extract_two_way" {
  name     = "pipeline-extract-two-way"
  location = var.region

  deletion_protection = false

  template {
    template {
      service_account = google_service_account.pipeline_extract_two_way.email
      timeout         = "1800s"
      max_retries     = 1

      containers {
        image   = local.pipeline_image
        command = ["pipeline-extract-two-way"]

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

resource "google_cloud_run_v2_job" "pipeline_prepare_serving" {
  name     = "pipeline-prepare-serving"
  location = var.region

  deletion_protection = false

  template {
    template {
      service_account = google_service_account.pipeline_prepare_serving.email
      timeout         = "1800s"
      max_retries     = 1

      containers {
        image   = local.pipeline_image
        command = ["pipeline-prepare-serving"]

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

resource "google_service_account_iam_member" "workflow_acts_as_extract_two_way" {
  service_account_id = google_service_account.pipeline_extract_two_way.name
  role               = "roles/iam.serviceAccountUser"
  member             = "serviceAccount:${google_service_account.pipeline_workflow.email}"
}

resource "google_service_account_iam_member" "workflow_acts_as_prepare_serving" {
  service_account_id = google_service_account.pipeline_prepare_serving.name
  role               = "roles/iam.serviceAccountUser"
  member             = "serviceAccount:${google_service_account.pipeline_workflow.email}"
}

resource "google_service_account_iam_member" "workflow_acts_as_load" {
  service_account_id = google_service_account.pipeline_load_to_cockroach.name
  role               = "roles/iam.serviceAccountUser"
  member             = "serviceAccount:${google_service_account.pipeline_workflow.email}"
}

# Workflow constructs per-job containerOverrides from caller-supplied args.
# All of `dataset` / `geofabrik_url` / `bbox` are required — the dispatcher
# (issue #237) sources them from `gs://...-yj-config/datasets.json`, ad-hoc
# runs pass them via `gcloud workflows execute --data='{...}'`. Removing the
# defaults keeps workload identity out of infrastructure-as-code.
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
              - dataset: $${args.dataset}
              - geofabrik_url: $${args.geofabrik_url}
              - bbox: $${args.bbox}
              - run_date: $${text.substring(time.format(sys.now(), "Asia/Tokyo"), 0, 10)}
              - raw_uri: $${"gs://${google_storage_bucket.yj_raw.name}/osm/" + run_date + "/" + dataset + ".osm.pbf"}
              - extracted_three_way_uri: $${"gs://${google_storage_bucket.yj_extracted.name}/three-way/" + run_date + "/" + dataset + ".parquet"}
              - extracted_two_way_uri: $${"gs://${google_storage_bucket.yj_extracted.name}/two-way/" + run_date + "/" + dataset + ".parquet"}
              - serving_uri: $${"gs://${google_storage_bucket.yj_serving.name}/" + run_date + "/" + dataset + ".parquet"}
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
        # 3-way / 2-way share the same PBF input; running them in parallel
        # keeps failure isolation per #220 (one extractor's OOM doesn't kill
        # the other) at the cost of parsing the PBF twice.
        - extract:
            parallel:
              branches:
                - extract_three_way:
                    steps:
                      - run_extract_three_way:
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
                                      - $${extracted_three_way_uri}
                                      - "--bbox"
                                      - $${bbox}
                - extract_two_way:
                    steps:
                      - run_extract_two_way:
                          call: googleapis.run.v2.projects.locations.jobs.run
                          args:
                            name: projects/${var.project_id}/locations/${var.region}/jobs/${google_cloud_run_v2_job.pipeline_extract_two_way.name}
                            body:
                              overrides:
                                containerOverrides:
                                  - args:
                                      - "--input"
                                      - $${raw_uri}
                                      - "--extracted-output"
                                      - $${extracted_two_way_uri}
                                      - "--bbox"
                                      - $${bbox}
        - prepare_serving:
            call: googleapis.run.v2.projects.locations.jobs.run
            args:
              name: projects/${var.project_id}/locations/${var.region}/jobs/${google_cloud_run_v2_job.pipeline_prepare_serving.name}
              body:
                overrides:
                  containerOverrides:
                    - args:
                        - "--input"
                        - $${extracted_three_way_uri}
                        - "--input"
                        - $${extracted_two_way_uri}
                        - "--output"
                        - $${serving_uri}
            result: prepare_serving_result
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

# ---------- Catalog dispatcher (issue #237) ----------------------------------
#
# Single Cloud Scheduler trigger -> dispatcher Workflow -> fan-out to
# yj-pipeline executions, one per dataset matching the requested schedule.
# Adding a dataset is a `gsutil cp pipeline/datasets.json gs://...-yj-config/`,
# not a `terraform apply` — workload identity stays out of IaC.

# Dispatcher SA reads catalog from yj-config bucket.
resource "google_storage_bucket_iam_member" "dispatcher_reads_config" {
  bucket = google_storage_bucket.yj_config.name
  role   = "roles/storage.objectViewer"
  member = "serviceAccount:${google_service_account.pipeline_dispatcher.email}"
}

# Dispatcher SA invokes yj-pipeline executions. Project-level matches the
# scheduler pattern; tighten to per-workflow if/when invoker count grows.
resource "google_project_iam_member" "dispatcher_invokes_pipeline" {
  project = var.project_id
  role    = "roles/workflows.invoker"
  member  = "serviceAccount:${google_service_account.pipeline_dispatcher.email}"
}

resource "google_workflows_workflow" "dispatcher" {
  name            = "yj-pipeline-dispatcher"
  region          = var.region
  service_account = google_service_account.pipeline_dispatcher.id

  source_contents = <<-EOT
    main:
      params: [args]
      steps:
        - init:
            assign:
              - schedule_filter: $${default(map.get(args, "schedule"), "monthly")}
        - read_catalog:
            call: http.get
            args:
              url: https://storage.googleapis.com/${google_storage_bucket.yj_config.name}/datasets.json
              auth:
                type: OAuth2
            result: catalog_raw
        # http.get auto-parses body when Content-Type is application/json, but
        # gsutil's content-type detection is best-effort — fall back to an
        # explicit json.decode when the body comes back as a string/bytes.
        - parse_catalog:
            switch:
              - condition: $${get_type(catalog_raw.body) == "list"}
                steps:
                  - body_already_list:
                      assign:
                        - catalog: $${catalog_raw.body}
              - condition: true
                steps:
                  - decode_body:
                      assign:
                        - catalog: $${json.decode(catalog_raw.body)}
        # Workflows has no built-in list.filter / lambda; do the schedule
        # check inline in the parallel.for loop. Non-matching entries are
        # cheap no-ops, no need for a separate filter pass.
        - fan_out:
            parallel:
              concurrency_limit: 4
              for:
                value: ds
                in: $${catalog}
                steps:
                  - dispatch_if_matches:
                      switch:
                        - condition: $${ds.schedule == schedule_filter}
                          steps:
                            - run_pipeline:
                                call: googleapis.workflowexecutions.v1.projects.locations.workflows.executions.create
                                args:
                                  parent: projects/${var.project_id}/locations/${var.region}/workflows/${google_workflows_workflow.pipeline.name}
                                  body:
                                    argument: $${json.encode_to_string(ds)}
  EOT

  depends_on = [
    google_project_service.workflows,
    google_storage_bucket_iam_member.dispatcher_reads_config,
    google_project_iam_member.dispatcher_invokes_pipeline,
  ]
}

# ---------- Cloud Scheduler --------------------------------------------------

resource "google_project_iam_member" "scheduler_invokes_workflow" {
  project = var.project_id
  role    = "roles/workflows.invoker"
  member  = "serviceAccount:${google_service_account.pipeline_scheduler.email}"
}

# Scheduled trigger for the dispatcher. The cron cadence is data, not part
# of the resource identity — change `schedule` (or pause via `gcloud
# scheduler jobs pause yj-pipeline-trigger`) without renaming the resource.
# The dispatcher reads `gs://...-yj-config/datasets.json`, filters to
# `schedule == "monthly"`, and fans out to yj-pipeline executions.
#
# Manual verification path:
#   gcloud workflows execute yj-pipeline-dispatcher --location=asia-northeast1
resource "google_cloud_scheduler_job" "pipeline_trigger" {
  name        = "yj-pipeline-trigger"
  region      = var.region
  description = "Scheduled trigger for the catalog dispatcher (issue #237)"
  paused      = false       # Active: dispatcher reads catalog and fans out monthly.
  schedule    = "0 3 1 * *" # 03:00 JST on the 1st of each month
  time_zone   = "Asia/Tokyo"

  http_target {
    http_method = "POST"
    uri         = "https://workflowexecutions.googleapis.com/v1/${google_workflows_workflow.dispatcher.id}/executions"

    oauth_token {
      service_account_email = google_service_account.pipeline_scheduler.email
    }
  }

  depends_on = [google_project_service.cloudscheduler]
}
