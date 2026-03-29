use crate::daemon::TldrDaemonCommand;
use crate::daemon::TldrDaemonResponse;
use anyhow::Result;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::File;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::time::Duration;
use std::time::Instant;
use tokio::time::sleep;

pub type QueryFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Option<TldrDaemonResponse>>> + Send + 'a>>;
pub type LaunchFuture<'a> = Pin<Box<dyn Future<Output = Result<bool>> + Send + 'a>>;

pub struct DaemonLifecycleManager {
    launch_backoff: Duration,
    ready_timeout: Duration,
    ready_poll_interval: Duration,
}

impl Default for DaemonLifecycleManager {
    fn default() -> Self {
        Self {
            launch_backoff: Duration::from_secs(5),
            ready_timeout: Duration::from_secs(5),
            ready_poll_interval: Duration::from_millis(25),
        }
    }
}

impl DaemonLifecycleManager {
    pub fn new(launch_backoff: Duration) -> Self {
        Self {
            launch_backoff,
            ..Self::default()
        }
    }

    pub async fn query_or_spawn_with_hooks<Q, E>(
        &self,
        project_root: &Path,
        command: &TldrDaemonCommand,
        query: Q,
        ensure_running: E,
    ) -> Result<Option<TldrDaemonResponse>>
    where
        Q: for<'a> Fn(&'a Path, &'a TldrDaemonCommand) -> QueryFuture<'a>,
        E: for<'a> Fn(&'a Path) -> LaunchFuture<'a>,
    {
        let mut daemon_response = query(project_root, command).await?;
        if daemon_response.is_none() && ensure_running(project_root).await? {
            daemon_response = query(project_root, command).await?;
        }
        Ok(daemon_response)
    }

    pub async fn ensure_running<L, A, C>(
        &self,
        project_root: &Path,
        is_alive: A,
        cleanup: C,
        launch: L,
    ) -> Result<bool>
    where
        L: for<'a> Fn(&'a Path) -> LaunchFuture<'a>,
        A: Fn(&Path) -> bool,
        C: Fn(&Path),
    {
        if is_alive(project_root) {
            return Ok(true);
        }

        let key = project_key(project_root);
        if wait_for_existing_launch(&key).await {
            return Ok(self.wait_until_alive(project_root, &is_alive).await);
        }
        if self.should_backoff(&key) {
            return Ok(false);
        }

        let _tracker = LaunchTracker::new(key.clone());
        if is_alive(project_root) {
            self.clear_backoff(&key);
            return Ok(true);
        }

        cleanup(project_root);
        if is_alive(project_root) {
            self.clear_backoff(&key);
            return Ok(true);
        }

        if launch(project_root).await? {
            self.clear_backoff(&key);
            return Ok(self.wait_until_alive(project_root, &is_alive).await);
        }
        self.record_launch_failure(&key);
        Ok(false)
    }

    pub async fn ensure_running_with_launcher_lock<L, A, C, D, O, W>(
        &self,
        project_root: &Path,
        is_alive: A,
        cleanup: C,
        daemon_lock_is_held: D,
        try_open_launcher_lock: O,
        on_launcher_wait: W,
        launch: L,
    ) -> Result<bool>
    where
        L: for<'a> Fn(&'a Path) -> LaunchFuture<'a>,
        A: Fn(&Path, bool) -> bool,
        C: Fn(&Path),
        D: Fn(&Path) -> Result<bool>,
        O: Fn(&Path) -> Result<Option<File>>,
        W: Fn(&Path),
    {
        if is_alive(project_root, false) {
            return Ok(true);
        }

        let key = project_key(project_root);
        if daemon_lock_is_held(project_root)? {
            return Ok(self
                .wait_until_alive_with_launcher_lock(project_root, &is_alive, true)
                .await);
        }
        if wait_for_existing_launch(&key).await {
            return Ok(self
                .wait_until_alive_with_launcher_lock(project_root, &is_alive, false)
                .await);
        }
        if self.should_backoff(&key) {
            return Ok(false);
        }

        let _tracker = LaunchTracker::new(key.clone());
        if is_alive(project_root, false) {
            self.clear_backoff(&key);
            return Ok(true);
        }

        let Some(_launcher_lock) = try_open_launcher_lock(project_root)? else {
            on_launcher_wait(project_root);
            return Ok(self
                .wait_until_alive_with_launcher_lock(project_root, &is_alive, false)
                .await);
        };

        if is_alive(project_root, true) {
            self.clear_backoff(&key);
            return Ok(true);
        }

        cleanup(project_root);
        if is_alive(project_root, true) {
            self.clear_backoff(&key);
            return Ok(true);
        }
        if daemon_lock_is_held(project_root)? {
            self.clear_backoff(&key);
            return Ok(self
                .wait_until_alive_with_launcher_lock(project_root, &is_alive, true)
                .await);
        }

        if launch(project_root).await? {
            self.clear_backoff(&key);
            return Ok(self
                .wait_until_alive_with_launcher_lock(project_root, &is_alive, true)
                .await);
        }
        self.record_launch_failure(&key);
        Ok(false)
    }

    async fn wait_until_alive<A>(&self, project_root: &Path, is_alive: &A) -> bool
    where
        A: Fn(&Path) -> bool,
    {
        let deadline = Instant::now() + self.ready_timeout;
        while Instant::now() < deadline {
            if is_alive(project_root) {
                return true;
            }
            sleep(self.ready_poll_interval).await;
        }
        is_alive(project_root)
    }

    async fn wait_until_alive_with_launcher_lock<A>(
        &self,
        project_root: &Path,
        is_alive: &A,
        ignore_launcher_lock: bool,
    ) -> bool
    where
        A: Fn(&Path, bool) -> bool,
    {
        let deadline = Instant::now() + self.ready_timeout;
        while Instant::now() < deadline {
            if is_alive(project_root, ignore_launcher_lock) {
                return true;
            }
            sleep(self.ready_poll_interval).await;
        }
        is_alive(project_root, ignore_launcher_lock)
    }

    fn should_backoff(&self, key: &str) -> bool {
        lock_map(&LAUNCH_FAILURES)
            .get(key)
            .map(|instant: &Instant| instant.elapsed() < self.launch_backoff)
            .unwrap_or(false)
    }

    fn clear_backoff(&self, key: &str) {
        lock_map(&LAUNCH_FAILURES).remove(key);
    }

    fn record_launch_failure(&self, key: &str) {
        lock_map(&LAUNCH_FAILURES).insert(key.to_string(), Instant::now());
    }
}

