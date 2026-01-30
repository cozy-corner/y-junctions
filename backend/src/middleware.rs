use axum::{extract::Request, middleware::Next, response::Response};
use tracing::Instrument;

use crate::logging::TraceContext;

/// X-Cloud-Trace-Context ヘッダーを抽出してトレースコンテキストを設定するミドルウェア
pub async fn trace_context_middleware(request: Request, next: Next) -> Response {
    // X-Cloud-Trace-Context ヘッダーを取得
    let trace_context = request
        .headers()
        .get("x-cloud-trace-context")
        .and_then(|value| value.to_str().ok())
        .and_then(TraceContext::from_header);

    // トレースコンテキストに応じてspanを作成
    let span = trace_context
        .as_ref()
        .map(|ctx| {
            let trace_id = ctx.trace_id.clone().unwrap_or_default();
            let span_id = ctx.span_id.clone().unwrap_or_default();

            // trace_sampledの有無に応じてspanを作成
            ctx.trace_sampled.map_or_else(
                || {
                    tracing::info_span!(
                        "http_request",
                        method = %request.method(),
                        uri = %request.uri(),
                        gcp_trace_id = %trace_id,
                        gcp_span_id = %span_id,
                    )
                },
                |sampled| {
                    tracing::info_span!(
                        "http_request",
                        method = %request.method(),
                        uri = %request.uri(),
                        gcp_trace_id = %trace_id,
                        gcp_span_id = %span_id,
                        gcp_trace_sampled = sampled,
                    )
                },
            )
        })
        .unwrap_or_else(|| {
            tracing::info_span!(
                "http_request",
                method = %request.method(),
                uri = %request.uri(),
            )
        });

    // spanの中でリクエストを処理
    next.run(request).instrument(span).await
}
