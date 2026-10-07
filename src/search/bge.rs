//! The real model: bge-small-en-v1.5 through fastembed, from local files only.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fastembed::{
    InitOptionsUserDefined, Pooling, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel,
};

use super::embedder::{Embedder, ModelLoader};
use crate::config::MODEL_NAME;

pub const DIMS: usize = 384;
/// The model cuts longer input.
const MAX_TOKENS: usize = 512;

pub struct BgeSmall {
    model: TextEmbedding,
}

impl BgeSmall {
    /// Loads the model from the files in `model_dir`. No hub, no cache, no network.
    pub fn load(model_dir: &Path) -> Result<Self> {
        let read = |name: &str| {
            let path = model_dir.join(name);
            fs::read(&path).with_context(|| format!("cannot read model file {}", path.display()))
        };
        let tokenizer_files = TokenizerFiles {
            tokenizer_file: read("tokenizer.json")?,
            config_file: read("config.json")?,
            special_tokens_map_file: read("special_tokens_map.json")?,
            tokenizer_config_file: read("tokenizer_config.json")?,
        };
        // fastembed leaves pooling unset for user-defined models. BGE needs the CLS token;
        // wrong pooling gives vectors that look fine but rank worse.
        let model = UserDefinedEmbeddingModel::new(read("model.onnx")?, tokenizer_files)
            .with_pooling(Pooling::Cls);
        let options = InitOptionsUserDefined::new().with_max_length(MAX_TOKENS);
        let model = TextEmbedding::try_new_from_user_defined(model, options)
            .with_context(|| format!("cannot load the model in {}", model_dir.display()))?;
        Ok(Self { model })
    }
}

impl Embedder for BgeSmall {
    fn model(&self) -> &str {
        MODEL_NAME
    }

    fn dims(&self) -> usize {
        DIMS
    }

    fn embed(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        // No prefix, for notes and for queries.
        Ok(self.model.embed(texts, None)?)
    }
}

/// Loads the model on first use and keeps it for the rest of the command.
pub struct BgeSmallLoader {
    model_dir: PathBuf,
    loaded: Option<BgeSmall>,
}

impl BgeSmallLoader {
    pub fn new(model_dir: PathBuf) -> Self {
        Self {
            model_dir,
            loaded: None,
        }
    }

    pub fn model_dir(&self) -> &Path {
        &self.model_dir
    }

    /// True when the model file is there. Does not load it.
    pub fn is_installed(&self) -> bool {
        self.model_dir.join("model.onnx").is_file()
    }
}

impl ModelLoader for BgeSmallLoader {
    fn model(&self) -> &str {
        MODEL_NAME
    }

    fn load(&mut self) -> Result<Option<&mut dyn Embedder>> {
        if !self.is_installed() {
            return Ok(None);
        }
        if self.loaded.is_none() {
            self.loaded = Some(BgeSmall::load(&self.model_dir)?);
        }
        Ok(self.loaded.as_mut().map(|model| model as &mut dyn Embedder))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::vectors::dot;
    use serde::Deserialize;

    /// `NV_MODEL_DIR`, or the `models/` folder of the repo.
    fn model_dir() -> PathBuf {
        std::env::var_os("NV_MODEL_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("models")
                    .join(MODEL_NAME)
            })
    }

    fn texts(texts: &[&str]) -> Vec<String> {
        texts.iter().map(|text| text.to_string()).collect()
    }

    #[test]
    fn missing_model_folder_gives_no_embedder() {
        let nowhere = tempfile::TempDir::new().unwrap();
        let mut loader = BgeSmallLoader::new(nowhere.path().join(MODEL_NAME));

        assert!(!loader.is_installed());
        assert!(loader.load().unwrap().is_none());
        assert_eq!(loader.model(), "bge-small-en-v1.5");
    }

    #[test]
    #[ignore = "loads the real model"]
    fn bge_small_uses_cls_pooling_and_384_dims() {
        let mut model = BgeSmall::load(&model_dir()).unwrap();

        let vectors = model
            .embed(&texts(&[
                "Retry 5 times\nRetry count for billing-api calls is 5.",
                "how many times do we retry billing calls",
            ]))
            .unwrap();

        for vector in &vectors {
            assert_eq!(vector.len(), 384);
            assert!((dot(vector, vector).sqrt() - 1.0).abs() < 1e-3);
        }
        // Pinned with CLS pooling. Mean pooling gives a different number.
        let cosine = dot(&vectors[0], &vectors[1]);
        assert!((cosine - PINNED_COSINE).abs() < 2e-3, "cosine was {cosine}");
    }

    const PINNED_COSINE: f32 = 0.9094;

    #[derive(Deserialize)]
    struct SpikeData {
        notes: Vec<SpikeNote>,
        queries: Vec<SpikeQuery>,
    }

    #[derive(Deserialize)]
    struct SpikeNote {
        id: i64,
        title: String,
        body: String,
    }

    #[derive(Deserialize)]
    struct SpikeQuery {
        query: String,
    }

    #[test]
    #[ignore = "loads the real model"]
    fn bge_small_pinned_ranking_on_spike_notes() {
        // Rank 1 for the ten spike queries, from spikes/embedding/REPORT.md (no prefix).
        const RANK_ONE: [i64; 10] = [2, 4, 5, 6, 9, 7, 17, 14, 13, 20];
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("spikes/embedding/notes.json");
        let data: SpikeData = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        let mut model = BgeSmall::load(&model_dir()).unwrap();
        let note_texts: Vec<String> = data
            .notes
            .iter()
            .map(|note| crate::search::vectors::embedding_text(&note.title, &note.body))
            .collect();
        let note_vectors = model.embed(&note_texts).unwrap();

        let mut rank_one = Vec::new();
        for query in &data.queries {
            let query_vector = model.embed(&[query.query.clone()]).unwrap().remove(0);
            let best = note_vectors
                .iter()
                .zip(&data.notes)
                .max_by(|a, b| dot(&query_vector, a.0).total_cmp(&dot(&query_vector, b.0)))
                .unwrap();
            rank_one.push(best.1.id);
        }

        assert_eq!(rank_one, RANK_ONE);
    }

    #[test]
    #[ignore = "loads the real model"]
    fn bge_small_loads_from_a_copied_folder_and_is_loaded_once() {
        let elsewhere = tempfile::TempDir::new().unwrap();
        let copy = elsewhere.path().join(MODEL_NAME);
        fs::create_dir_all(&copy).unwrap();
        for file in fs::read_dir(model_dir()).unwrap() {
            let file = file.unwrap();
            fs::copy(file.path(), copy.join(file.file_name())).unwrap();
        }
        let mut loader = BgeSmallLoader::new(copy);

        let first = loader
            .load()
            .unwrap()
            .unwrap()
            .embed(&texts(&["when is my exam"]))
            .unwrap();
        let second = loader
            .load()
            .unwrap()
            .unwrap()
            .embed(&texts(&["when is my exam"]))
            .unwrap();

        assert_eq!(first, second);
        assert_eq!(first[0].len(), DIMS);
    }
}
