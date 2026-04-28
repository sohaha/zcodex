use crate::api::OnnxRuntimeStatus;
use anyhow::Context;
use anyhow::Result;
#[cfg(not(test))]
use fastembed::EmbeddingModel;
#[cfg(not(test))]
use fastembed::InitOptions;
#[cfg(not(test))]
use fastembed::TextEmbedding;
#[cfg(not(test))]
use std::collections::BTreeMap;
#[cfg(unix)]
use std::ffi::CStr;
#[cfg(unix)]
use std::ffi::CString;
use std::ffi::OsString;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::path::PathBuf;
#[cfg(not(test))]
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EmbeddingInputKind {
    Query,
    Document,
}

#[derive(Debug, Clone)]
pub(crate) struct SemanticEmbedder {
    #[allow(dead_code)]
    model: String,
}

impl SemanticEmbedder {
    pub(crate) fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
        }
    }

    pub(crate) fn embed_query(&self, query: &str, output_dims: usize) -> Result<Vec<f32>> {
        let mut vectors =
            self.embed_many(&[query.to_string()], EmbeddingInputKind::Query, output_dims)?;
        vectors
            .pop()
            .context("semantic embedder returned no query vector")
    }

    pub(crate) fn embed_documents(
        &self,
        documents: &[String],
        output_dims: usize,
    ) -> Result<Vec<Vec<f32>>> {
        self.embed_many(documents, EmbeddingInputKind::Document, output_dims)
    }

    fn embed_many(
        &self,
        inputs: &[String],
        kind: EmbeddingInputKind,
        output_dims: usize,
    ) -> Result<Vec<Vec<f32>>> {
        #[cfg(test)]
        {
            maybe_fail_test_embedding()?;
            Ok(inputs
                .iter()
                .map(|input| fake_embed(input, kind, output_dims))
                .collect())
        }

        #[cfg(not(test))]
        {
            embed_with_fastembed(&self.model, inputs, kind, output_dims)
        }
    }
}

const ORT_BACKEND_UNAVAILABLE_MARKER: &str =
    "semantic embedding backend requires ONNX Runtime dylib";

pub(crate) fn is_embedding_backend_unavailable(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| cause.to_string().contains(ORT_BACKEND_UNAVAILABLE_MARKER))
}

#[cfg(not(test))]
struct FastembedHandle {
    inner: Mutex<TextEmbedding>,
}

#[cfg(not(test))]
fn embed_with_fastembed(
    model: &str,
    inputs: &[String],
    kind: EmbeddingInputKind,
    output_dims: usize,
) -> Result<Vec<Vec<f32>>> {
    let cache = FASTEMBED_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let handle = {
        let mut guard = cache
            .lock()
            .map_err(|_| anyhow::anyhow!("semantic embedder cache lock poisoned"))?;
        if let Some(handle) = guard.get(model) {
            Arc::clone(handle)
        } else {
            ensure_onnxruntime_dylib_loadable()?;
            let handle = Arc::new(FastembedHandle {
                inner: Mutex::new(
                    TextEmbedding::try_new(
                        InitOptions::new(embedding_model(model)?)
                            .with_cache_dir(fastembed_cache_dir()?),
                    )
                    .context("initialize semantic embedding backend")?,
                ),
            });
            guard.insert(model.to_string(), Arc::clone(&handle));
            handle
        }
    };
    let prefixed_inputs = inputs
        .iter()
        .map(|input| match kind {
            EmbeddingInputKind::Query => format!("query: {input}"),
            EmbeddingInputKind::Document => format!("passage: {input}"),
        })
        .collect::<Vec<_>>();
    let mut embedding = handle
        .inner
        .lock()
        .map_err(|_| anyhow::anyhow!("semantic embedder runtime lock poisoned"))?;
    let vectors = embedding
        .embed(prefixed_inputs, None)
        .context("generate dense semantic embeddings")?;
    Ok(vectors
        .iter()
        .map(|vector| project_and_normalize(vector, output_dims))
        .collect())
}

#[cfg(not(test))]
static FASTEMBED_CACHE: OnceLock<Mutex<BTreeMap<String, Arc<FastembedHandle>>>> = OnceLock::new();

#[cfg(not(test))]
fn fastembed_cache_dir() -> Result<PathBuf> {
    fastembed_cache_dir_from_env(std::env::var_os("HF_HOME"), default_home_dir())
}