async fn wait_for_existing_launch(key: &str) -> bool {
    if !lock_map(&LAUNCHING_PROJECTS).contains(key) {
        return false;
    }
    loop {
        if !lock_map(&LAUNCHING_PROJECTS).contains(key) {
            return true;
        }
        sleep(Duration::from_millis(25)).await;
    }
}

fn project_key(project_root: &Path) -> String {
    project_root.to_string_lossy().to_string()
}

static LAUNCH_FAILURES: Lazy<Mutex<HashMap<String, Instant>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static LAUNCHING_PROJECTS: Lazy<Mutex<HashSet<String>>> = Lazy::new(|| Mutex::new(HashSet::new()));

struct LaunchTracker {
    key: String,
}

impl LaunchTracker {
    fn new(key: String) -> Self {
        lock_map(&LAUNCHING_PROJECTS).insert(key.clone());
        Self { key }
    }
}

impl Drop for LaunchTracker {
    fn drop(&mut self) {
        lock_map(&LAUNCHING_PROJECTS).remove(&self.key);
    }
}

fn lock_map<'a, T>(mutex: &'a Mutex<T>) -> MutexGuard<'a, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(test)]
mod tests {
    use super::DaemonLifecycleManager;
    use crate::daemon::TldrDaemonCommand;
    use crate::daemon::TldrDaemonResponse;
    use pretty_assertions::assert_eq;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::time::Duration;
    use tempfile::tempdir;
    use tokio::sync::Notify;
    use tokio::time::sleep;

