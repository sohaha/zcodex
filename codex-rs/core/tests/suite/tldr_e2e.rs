#![cfg(not(target_os = "windows"))]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use anyhow::Result;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_function_call;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_once;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::test_codex;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;

const TLDR_JSON_BEGIN: &str = "---BEGIN_TLDR_JSON---";
const TLDR_JSON_END: &str = "---END_TLDR_JSON---";

fn extract_tldr_json_block(text: &str) -> Value {
    let (_, json_and_suffix) = text
        .split_once(&format!("\n{TLDR_JSON_BEGIN}\n"))
        .expect("tldr output should include a begin marker on its own line");
    let json = json_and_suffix
        .strip_suffix(&format!("\n{TLDR_JSON_END}"))
        .expect("tldr output should include the closing marker");
    serde_json::from_str(json).expect("tldr json block should parse")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tldr_function_output_exposes_bounded_json_to_model() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex();
    let test = builder.build(&server).await?;

    let src_dir = test.cwd.path().join("src");
    std::fs::create_dir_all(&src_dir)?;
    std::fs::write(
        src_dir.join("main.rs"),
        "fn helper() {}\nfn main() { helper(); }\n",
    )?;

    mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                "call-1",
                "tldr",
                &serde_json::to_string(&json!({
                    "action": "context",
                    "language": "rust",
                    "symbol": "helper"
                }))?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let follow_up = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("inspect helper context").await?;

    let output = follow_up
        .single_request()
        .function_call_output_text("call-1")
        .expect("function tool output should be present");

    assert!(output.contains("analysis kind: call_graph"));
    assert!(output.contains(TLDR_JSON_BEGIN));
    assert!(output.contains(TLDR_JSON_END));

    let payload = extract_tldr_json_block(&output);
    assert_eq!(payload["action"], "context");
    assert_eq!(payload["language"], "rust");
    assert_eq!(payload["symbol"], "helper");
    assert_eq!(payload["analysis"]["kind"], "call_graph");
    assert_eq!(
        payload["analysis"]["details"]["overview"]["incoming_edges"],
        1
    );
    let helper_node = payload["analysis"]["details"]["nodes"]
        .as_array()
        .expect("nodes should be an array")
        .iter()
        .find(|node| node["id"] == "helper")
        .expect("helper node should be present");
    assert_eq!(helper_node["kind"], "function");

    let helper_call_edges = payload["analysis"]["details"]["edges"]
        .as_array()
        .expect("edges should be an array")
        .iter()
        .filter(|edge| edge["kind"] == "calls" && edge["from"] == "main" && edge["to"] == "helper")
        .count();
    assert_eq!(helper_call_edges, 1);

    Ok(())
}