fn fastembed_cache_dir_from_env(
    hf_home: Option<OsString>,
    home_dir: Option<PathBuf>,
) -> Result<PathBuf> {
    if let Some(path) = hf_home.map(PathBuf::from) {
        return Ok(path);
    }

    let home = home_dir.context("locate home directory for fastembed cache")?;
    Ok(home.join(".codex").join("embed"))
}

#[cfg(not(test))]
fn default_home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

#[cfg(not(test))]
pub(crate) fn onnx_runtime_status(embedding_enabled: bool) -> OnnxRuntimeStatus {
    if !embedding_enabled {
        return OnnxRuntimeStatus {
            embedding_enabled,
            checked: false,
            loadable: false,
            would_use: false,
            dylib_path: None,
            reason: Some("ONNX Runtime disabled by ztldr config".to_string()),
        };
    }

    let path = resolved_onnxruntime_dylib_path();
    match ensure_onnxruntime_dylib_loadable_at(&path) {
        Ok(()) => OnnxRuntimeStatus {
            embedding_enabled,
            checked: true,
            loadable: true,
            would_use: embedding_enabled,
            dylib_path: Some(path.display().to_string()),
            reason: None,
        },
        Err(err) => OnnxRuntimeStatus {
            embedding_enabled,
            checked: true,
            loadable: false,
            would_use: false,
            dylib_path: Some(path.display().to_string()),
            reason: Some(err.to_string()),
        },
    }
}

#[cfg(test)]
pub(crate) fn onnx_runtime_status(embedding_enabled: bool) -> OnnxRuntimeStatus {
    if !embedding_enabled {
        return OnnxRuntimeStatus {
            embedding_enabled,
            checked: false,
            loadable: false,
            would_use: false,
            dylib_path: None,
            reason: Some("ONNX Runtime disabled by ztldr config".to_string()),
        };
    }

    OnnxRuntimeStatus {
        embedding_enabled,
        checked: false,
        loadable: false,
        would_use: false,
        dylib_path: None,
        reason: Some("test embedding backend does not load ONNX Runtime".to_string()),
    }
}

#[cfg(not(test))]
fn ensure_onnxruntime_dylib_loadable() -> Result<()> {
    ensure_onnxruntime_dylib_loadable_at(&resolved_onnxruntime_dylib_path())
}

#[cfg(not(test))]
fn resolved_onnxruntime_dylib_path() -> PathBuf {
    let raw_path = std::env::var_os("ORT_DYLIB_PATH")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| default_onnxruntime_dylib_name().into());

    if raw_path.is_absolute() {
        return raw_path;
    }

    if let Ok(current_exe) = std::env::current_exe()
        && let Some(parent) = current_exe.parent()
    {
        let relative = parent.join(&raw_path);
        if relative.exists() {
            return relative;
        }
    }

    raw_path
}

#[cfg(not(test))]
fn default_onnxruntime_dylib_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "onnxruntime.dll"
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        "libonnxruntime.so"
    }
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        "libonnxruntime.dylib"
    }
}

#[cfg(unix)]
fn ensure_onnxruntime_dylib_loadable_at(path: &Path) -> Result<()> {
    let c_path = CString::new(path.as_os_str().as_bytes())
        .with_context(|| format!("invalid ONNX Runtime dylib path `{}`", path.display()))?;
    let symbol = c"OrtGetApiBase";

    unsafe {
        let _ = libc::dlerror();
    }

    let handle = unsafe { libc::dlopen(c_path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
    if handle.is_null() {
        let detail = dlerror_message().unwrap_or_else(|| "unknown dlopen error".to_string());
        return Err(anyhow::anyhow!(
            "{ORT_BACKEND_UNAVAILABLE_MARKER} `{}` to be loadable: {detail}",
            path.display(),
        ));
    }

    unsafe {
        let _ = libc::dlerror();
    }
    let symbol_ptr = unsafe { libc::dlsym(handle, symbol.as_ptr()) };
    let symbol_error = dlerror_message();
    unsafe {
        libc::dlclose(handle);
    }

    if symbol_ptr.is_null() {
        let detail = symbol_error.unwrap_or_else(|| "missing `OrtGetApiBase` symbol".to_string());
        return Err(anyhow::anyhow!(
            "{ORT_BACKEND_UNAVAILABLE_MARKER} `{}` to expose `OrtGetApiBase`: {detail}",
            path.display(),
        ));
    }

    Ok(())
}

#[cfg(all(not(test), not(unix)))]
fn ensure_onnxruntime_dylib_loadable_at(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn dlerror_message() -> Option<String> {
    let error = unsafe { libc::dlerror() };
    if error.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(error) }
                .to_string_lossy()
                .into_owned(),
        )
    }
}

