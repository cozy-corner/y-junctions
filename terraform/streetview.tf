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

  # API 制限で Street View Static API 以外には使えないキーにする（least privilege）。
  # ただしこれはサービス単位の制限で、Street View Static には無料の metadata と
  # 課金対象の画像取得の両方が含まれるため、漏洩時の画像課金までは防げない。
  # 追加の緩和として:
  #  - IP 制限(server_key_restrictions.allowed_ips): enrich はローカルの動的 IP で
  #    走るため不可。
  #  - メソッド制限(api_targets.methods)で metadata 限定: Street View の metadata は
  #    RPC メソッドではなく URL パスの違いで、絞れるメソッド識別子が無いため不可。
  #  - 採用: enrich は metadata(無料)しか叩かないので、Cloud Console で画像リクエストの
  #    日次クォータ上限を最小化し漏洩時の課金を頭打ちにする。詳細: doc/streetview-coverage-filter.md
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