    #[tokio::test]
    async fn query_or_spawn_with_hooks_retries_after_launch() {
        let tempdir = tempdir().expect("tempdir should exist");
        let manager = DaemonLifecycleManager::default();
        let command = TldrDaemonCommand::Ping;
        let query_calls = Arc::new(AtomicUsize::new(0));
        let ensure_calls = Arc::new(AtomicUsize::new(0));
        let query_response = TldrDaemonResponse {
            status: "ok".to_string(),
            message: "pong".to_string(),
            analysis: None,
            imports: None,
            importers: None,
            search: None,
            diagnostics: None,
            semantic: None,
            snapshot: None,
            daemon_status: None,
            reindex_report: None,
        };

        let response = manager
            .query_or_spawn_with_hooks(
                tempdir.path(),
                &command,
                {
                    let query_calls = Arc::clone(&query_calls);
                    let query_response = query_response.clone();
                    move |_project_root, _command| {
                        let query_calls = Arc::clone(&query_calls);
                        let query_response = query_response.clone();
                        Box::pin(async move {
                            let call_index = query_calls.fetch_add(1, Ordering::SeqCst);
                            Ok(if call_index == 0 {
                                None
                            } else {
                                Some(query_response)
                            })
                        })
                    }
                },
                {
                    let ensure_calls = Arc::clone(&ensure_calls);
                    move |_project_root| {
                        let ensure_calls = Arc::clone(&ensure_calls);
                        Box::pin(async move {
                            ensure_calls.fetch_add(1, Ordering::SeqCst);
                            Ok(true)
                        })
                    }
                },
            )
            .await
            .expect("query_or_spawn_with_hooks should succeed");

        assert_eq!(response, Some(query_response));
        assert_eq!(query_calls.load(Ordering::SeqCst), 2);
        assert_eq!(ensure_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn query_or_spawn_with_hooks_skips_retry_when_launch_fails() {
        let tempdir = tempdir().expect("tempdir should exist");
        let manager = DaemonLifecycleManager::default();
        let command = TldrDaemonCommand::Ping;
        let query_calls = Arc::new(AtomicUsize::new(0));
        let ensure_calls = Arc::new(AtomicUsize::new(0));

        let response = manager
            .query_or_spawn_with_hooks(
                tempdir.path(),
                &command,
                {
                    let query_calls = Arc::clone(&query_calls);
                    move |_project_root, _command| {
                        let query_calls = Arc::clone(&query_calls);
                        Box::pin(async move {
                            query_calls.fetch_add(1, Ordering::SeqCst);
                            Ok(None)
                        })
                    }
                },
                {
                    let ensure_calls = Arc::clone(&ensure_calls);
                    move |_project_root| {
                        let ensure_calls = Arc::clone(&ensure_calls);
                        Box::pin(async move {
                            ensure_calls.fetch_add(1, Ordering::SeqCst);
                            Ok(false)
                        })
                    }
                },
            )
            .await
            .expect("query_or_spawn_with_hooks should succeed");

        assert_eq!(response, None);
        assert_eq!(query_calls.load(Ordering::SeqCst), 1);
        assert_eq!(ensure_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ensure_running_serializes_concurrent_launches() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().to_path_buf();
        let manager = Arc::new(DaemonLifecycleManager::default());
        let launch_count = Arc::new(AtomicUsize::new(0));
        let alive_flag = Arc::new(AtomicBool::new(false));
        let launch_notify = Arc::new(Notify::new());

        let manager_clone = Arc::clone(&manager);
        let project_clone = project_root.clone();
        let launch_count_clone = Arc::clone(&launch_count);
        let alive_clone = Arc::clone(&alive_flag);
        let alive_for_launch = Arc::clone(&alive_flag);
        let notify_clone = Arc::clone(&launch_notify);
        let first = tokio::spawn(async move {
            manager_clone
                .ensure_running(
                    &project_clone,
                    move |_path| alive_clone.load(Ordering::SeqCst),
                    |_path| {},
                    move |_path| {
                        let launch_count = Arc::clone(&launch_count_clone);
                        let alive = Arc::clone(&alive_for_launch);
                        let notify = Arc::clone(&notify_clone);
                        Box::pin(async move {
                            launch_count.fetch_add(1, Ordering::SeqCst);
                            notify.notify_waiters();
                            sleep(Duration::from_millis(200)).await;
                            alive.store(true, Ordering::SeqCst);
                            Ok(true)
                        })
                    },
                )
                .await
        });

        launch_notify.notified().await;

        let manager_clone = Arc::clone(&manager);
        let project_clone = project_root.clone();
        let alive_clone = Arc::clone(&alive_flag);
        let second = tokio::spawn(async move {
            manager_clone
                .ensure_running(
                    &project_clone,
                    move |_path| alive_clone.load(Ordering::SeqCst),
                    |_path| {},
                    |_path| {
                        Box::pin(async move {
                            panic!("second ensure_running should not launch a daemon");
                        })
                    },
                )
                .await
        });

        first
            .await
            .unwrap()
            .expect("first ensure_running should succeed");
        second
            .await
            .unwrap()
            .expect("second ensure_running should succeed");
        assert_eq!(launch_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn ensure_running_only_launches_once_per_project_in_process() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().to_path_buf();
        let manager = Arc::new(DaemonLifecycleManager::new(Duration::from_millis(5)));
        let alive = Arc::new(AtomicBool::new(false));
        let cleanup_calls = Arc::new(AtomicUsize::new(0));
        let launch_calls = Arc::new(AtomicUsize::new(0));

        let task1 = tokio::spawn({
            let manager = Arc::clone(&manager);
            let alive = Arc::clone(&alive);
            let cleanup_calls = Arc::clone(&cleanup_calls);
            let launch_calls = Arc::clone(&launch_calls);
            let project_root = project_root.clone();
            async move {
                manager
                    .ensure_running(
                        &project_root,
                        {
                            let alive = Arc::clone(&alive);
                            move |_| alive.load(Ordering::SeqCst)
                        },
                        {
                            let cleanup_calls = Arc::clone(&cleanup_calls);
                            move |_| {
                                cleanup_calls.fetch_add(1, Ordering::SeqCst);
                            }
                        },
                        {
                            let alive = Arc::clone(&alive);
                            let launch_calls = Arc::clone(&launch_calls);
                            move |_| {
                                let alive = Arc::clone(&alive);
                                let launch_calls = Arc::clone(&launch_calls);
                                Box::pin(async move {
                                    launch_calls.fetch_add(1, Ordering::SeqCst);
                                    tokio::time::sleep(Duration::from_millis(50)).await;
                                    alive.store(true, Ordering::SeqCst);
                                    Ok(true)
                                })
                            }
                        },
                    )
                    .await
                    .expect("first ensure_running should succeed")
            }
        });
        let task2 = tokio::spawn({
            let manager = Arc::clone(&manager);
            let alive = Arc::clone(&alive);
            let cleanup_calls = Arc::clone(&cleanup_calls);
            let launch_calls = Arc::clone(&launch_calls);
            let project_root = project_root.clone();
            async move {
                manager
                    .ensure_running(
                        &project_root,
                        {
                            let alive = Arc::clone(&alive);
                            move |_| alive.load(Ordering::SeqCst)
                        },
                        {
                            let cleanup_calls = Arc::clone(&cleanup_calls);
                            move |_| {
                                cleanup_calls.fetch_add(1, Ordering::SeqCst);
                            }
                        },
                        {
                            let alive = Arc::clone(&alive);
                            let launch_calls = Arc::clone(&launch_calls);
                            move |_| {
                                let alive = Arc::clone(&alive);
                                let launch_calls = Arc::clone(&launch_calls);
                                Box::pin(async move {
                                    launch_calls.fetch_add(1, Ordering::SeqCst);
                                    alive.store(true, Ordering::SeqCst);
                                    Ok(true)
                                })
                            }
                        },
                    )
                    .await
                    .expect("second ensure_running should succeed")
            }
        });

        let (first, second) = tokio::join!(task1, task2);
        assert!(first.expect("first task should join"));
        assert!(second.expect("second task should join"));
        assert_eq!(cleanup_calls.load(Ordering::SeqCst), 1);
        assert_eq!(launch_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn ensure_running_waits_for_daemon_readiness_after_launch() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().to_path_buf();
        let manager = DaemonLifecycleManager::new(Duration::from_millis(5));
        let alive = Arc::new(AtomicBool::new(false));

        let started = manager
            .ensure_running(
                &project_root,
                {
                    let alive = Arc::clone(&alive);
                    move |_| alive.load(Ordering::SeqCst)
                },
                |_| {},
                {
                    let alive = Arc::clone(&alive);
                    move |_| {
                        let alive = Arc::clone(&alive);
                        Box::pin(async move {
                            let _ready_task = tokio::spawn(async move {
                                sleep(Duration::from_millis(50)).await;
                                alive.store(true, Ordering::SeqCst);
                            });
                            Ok(true)
                        })
                    }
                },
            )
            .await
            .expect("ensure_running should succeed");

        assert!(started);
        assert!(alive.load(Ordering::SeqCst));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ensure_running_waits_for_existing_launch_to_turn_alive() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().to_path_buf();
        let manager = Arc::new(DaemonLifecycleManager::new(Duration::from_millis(5)));
        let alive = Arc::new(AtomicBool::new(false));
        let launch_count = Arc::new(AtomicUsize::new(0));
        let launch_started = Arc::new(Notify::new());

        let first = tokio::spawn({
            let manager = Arc::clone(&manager);
            let project_root = project_root.clone();
            let alive = Arc::clone(&alive);
            let launch_count = Arc::clone(&launch_count);
            let launch_started = Arc::clone(&launch_started);
            async move {
                manager
                    .ensure_running(
                        &project_root,
                        {
                            let alive = Arc::clone(&alive);
                            move |_| alive.load(Ordering::SeqCst)
                        },
                        |_| {},
                        move |_| {
                            let alive = Arc::clone(&alive);
                            let launch_count = Arc::clone(&launch_count);
                            let launch_started = Arc::clone(&launch_started);
                            Box::pin(async move {
                                launch_count.fetch_add(1, Ordering::SeqCst);
                                launch_started.notify_waiters();
                                let _ready_task = tokio::spawn(async move {
                                    sleep(Duration::from_millis(50)).await;
                                    alive.store(true, Ordering::SeqCst);
                                });
                                Ok(true)
                            })
                        },
                    )
                    .await
            }
        });

        launch_started.notified().await;

        let second = tokio::spawn({
            let manager = Arc::clone(&manager);
            let project_root = project_root.clone();
            let alive = Arc::clone(&alive);
            async move {
                manager
                    .ensure_running(
                        &project_root,
                        {
                            let alive = Arc::clone(&alive);
                            move |_| alive.load(Ordering::SeqCst)
                        },
                        |_| {},
                        |_| {
                            Box::pin(async move {
                                panic!("second ensure_running should wait instead of launching");
                            })
                        },
                    )
                    .await
            }
        });

        assert!(
            first
                .await
                .unwrap()
                .expect("first ensure_running should succeed")
        );
        assert!(
            second
                .await
                .unwrap()
                .expect("second ensure_running should succeed")
        );
        assert_eq!(launch_count.load(Ordering::SeqCst), 1);
        assert!(alive.load(Ordering::SeqCst));
    }
}
