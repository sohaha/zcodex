use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fs::File;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;

use serde::Deserialize;
use tokio::sync::Barrier;
use tokio::time::sleep;

use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;

pub struct TestSyncHandler;

const DEFAULT_TIMEOUT_MS: u64 = 1_000;

static BARRIERS: OnceLock<tokio::sync::Mutex<HashMap<String, BarrierState>>> = OnceLock::new();

struct BarrierState {
    barrier: Arc<Barrier>,
    participants: usize,
}

#[derive(Debug, Deserialize)]
struct BarrierArgs {
    id: String,
    participants: usize,
    #[serde(default = "default_timeout_ms")]
    timeout_ms: u64,
}

#[derive(Debug, Deserialize)]
struct TestSyncArgs {
    #[serde(default)]
    sleep_before_ms: Option<u64>,
    #[serde(default)]
    sleep_after_ms: Option<u64>,
    #[serde(default)]
    touch_path: Option<PathBuf>,
    #[serde(default)]
    wait_for_path: Option<PathBuf>,
    #[serde(default = "default_timeout_ms")]
    wait_timeout_ms: u64,
    #[serde(default)]
    barrier: Option<BarrierArgs>,
}

fn default_timeout_ms() -> u64 {
    DEFAULT_TIMEOUT_MS
}

fn barrier_map() -> &'static tokio::sync::Mutex<HashMap<String, BarrierState>> {
    BARRIERS.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
}

impl ToolHandler for TestSyncHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation { payload, .. } = invocation;

        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "test_sync_tool handler received unsupported payload".to_string(),
                ));
            }
        };

        let args: TestSyncArgs = parse_arguments(&arguments)?;

        if let Some(delay) = args.sleep_before_ms
            && delay > 0
        {
            sleep(Duration::from_millis(delay)).await;
        }

        if let Some(path) = args.touch_path.as_deref() {
            touch_path(path)?;
        }

        if let Some(path) = args.wait_for_path.as_deref() {
            wait_for_path(path, args.wait_timeout_ms).await?;
        }

        if let Some(barrier) = args.barrier {
            wait_on_barrier(barrier).await?;
        }

        if let Some(delay) = args.sleep_after_ms
            && delay > 0
        {
            sleep(Duration::from_millis(delay)).await;
        }

        Ok(FunctionToolOutput::from_text("ok".to_string(), Some(true)))
    }
}

fn touch_path(path: &Path) -> Result<(), FunctionCallError> {
    File::create(path).map_err(|err| {
        FunctionCallError::RespondToModel(format!(
            "test_sync_tool failed to create signal file {}: {err}",
            path.display()
        ))
    })?;
    Ok(())
}

async fn wait_for_path(path: &Path, timeout_ms: u64) -> Result<(), FunctionCallError> {
    if timeout_ms == 0 {
        return Err(FunctionCallError::RespondToModel(
            "test_sync_tool wait timeout must be greater than zero".to_string(),
        ));
    }

    let started = tokio::time::Instant::now();
    let timeout = Duration::from_millis(timeout_ms);
    while !path.exists() {
        if started.elapsed() >= timeout {
            return Err(FunctionCallError::RespondToModel(format!(
                "test_sync_tool timed out waiting for signal file {}",
                path.display()
            )));
        }
        sleep(Duration::from_millis(20)).await;
    }

    Ok(())
}

async fn wait_on_barrier(args: BarrierArgs) -> Result<(), FunctionCallError> {
    if args.participants == 0 {
        return Err(FunctionCallError::RespondToModel(
            "barrier participants must be greater than zero".to_string(),
        ));
    }

    if args.timeout_ms == 0 {
        return Err(FunctionCallError::RespondToModel(
            "barrier timeout must be greater than zero".to_string(),
        ));
    }

    let barrier_id = args.id.clone();
    let barrier = {
        let mut map = barrier_map().lock().await;
        match map.entry(barrier_id.clone()) {
            Entry::Occupied(entry) => {
                let state = entry.get();
                if state.participants != args.participants {
                    let existing = state.participants;
                    return Err(FunctionCallError::RespondToModel(format!(
                        "barrier {barrier_id} already registered with {existing} participants"
                    )));
                }
                state.barrier.clone()
            }
            Entry::Vacant(entry) => {
                let barrier = Arc::new(Barrier::new(args.participants));
                entry.insert(BarrierState {
                    barrier: barrier.clone(),
                    participants: args.participants,
                });
                barrier
            }
        }
    };

    let timeout = Duration::from_millis(args.timeout_ms);
    let wait_result = tokio::time::timeout(timeout, barrier.wait())
        .await
        .map_err(|_| {
            FunctionCallError::RespondToModel("test_sync_tool barrier wait timed out".to_string())
        })?;

    if wait_result.is_leader() {
        let mut map = barrier_map().lock().await;
        if let Some(state) = map.get(&barrier_id)
            && Arc::ptr_eq(&state.barrier, &barrier)
        {
            map.remove(&barrier_id);
        }
    }

    Ok(())
}
