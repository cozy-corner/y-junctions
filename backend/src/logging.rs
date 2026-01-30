use serde::Serialize;
use std::fmt;
use std::io;
use tracing::span::{Attributes, Id};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

/// Google Cloud Logging用のJSONフォーマッタ
pub struct GoogleCloudLoggingFormatter {
    project_id: String,
}

impl GoogleCloudLoggingFormatter {
    pub fn new(project_id: String) -> Self {
        Self { project_id }
    }
}

#[derive(Serialize)]
struct LogEntry<'a> {
    severity: &'static str,
    message: String,
    #[serde(
        rename = "logging.googleapis.com/trace",
        skip_serializing_if = "Option::is_none"
    )]
    trace: Option<String>,
    #[serde(
        rename = "logging.googleapis.com/spanId",
        skip_serializing_if = "Option::is_none"
    )]
    span_id: Option<String>,
    #[serde(
        rename = "logging.googleapis.com/trace_sampled",
        skip_serializing_if = "Option::is_none"
    )]
    trace_sampled: Option<bool>,
    #[serde(flatten)]
    fields: serde_json::Map<String, serde_json::Value>,
    timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_location: Option<SourceLocation<'a>>,
}

#[derive(Serialize)]
struct SourceLocation<'a> {
    file: &'a str,
    line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    function: Option<&'a str>,
}

impl<S, N> FormatEvent<S, N> for GoogleCloudLoggingFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let metadata = event.metadata();

        // severity（Cloud Loggingの形式）
        let severity = match *metadata.level() {
            Level::TRACE => "DEBUG",
            Level::DEBUG => "DEBUG",
            Level::INFO => "INFO",
            Level::WARN => "WARNING",
            Level::ERROR => "ERROR",
        };

        // タイムスタンプ（RFC3339形式）
        let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        // ソースロケーション
        let source_location = metadata.file().map(|file| SourceLocation {
            file,
            line: metadata.line().unwrap_or(0),
            function: Some(metadata.name()),
        });

        // メッセージとフィールドを収集
        let mut message = String::new();
        let mut fields = serde_json::Map::new();

        let mut visitor = JsonVisitor {
            message: &mut message,
            fields: &mut fields,
        };

        event.record(&mut visitor);

        // spanからトレース情報を取得
        let (trace, span_id, trace_sampled) = if let Some(span) = ctx.lookup_current() {
            let extensions = span.extensions();

            let trace = extensions.get::<TraceContext>().and_then(|tc| {
                tc.trace_id
                    .as_ref()
                    .map(|tid| format!("projects/{}/traces/{}", self.project_id, tid))
            });

            let span_id = extensions
                .get::<TraceContext>()
                .and_then(|tc| tc.span_id.clone());

            let trace_sampled = extensions
                .get::<TraceContext>()
                .and_then(|tc| tc.trace_sampled);

            (trace, span_id, trace_sampled)
        } else {
            (None, None, None)
        };

        let log_entry = LogEntry {
            severity,
            message,
            trace,
            span_id,
            trace_sampled,
            fields,
            timestamp,
            source_location,
        };

        let json = serde_json::to_string(&log_entry).map_err(|_| fmt::Error)?;

        writeln!(writer, "{}", json)
    }
}

/// JSONフィールドを収集するVisitor
struct JsonVisitor<'a> {
    message: &'a mut String,
    fields: &'a mut serde_json::Map<String, serde_json::Value>,
}

impl<'a> tracing::field::Visit for JsonVisitor<'a> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            *self.message = format!("{:?}", value);
        } else {
            self.fields.insert(
                field.name().to_string(),
                serde_json::Value::String(format!("{:?}", value)),
            );
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            *self.message = value.to_string();
        } else {
            self.fields.insert(
                field.name().to_string(),
                serde_json::Value::String(value.to_string()),
            );
        }
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.fields.insert(
            field.name().to_string(),
            serde_json::Value::Number(value.into()),
        );
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.fields.insert(
            field.name().to_string(),
            serde_json::Value::Number(value.into()),
        );
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.fields
            .insert(field.name().to_string(), serde_json::Value::Bool(value));
    }
}

/// トレースコンテキストをspanのextensionsに保存するための型
#[derive(Debug, Clone)]
pub struct TraceContext {
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub trace_sampled: Option<bool>,
}

