use pretty_assertions::assert_eq;
use tracing::subscriber::with_default;
use tracing_error::ErrorLayer;
use tracing_error::SpanTrace;
use tracing_error::SpanTraceStatus;
use tracing_subscriber::prelude::*;

#[test]
fn tui_tracing_stack_supports_span_traces() {
    let subscriber = tracing_subscriber::registry().with(ErrorLayer::default());

    with_default(subscriber, || {
        let span = tracing::info_span!("span_trace_smoke_test");
        let _guard = span.enter();

        let span_trace = SpanTrace::capture();
        assert_eq!(span_trace.status(), SpanTraceStatus::CAPTURED);
    });
}
