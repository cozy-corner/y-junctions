# Cloud Run ログ分析計画

## 目的

特定のURLへのアクセス数や、曜日・時間帯別のアクセスパターンを継続的に分析できるようにする。

## 現状の課題

- ログをJSON形式でダウンロードして手動分析する必要がある
- Cloud Loggingは30日でログが期限切れになる
- 大量のログを分析するには非効率
- トレンド分析や長期的なパターン把握が困難

## 提案するアーキテクチャ

```
Cloud Run (y-junctions-api)
    ↓ HTTPリクエスト
Cloud Logging
    ↓ ログシンク（自動エクスポート）
BigQuery (cloud_run_logs dataset)
    ↓ SQLクエリ
分析結果・レポート
```

### 特徴

1. **リアルタイムエクスポート**: ログが自動的にBigQueryに送信される
2. **長期保存**: 90日間保存（設定変更可能）
3. **高速分析**: SQLで柔軟な集計が可能
4. **コスト効率**: 無料枠内で十分運用可能

## Terraform設定

### 1. ファイル構成

新規ファイル: `terraform/logging.tf`

### 2. リソース詳細

#### 2.1 BigQueryデータセット

```hcl
resource "google_bigquery_dataset" "cloud_run_logs" {
  dataset_id    = "cloud_run_logs"
  friendly_name = "Cloud Run Logs"
  description   = "Logs from Cloud Run services for analytics"
  location      = "asia-northeast1"  # Cloud Runと同じリージョン

  # 90日後に自動削除
  default_table_expiration_ms = 90 * 24 * 60 * 60 * 1000
}
```

#### 2.2 ログシンク（Export設定）

```hcl
resource "google_logging_project_sink" "cloud_run_requests" {
  name        = "cloud-run-requests-to-bigquery"
  destination = "bigquery.googleapis.com/projects/y-junctions-prod/datasets/cloud_run_logs"

  # フィルター: Cloud RunのHTTPリクエストログのみ
  filter = <<-EOT
    resource.type="cloud_run_revision"
    resource.labels.service_name="y-junctions-api"
    httpRequest.requestMethod!=""
  EOT

  # パーティション分割テーブル（日付別）
  bigquery_options {
    use_partitioned_tables = true
  }

  unique_writer_identity = true
}
```

**フィルター戦略**:
- ✅ **全HTTPリクエスト**をエクスポート（広めのフィルター）
- ❌ 特定のURL（bboxパラメータ）でフィルターしない
  - URLパラメータは動的で毎回異なる
  - 将来の分析の柔軟性を確保
  - BigQueryクエリ側で詳細フィルター

#### 2.3 IAM権限

```hcl
resource "google_bigquery_dataset_iam_member" "logging_sink_bigquery" {
  dataset_id = google_bigquery_dataset.cloud_run_logs.dataset_id
  role       = "roles/bigquery.dataEditor"
  member     = google_logging_project_sink.cloud_run_requests.writer_identity
}
```

## 分析クエリ例

### 1. 特定URLへのアクセス数

```sql
-- 特定のbboxパターンへのアクセス
SELECT
  COUNT(*) AS access_count,
  DATE(timestamp, 'Asia/Tokyo') AS date_jst
FROM `y-junctions-prod.cloud_run_logs.run_googleapis_com_requests_*`
WHERE
  httpRequest.requestUrl LIKE '%bbox=139.64266777038577%2C35.63058198239574%2C139.8750972747803%2C35.7332061325538%'
  AND DATE(timestamp, 'Asia/Tokyo') >= DATE_SUB(CURRENT_DATE('Asia/Tokyo'), INTERVAL 30 DAY)
GROUP BY date_jst
ORDER BY date_jst DESC
```

### 2. 時間帯別アクセス数（JST）

```sql
SELECT
  EXTRACT(HOUR FROM TIMESTAMP(timestamp, 'Asia/Tokyo')) AS hour_jst,
  COUNT(*) AS request_count,
  AVG(CAST(REGEXP_EXTRACT(httpRequest.latency, r'([\d.]+)') AS FLOAT64)) AS avg_latency_sec,
  MAX(CAST(REGEXP_EXTRACT(httpRequest.latency, r'([\d.]+)') AS FLOAT64)) AS max_latency_sec
FROM `y-junctions-prod.cloud_run_logs.run_googleapis_com_requests_*`
WHERE
  DATE(timestamp, 'Asia/Tokyo') >= DATE_SUB(CURRENT_DATE('Asia/Tokyo'), INTERVAL 30 DAY)
GROUP BY hour_jst
ORDER BY hour_jst
```

### 3. 曜日別アクセス数

```sql
SELECT
  FORMAT_TIMESTAMP('%A', TIMESTAMP(timestamp, 'Asia/Tokyo')) AS day_of_week,
  EXTRACT(DAYOFWEEK FROM TIMESTAMP(timestamp, 'Asia/Tokyo')) AS day_num,
  COUNT(*) AS request_count,
  AVG(CAST(REGEXP_EXTRACT(httpRequest.latency, r'([\d.]+)') AS FLOAT64)) AS avg_latency_sec
FROM `y-junctions-prod.cloud_run_logs.run_googleapis_com_requests_*`
WHERE
  DATE(timestamp, 'Asia/Tokyo') >= DATE_SUB(CURRENT_DATE('Asia/Tokyo'), INTERVAL 30 DAY)
GROUP BY day_of_week, day_num
ORDER BY day_num
```

### 4. 時間帯×曜日のヒートマップデータ

