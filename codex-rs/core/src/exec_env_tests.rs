use super::*;
use codex_config::types::ShellEnvironmentPolicyInherit;
use maplit::hashmap;
use tempfile::tempdir;

fn make_vars(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn test_core_inherit_defaults_keep_sensitive_vars() {
    let vars = make_vars(&[
        ("PATH", "/usr/bin"),
        ("HOME", "/home/user"),
        ("API_KEY", "secret"),
        ("SECRET_TOKEN", "t"),
    ]);

    let policy = ShellEnvironmentPolicy::default(); // inherit All, default excludes ignored
    let thread_id = ThreadId::new();
    let result = populate_env(vars, &policy, Some(thread_id));

    let mut expected: HashMap<String, String> = hashmap! {
        "PATH".to_string() => "/usr/bin".to_string(),
        "HOME".to_string() => "/home/user".to_string(),
        "API_KEY".to_string() => "secret".to_string(),
        "SECRET_TOKEN".to_string() => "t".to_string(),
    };
    expected.insert(CODEX_THREAD_ID_ENV_VAR.to_string(), thread_id.to_string());

    assert_eq!(result, expected);
}

#[test]
fn test_core_inherit_with_default_excludes_enabled() {
    let vars = make_vars(&[
        ("PATH", "/usr/bin"),
        ("HOME", "/home/user"),
        ("API_KEY", "secret"),
        ("SECRET_TOKEN", "t"),
    ]);

    let policy = ShellEnvironmentPolicy {
        ignore_default_excludes: false, // apply KEY/SECRET/TOKEN filter
        ..Default::default()
    };
    let thread_id = ThreadId::new();
    let result = populate_env(vars, &policy, Some(thread_id));

    let mut expected: HashMap<String, String> = hashmap! {
        "PATH".to_string() => "/usr/bin".to_string(),
        "HOME".to_string() => "/home/user".to_string(),
    };
    expected.insert(CODEX_THREAD_ID_ENV_VAR.to_string(), thread_id.to_string());

    assert_eq!(result, expected);
}

#[test]
fn test_include_only() {
    let vars = make_vars(&[("PATH", "/usr/bin"), ("FOO", "bar")]);

    let policy = ShellEnvironmentPolicy {
        // skip default excludes so nothing is removed prematurely
        ignore_default_excludes: true,
        include_only: vec![EnvironmentVariablePattern::new_case_insensitive("*PATH")],
        ..Default::default()
    };

    let thread_id = ThreadId::new();
    let result = populate_env(vars, &policy, Some(thread_id));

    let mut expected: HashMap<String, String> = hashmap! {
        "PATH".to_string() => "/usr/bin".to_string(),
    };
    expected.insert(CODEX_THREAD_ID_ENV_VAR.to_string(), thread_id.to_string());

    assert_eq!(result, expected);
}

#[test]
fn test_set_overrides() {
    let vars = make_vars(&[("PATH", "/usr/bin")]);

    let mut policy = ShellEnvironmentPolicy {
        ignore_default_excludes: true,
        ..Default::default()
    };
    policy.r#set.insert("NEW_VAR".to_string(), "42".to_string());

    let thread_id = ThreadId::new();
    let result = populate_env(vars, &policy, Some(thread_id));

    let mut expected: HashMap<String, String> = hashmap! {
        "PATH".to_string() => "/usr/bin".to_string(),
        "NEW_VAR".to_string() => "42".to_string(),
    };
    expected.insert(CODEX_THREAD_ID_ENV_VAR.to_string(), thread_id.to_string());

    assert_eq!(result, expected);
}

#[test]
fn populate_env_inserts_thread_id() {
    let vars = make_vars(&[("PATH", "/usr/bin")]);
    let policy = ShellEnvironmentPolicy::default();
    let thread_id = ThreadId::new();
    let result = populate_env(vars, &policy, Some(thread_id));

    let mut expected: HashMap<String, String> = hashmap! {
        "PATH".to_string() => "/usr/bin".to_string(),
    };
    expected.insert(CODEX_THREAD_ID_ENV_VAR.to_string(), thread_id.to_string());

    assert_eq!(result, expected);
}

#[test]
fn populate_env_omits_thread_id_when_missing() {
    let vars = make_vars(&[("PATH", "/usr/bin")]);
    let policy = ShellEnvironmentPolicy::default();
    let result = populate_env(vars, &policy, /*thread_id*/ None);

    let expected: HashMap<String, String> = hashmap! {
        "PATH".to_string() => "/usr/bin".to_string(),
    };

    assert_eq!(result, expected);
}

#[test]
fn test_inherit_all() {
    let vars = make_vars(&[("PATH", "/usr/bin"), ("FOO", "bar")]);

    let policy = ShellEnvironmentPolicy {
        inherit: ShellEnvironmentPolicyInherit::All,
        ignore_default_excludes: true, // keep everything
        ..Default::default()
    };

    let thread_id = ThreadId::new();
    let result = populate_env(vars.clone(), &policy, Some(thread_id));
    let mut expected: HashMap<String, String> = vars.into_iter().collect();
    expected.insert(CODEX_THREAD_ID_ENV_VAR.to_string(), thread_id.to_string());
    assert_eq!(result, expected);
}

#[test]
fn test_inherit_all_with_default_excludes() {
    let vars = make_vars(&[("PATH", "/usr/bin"), ("API_KEY", "secret")]);

    let policy = ShellEnvironmentPolicy {
        inherit: ShellEnvironmentPolicyInherit::All,
        ignore_default_excludes: false,
        ..Default::default()
    };

    let thread_id = ThreadId::new();
    let result = populate_env(vars, &policy, Some(thread_id));
    let mut expected: HashMap<String, String> = hashmap! {
        "PATH".to_string() => "/usr/bin".to_string(),
    };
    expected.insert(CODEX_THREAD_ID_ENV_VAR.to_string(), thread_id.to_string());
    assert_eq!(result, expected);
}

#[test]
#[cfg(target_os = "windows")]
fn test_core_inherit_respects_case_insensitive_names_on_windows() {
    let vars = make_vars(&[
        ("Path", "C:\\Windows\\System32"),
        ("TEMP", "C:\\Temp"),
        ("FOO", "bar"),
    ]);

    let policy = ShellEnvironmentPolicy {
        inherit: ShellEnvironmentPolicyInherit::Core,
        ignore_default_excludes: true,
        ..Default::default()
    };

    let thread_id = ThreadId::new();
    let result = populate_env(vars, &policy, Some(thread_id));
    let mut expected: HashMap<String, String> = hashmap! {
        "Path".to_string() => "C:\\Windows\\System32".to_string(),
        "TEMP".to_string() => "C:\\Temp".to_string(),
    };
    expected.insert(CODEX_THREAD_ID_ENV_VAR.to_string(), thread_id.to_string());

    assert_eq!(result, expected);
}

#[test]
fn test_inherit_none() {
    let vars = make_vars(&[("PATH", "/usr/bin"), ("HOME", "/home")]);

    let mut policy = ShellEnvironmentPolicy {
        inherit: ShellEnvironmentPolicyInherit::None,
        ignore_default_excludes: true,
        ..Default::default()
    };
    policy
        .r#set
        .insert("ONLY_VAR".to_string(), "yes".to_string());

    let thread_id = ThreadId::new();
    let result = populate_env(vars, &policy, Some(thread_id));
    let mut expected: HashMap<String, String> = hashmap! {
        "ONLY_VAR".to_string() => "yes".to_string(),
    };
    expected.insert(CODEX_THREAD_ID_ENV_VAR.to_string(), thread_id.to_string());
    assert_eq!(result, expected);
}

#[test]
fn prepend_arg0_helper_dir_to_path_prepends_helper_dir() {
    let mut env = hashmap! {
        "PATH".to_string() => "/usr/local/bin:/usr/bin".to_string(),
    };

    prepend_arg0_helper_dir_to_path(
        &mut env,
        Some(std::path::Path::new("/tmp/codex-arg0/codex-execve-wrapper")),
        None,
    );

    assert_eq!(
        env.get("PATH"),
        Some(&"/tmp/codex-arg0:/usr/local/bin:/usr/bin".to_string())
    );
}

#[test]
fn prepend_arg0_helper_dir_to_path_skips_duplicate_entries() {
    let mut env = hashmap! {
        "PATH".to_string() => "/tmp/codex-arg0:/usr/local/bin:/usr/bin".to_string(),
    };

    prepend_arg0_helper_dir_to_path(
        &mut env,
        Some(std::path::Path::new("/tmp/codex-arg0/codex-execve-wrapper")),
        None,
    );

    assert_eq!(
        env.get("PATH"),
        Some(&"/tmp/codex-arg0:/usr/local/bin:/usr/bin".to_string())
    );
}

#[test]
fn explicit_snapshot_env_overrides_preserve_helper_path_and_dependency_env() {
    let shell_env_overrides = hashmap! {
        "FOO".to_string() => "bar".to_string(),
    };
    let dependency_env = hashmap! {
        "OPENAI_API_KEY".to_string() => "secret".to_string(),
    };
    let exec_env = hashmap! {
        "PATH".to_string() => "/tmp/codex-arg0-helper:/usr/local/bin:/usr/bin".to_string(),
        "OPENAI_API_KEY".to_string() => "secret".to_string(),
    };

    let overrides =
        explicit_snapshot_env_overrides(&shell_env_overrides, &dependency_env, &exec_env);

    assert_eq!(
        overrides.get("PATH"),
        Some(&"/tmp/codex-arg0-helper:/usr/local/bin:/usr/bin".to_string())
    );
    assert_eq!(overrides.get("FOO"), Some(&"bar".to_string()));
    assert_eq!(overrides.get("OPENAI_API_KEY"), Some(&"secret".to_string()));
}

#[test]
fn populate_env_removes_invalid_ripgrep_config_path() {
    let vars = make_vars(&[
        ("PATH", "/usr/bin"),
        ("RIPGREP_CONFIG_PATH", "/tmp/definitely-missing-ripgreprc"),
    ]);
    let policy = ShellEnvironmentPolicy {
        ignore_default_excludes: true,
        ..Default::default()
    };

    let result = populate_env(vars, &policy, None);

    assert_eq!(result.get("PATH"), Some(&"/usr/bin".to_string()));
    assert!(!result.contains_key("RIPGREP_CONFIG_PATH"));
}

#[test]
fn populate_env_keeps_valid_ripgrep_config_path() {
    let dir = tempdir().expect("create temp dir");
    let config_path = dir.path().join("ripgreprc");
    std::fs::write(&config_path, "--hidden\n").expect("write ripgreprc");
    let vars = vec![
        ("PATH".to_string(), "/usr/bin".to_string()),
        (
            "RIPGREP_CONFIG_PATH".to_string(),
            config_path.display().to_string(),
        ),
    ];
    let policy = ShellEnvironmentPolicy {
        ignore_default_excludes: true,
        ..Default::default()
    };

    let result = populate_env(vars, &policy, None);

    assert_eq!(
        result.get("RIPGREP_CONFIG_PATH"),
        Some(&config_path.display().to_string())
    );
}
