# Street View Static API metadata を叩くための API 有効化とキー発行。
# キーはローカルの enrich バッチ（backend/src/bin, issue #306）が backend/.env
# から読むために使う。サーバー（Cloud Run）には載せないので Secret Manager /
# Cloud Run への配線は無い。キー値は output 経由でローカル .env に貼る。
# 詳細設計: doc/streetview-coverage-filter.md

resource "google_project_service" "streetview" {
  service            = "street-view-image-backend.googleapis.com"
  disable_on_destroy = false
}

# キーを Terraform で払い出すには API Keys API 自体の有効化が前提。
resource "google_project_service" "apikeys" {
  service            = "apikeys.googleapis.com"
  disable_on_destroy = false
}

resource "google_apikeys_key" "streetview_metadata" {
  name         = "streetview-metadata-local"
  display_name = "Street View Metadata (local enrich, issue #306)"
  project      = var.project_id

  # Street View Static API 以外には使えないキーにする（漏洩時の課金事故を防ぐ）。
  restrictions {
    api_targets {
      service = "street-view-image-backend.googleapis.com"
    }
  }

  depends_on = [
    google_project_service.streetview,
    google_project_service.apikeys,
  ]
}
