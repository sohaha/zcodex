use super::ContextualUserFragment;

const ZCONTEXT_SNAPSHOT_OPEN_TAG: &str = "<zcontext_snapshot>";
const ZCONTEXT_SNAPSHOT_CLOSE_TAG: &str = "</zcontext_snapshot>";

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ZcontextSnapshotContext {
    text: String,
}

impl ZcontextSnapshotContext {
    pub(crate) fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

impl ContextualUserFragment for ZcontextSnapshotContext {
    const ROLE: &'static str = "developer";
    const START_MARKER: &'static str = ZCONTEXT_SNAPSHOT_OPEN_TAG;
    const END_MARKER: &'static str = ZCONTEXT_SNAPSHOT_CLOSE_TAG;

    fn body(&self) -> String {
        self.text.clone()
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn extract_text(content: &[codex_protocol::models::ContentItem]) -> String {
        content
            .iter()
            .filter_map(|c| match c {
                codex_protocol::models::ContentItem::InputText { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn snapshot_renders_with_markers() {
        let ctx = ZcontextSnapshotContext::new("test snapshot content");
        let rendered = ctx.render();
        assert_eq!(
            rendered,
            "<zcontext_snapshot>test snapshot content</zcontext_snapshot>"
        );
    }

    #[test]
    fn snapshot_into_response_item() {
        let ctx = ZcontextSnapshotContext::new("snapshot text");
        let item = ContextualUserFragment::into(ctx);
        match item {
            codex_protocol::models::ResponseItem::Message {
                role, content, id, ..
            } => {
                assert_eq!(role, "developer");
                assert!(id.is_none());
                let text = extract_text(&content);
                assert_eq!(text, "<zcontext_snapshot>snapshot text</zcontext_snapshot>");
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn snapshot_matches_text_with_markers() {
        assert!(ZcontextSnapshotContext::matches_text(
            "<zcontext_snapshot>any content</zcontext_snapshot>"
        ));
        assert!(ZcontextSnapshotContext::matches_text(
            "  <zcontext_snapshot>content</zcontext_snapshot>  "
        ));
        assert!(!ZcontextSnapshotContext::matches_text("no markers here"));
        assert!(!ZcontextSnapshotContext::matches_text(""));
    }

    #[test]
    fn empty_snapshot_renders_markers_only() {
        let ctx = ZcontextSnapshotContext::new("");
        assert_eq!(ctx.render(), "<zcontext_snapshot></zcontext_snapshot>");
    }
}