```sql
SELECT
  EXTRACT(DAYOFWEEK FROM TIMESTAMP(timestamp, 'Asia/Tokyo')) AS day_of_week,
  EXTRACT(HOUR FROM TIMESTAMP(timestamp, 'Asia/Tokyo')) AS hour_jst,
  COUNT(*) AS request_count
FROM `y-junctions-prod.cloud_run_logs.run_googleapis_com_requests_*`
WHERE
  DATE(timestamp, 'Asia/Tokyo') >= DATE_SUB(CURRENT_DATE('Asia/Tokyo'), INTERVAL 30 DAY)
GROUP BY day_of_week, hour_jst
ORDER BY day_of_week, hour_jst
```

### 5. コールドスタート分析

```sql
-- レイテンシが1秒以上のリクエスト（コールドスタートの可能性）
SELECT
  timestamp,
  httpRequest.requestUrl,
  httpRequest.latency,
  labels.instanceId
FROM `y-junctions-prod.cloud_run_logs.run_googleapis_com_requests_*`
WHERE
  CAST(REGEXP_EXTRACT(httpRequest.latency, r'([\d.]+)') AS FLOAT64) >= 1.0
  AND DATE(timestamp, 'Asia/Tokyo') >= DATE_SUB(CURRENT_DATE('Asia/Tokyo'), INTERVAL 7 DAY)
ORDER BY timestamp DESC
```

### 6. エンドポイント別アクセス数

```sql
SELECT
  REGEXP_EXTRACT(httpRequest.requestUrl, r'/api/([^?]+)') AS endpoint,
  COUNT(*) AS request_count,
  AVG(CAST(REGEXP_EXTRACT(httpRequest.latency, r'([\d.]+)') AS FLOAT64)) AS avg_latency_sec
FROM `y-junctions-prod.cloud_run_logs.run_googleapis_com_requests_*`
WHERE
  DATE(timestamp, 'Asia/Tokyo') >= DATE_SUB(CURRENT_DATE('Asia/Tokyo'), INTERVAL 30 DAY)
GROUP BY endpoint
ORDER BY request_count DESC
```

## コスト見積もり

### BigQueryの無料枠

- **クエリ**: 1TB/月まで無料
- **ストレージ**: 最初の10GB無料、その後$0.02/GB/月
- **ストリーミングインサート**: 最初の200MBまで無料（ログシンクは対象外）

### このプロジェクトの見積もり

#### ログ量の推定

1リクエスト = 約2KBのログデータ（HTTPリクエスト情報）

| アクセス数/月 | ログサイズ/月 | ストレージコスト/月 |
|-------------|-------------|------------------|
| 10,000      | 20MB        | $0 (無料枠内)     |
| 100,000     | 200MB       | $0 (無料枠内)     |
| 1,000,000   | 2GB         | $0 (無料枠内)     |
| 10,000,000  | 20GB        | $0.20            |

#### クエリコストの推定

- 1クエリあたりのスキャン量: 約1GB（30日分、月100万アクセスの場合）
- 無料枠: 1TB/月まで
- **月1000回クエリを実行しても無料枠内**

### 結論

**現在のアクセス規模では完全に無料枠内で運用可能**

## 実装手順

### 1. Terraform設定の追加

```bash
cd terraform

# 新規ファイル作成
# terraform/logging.tf を作成（上記内容）

# 設定を確認
terraform plan

# 適用
terraform apply
```

### 2. ログシンクの動作確認（数分待機）

```bash
# BigQueryにテーブルが自動作成されているか確認
bq ls --project_id=y-junctions-prod cloud_run_logs

# テーブル名の例: run_googleapis_com_requests_20260131
```

### 3. テストクエリ実行

```bash
# コマンドラインからクエリ実行
bq query --use_legacy_sql=false '
SELECT COUNT(*) as total_requests
FROM `y-junctions-prod.cloud_run_logs.run_googleapis_com_requests_*`
LIMIT 10
'
```

### 4. Cloud Consoleでの確認

1. **BigQuery Console**: https://console.cloud.google.com/bigquery
2. プロジェクト `y-junctions-prod` を選択
3. データセット `cloud_run_logs` を展開
4. テーブルを選択してプレビュー

### 5. 定期レポートの設定（オプション）

BigQuery Scheduled Queriesを使用して、定期的にレポートを生成可能：

```sql
-- 日次アクセスサマリー（毎日9:00 JSTに実行）
CREATE OR REPLACE TABLE `y-junctions-prod.cloud_run_logs.daily_summary`
AS
SELECT
  CURRENT_DATE('Asia/Tokyo') AS report_date,
  COUNT(*) AS total_requests,
  AVG(CAST(REGEXP_EXTRACT(httpRequest.latency, r'([\d.]+)') AS FLOAT64)) AS avg_latency_sec,
  COUNTIF(CAST(REGEXP_EXTRACT(httpRequest.latency, r'([\d.]+)') AS FLOAT64) >= 1.0) AS cold_starts
FROM `y-junctions-prod.cloud_run_logs.run_googleapis_com_requests_*`
WHERE DATE(timestamp, 'Asia/Tokyo') = CURRENT_DATE('Asia/Tokyo') - 1
```

## まとめ

1. **Export時**: 全HTTPリクエストログをBigQueryにエクスポート
2. **分析時**: SQLで特定URLや時間帯をフィルター
3. **コスト**: 無料枠内で十分運用可能
4. **実装**: Terraformで1回設定すれば自動運用

この方式により、継続的かつ柔軟なログ分析が可能になります。
