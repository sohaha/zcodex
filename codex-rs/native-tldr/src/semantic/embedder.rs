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
#[cfg(not(test))]
use std::sync::Arc;
#[cfg(not(test))]
use std::sync::Mutex;
#[cfg(not(test))]
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
            let handle = Arc::new(FastembedHandle {
                inner: Mutex::new(TextEmbedding::try_new(InitOptions::new(embedding_model(
                    model,
                )?))?),
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
    let vectors = handle
        .inner
        .lock()
        .map_err(|_| anyhow::anyhow!("semantic embedder runtime lock poisoned"))?
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
fn embedding_model(model: &str) -> Result<EmbeddingModel> {
    match model {
        "minilm" | "all-minilm-l6-v2" => Ok(EmbeddingModel::AllMiniLML6V2),
        "bge-small-en-v1.5" => Ok(EmbeddingModel::BGESmallENV15),
        "bge-base-en-v1.5" => Ok(EmbeddingModel::BGEBaseENV15),
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