#[cfg(not(test))]
fn embedding_model(model: &str) -> Result<EmbeddingModel> {
    match model {
        "minilm" | "all-minilm-l6-v2" => Ok(EmbeddingModel::AllMiniLML6V2),
        "bge-small-en-v1.5" => Ok(EmbeddingModel::BGESmallENV15),
        "bge-base-en-v1.5" => Ok(EmbeddingModel::BGEBaseENV15),
        "bge-m3" => Ok(EmbeddingModel::BGEM3),
        "jina-code" | "jina-embeddings-v2-base-code" => {
            Ok(EmbeddingModel::JinaEmbeddingsV2BaseCode)
        }
        other => Err(anyhow::anyhow!(
            "unsupported semantic embedding model `{other}`"
        )),
    }
}

#[cfg(test)]
fn fake_embed(input: &str, kind: EmbeddingInputKind, output_dims: usize) -> Vec<f32> {
    let prefix = match kind {
        EmbeddingInputKind::Query => "query",
        EmbeddingInputKind::Document => "passage",
    };
    let mut vector = vec![0.0; output_dims.max(1)];
    for token in format!("{prefix} {input}")
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .filter(|token| !token.is_empty())
    {
        let idx = hash_token(token) % output_dims.max(1);
        vector[idx] += 1.0;
    }
    normalize(vector)
}

#[cfg(test)]
fn hash_token(token: &str) -> usize {
    token.bytes().fold(0usize, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(byte as usize)
    })
}

#[cfg(test)]
static TEST_EMBEDDING_FAILURE: OnceLock<Mutex<Option<String>>> = OnceLock::new();

#[cfg(test)]
pub(crate) fn set_test_embedding_failure(message: Option<&str>) {
    let mut guard = TEST_EMBEDDING_FAILURE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("test embedding failure lock should not be poisoned");
    *guard = message.map(str::to_string);
}

#[cfg(test)]
fn maybe_fail_test_embedding() -> Result<()> {
    let guard = TEST_EMBEDDING_FAILURE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("test embedding failure lock should not be poisoned");
    if let Some(message) = guard.as_ref() {
        Err(anyhow::anyhow!("{message}"))
    } else {
        Ok(())
    }
}

#[cfg(not(test))]
fn project_and_normalize(raw: &[f32], output_dims: usize) -> Vec<f32> {
    let dims = output_dims.max(1);
    let mut projected = vec![0.0; dims];
    for (index, value) in raw.iter().enumerate() {
        projected[index % dims] += *value;
    }
    normalize(projected)
}

fn normalize(mut vector: Vec<f32>) -> Vec<f32> {
    let magnitude = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if magnitude > 0.0 {
        for value in &mut vector {
            *value /= magnitude;
        }
    }
    vector
}

#[cfg(test)]
mod tests {
    use super::fastembed_cache_dir_from_env;
    use super::normalize;
    use pretty_assertions::assert_eq;
    use std::ffi::OsString;
    use std::path::PathBuf;

    #[test]
    fn normalize_preserves_zero_vector() {
        assert_eq!(normalize(vec![0.0, 0.0]), vec![0.0, 0.0]);
    }

    #[test]
    fn normalize_scales_non_zero_vector() {
        assert_eq!(normalize(vec![0.0, 5.0]), vec![0.0, 1.0]);
    }

    #[test]
    fn fastembed_cache_dir_prefers_hf_home() {
        assert_eq!(
            fastembed_cache_dir_from_env(
                Some(OsString::from("/tmp/hf-cache")),
                Some(PathBuf::from("/home/tester")),
            )
            .expect("cache dir should resolve"),
            PathBuf::from("/tmp/hf-cache")
        );
    }

    #[test]
    fn fastembed_cache_dir_defaults_to_codex_embed_under_home_without_hf_home() {
        assert_eq!(
            fastembed_cache_dir_from_env(None, Some(PathBuf::from("/home/tester")))
                .expect("cache dir should resolve"),
            PathBuf::from("/home/tester/.codex/embed")
        );
    }

    #[cfg(unix)]
    #[test]
    fn missing_dylib_returns_error_instead_of_panicking() {
        use super::ensure_onnxruntime_dylib_loadable_at;

        let error = ensure_onnxruntime_dylib_loadable_at(std::path::Path::new(
            "/definitely-missing/libonnxruntime.so",
        ))
        .expect_err("missing dylib should return an explicit error");

        let message = error.to_string();
        assert!(message.contains("requires ONNX Runtime dylib"));
        assert!(message.contains("/definitely-missing/libonnxruntime.so"));
    }
}
