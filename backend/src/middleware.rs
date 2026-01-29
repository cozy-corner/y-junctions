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

    // トレースコンテキストがある場合はspanに設定
    if let Some(ctx) = trace_context {
        let trace_id = ctx.trace_id.clone().unwrap_or_default();
        let span_id = ctx.span_id.clone().unwrap_or_default();
        let trace_sampled = ctx.trace_sampled.unwrap_or(false);

        let span = tracing::info_span!(
            "http_request",
            method = %request.method(),
            uri = %request.uri(),
            gcp_trace_id = %trace_id,
            gcp_span_id = %span_id,
            gcp_trace_sampled = trace_sampled,
        );

        // spanの中でリクエストを処理
        next.run(request).instrument(span).await
    } else {
        // トレースコンテキストがない場合は通常通り処理
        let span = tracing::info_span!(
            "http_request",
            method = %request.method(),
            uri = %request.uri(),
        );
        next.run(request).instrument(span).await
    }
}