impl TraceContext {
    /// X-Cloud-Trace-Context ヘッダーからトレースコンテキストをパース
    /// 形式: TRACE_ID/SPAN_ID;o=OPTIONS
    pub fn from_header(header_value: &str) -> Option<Self> {
        let parts: Vec<&str> = header_value.split('/').collect();
        if parts.len() < 2 {
            return None;
        }

        // trace_idのバリデーション: 空文字列を拒否
        let trace_id = parts[0].trim();
        if trace_id.is_empty() {
            return None;
        }
        let trace_id = trace_id.to_string();

        // SPAN_ID;o=OPTIONS の部分をパース
        let span_parts: Vec<&str> = parts[1].split(';').collect();
        let span_id_raw = span_parts[0].trim();

        // span_idを16進数形式に変換（10進数から16進数へ）
        // パースに失敗した場合はNoneを返す（不正な入力を拒否）
        let span_decimal = span_id_raw.parse::<u64>().ok()?;
        let span_id_hex = format!("{:016x}", span_decimal);

        // trace_sampledをパース（o=1 なら true）
        let trace_sampled = span_parts
            .get(1)
            .and_then(|opt| {
                if opt.starts_with("o=") {
                    opt.strip_prefix("o=")
                } else {
                    None
                }
            })
            .map(|val| val == "1");

        Some(Self {
            trace_id: Some(trace_id),
            span_id: Some(span_id_hex),
            trace_sampled,
        })
    }
}

/// spanのフィールドからトレース情報を抽出してextensionsに保存するLayer
pub struct TraceContextExtractorLayer;

impl<S> Layer<S> for TraceContextExtractorLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).expect("Span not found");

        let mut trace_id = None;
        let mut span_id = None;
        let mut trace_sampled = None;

        // spanのフィールドを走査してトレース情報を取得
        attrs.record(
            &mut |field: &tracing::field::Field, value: &dyn fmt::Debug| match field.name() {
                "gcp_trace_id" => {
                    trace_id = Some(format!("{:?}", value).trim_matches('"').to_string());
                }
                "gcp_span_id" => {
                    span_id = Some(format!("{:?}", value).trim_matches('"').to_string());
                }
                "gcp_trace_sampled" => {
                    let value_str = format!("{:?}", value);
                    trace_sampled = value_str.parse().ok();
                }
                _ => {}
            },
        );

        // トレース情報がある場合はextensionsに保存
        if trace_id.is_some() || span_id.is_some() || trace_sampled.is_some() {
            let trace_context = TraceContext {
                trace_id,
                span_id,
                trace_sampled,
            };
            span.extensions_mut().insert(trace_context);
        }
    }
}

/// Google Cloud Loggingフォーマッタを使用したSubscriberを初期化
pub fn init_google_cloud_logging(project_id: String) -> Result<(), Box<dyn std::error::Error>> {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let formatter = GoogleCloudLoggingFormatter::new(project_id);

    let fmt_layer = tracing_subscriber::fmt::layer()
        .event_format(formatter)
        .with_writer(io::stdout);

    let trace_extractor = TraceContextExtractorLayer;

    tracing_subscriber::registry()
        .with(trace_extractor)
        .with(fmt_layer)
        .try_init()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_trace_context() {
        let header = "105445aa7843bc8bf206b12000100000/1;o=1";
        let ctx = TraceContext::from_header(header).unwrap();

        assert_eq!(
            ctx.trace_id,
            Some("105445aa7843bc8bf206b12000100000".to_string())
        );
        assert_eq!(ctx.span_id, Some("0000000000000001".to_string()));
        assert_eq!(ctx.trace_sampled, Some(true));
    }

    #[test]
    fn test_parse_trace_context_not_sampled() {
        let header = "105445aa7843bc8bf206b12000100000/123;o=0";
        let ctx = TraceContext::from_header(header).unwrap();

        assert_eq!(
            ctx.trace_id,
            Some("105445aa7843bc8bf206b12000100000".to_string())
        );
        assert_eq!(ctx.span_id, Some("000000000000007b".to_string()));
        assert_eq!(ctx.trace_sampled, Some(false));
    }

    #[test]
    fn test_parse_trace_context_empty_trace_id() {
        // 空のtrace_idは拒否される
        let header = "/123;o=1";
        let ctx = TraceContext::from_header(header);
        assert!(ctx.is_none());
    }

    #[test]
    fn test_parse_trace_context_invalid_span_id() {
        // 非10進数のspan_idは拒否される
        let header = "abc123/xyz;o=1";
        let ctx = TraceContext::from_header(header);
        assert!(ctx.is_none());
    }

    #[test]
    fn test_parse_trace_context_with_whitespace() {
        // 空白はtrimされる
        let header = " abc123 / 456 ;o=1";
        let ctx = TraceContext::from_header(header).unwrap();

        assert_eq!(ctx.trace_id, Some("abc123".to_string()));
        assert_eq!(ctx.span_id, Some("00000000000001c8".to_string())); // 456 in hex
        assert_eq!(ctx.trace_sampled, Some(true));
    }
}
