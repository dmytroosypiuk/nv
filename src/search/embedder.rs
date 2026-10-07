//! What search needs from an embedding model, so tests can use a fake one.

use anyhow::Result;

/// Turns texts into normalized vectors.
pub trait Embedder {
    /// Name of the model, as stored with each embedding.
    fn model(&self) -> &str;
    fn dims(&self) -> usize;
    /// One vector per text, in the same order.
    fn embed(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}

/// Gives the embedder only when vectors are really needed: loading the real model
/// costs about 0.35 s and 330 MB.
pub trait ModelLoader {
    /// Name of the model, known without loading it.
    fn model(&self) -> &str;
    fn dims(&self) -> usize;
    /// True when the model files are there. Does not load them.
    fn is_installed(&self) -> bool;
    /// The loaded embedder, or `None` when the model is not installed.
    fn load(&mut self) -> Result<Option<&mut dyn Embedder>>;

    /// Like `load`, but gives `None` without loading when `skip` is true.
    fn load_unless(&mut self, skip: bool) -> Result<Option<&mut dyn Embedder>> {
        if skip { Ok(None) } else { self.load() }
    }
}

#[cfg(test)]
pub mod fake {
    use super::*;

    pub const FAKE_MODEL: &str = "fake-bag-of-words";
    const DIMS: usize = 64;

    /// Deterministic bag of words: texts that share words get similar vectors.
    #[derive(Debug, Default)]
    pub struct FakeEmbedder {
        /// Every text it was asked to embed, in order.
        pub embedded: Vec<String>,
    }

    impl Embedder for FakeEmbedder {
        fn model(&self) -> &str {
            FAKE_MODEL
        }

        fn dims(&self) -> usize {
            DIMS
        }

        fn embed(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            self.embedded.extend(texts.iter().cloned());
            Ok(texts.iter().map(|text| bag_of_words(text)).collect())
        }
    }

    fn bag_of_words(text: &str) -> Vec<f32> {
        let mut vector = vec![0.0f32; DIMS];
        let lowercase = text.to_lowercase();
        for word in lowercase.split(|c: char| !c.is_alphanumeric()) {
            if !word.is_empty() {
                // FNV-1a: stable across runs, unlike the std hasher.
                let hash = word.bytes().fold(0xcbf29ce484222325u64, |hash, byte| {
                    (hash ^ byte as u64).wrapping_mul(0x100000001b3)
                });
                vector[(hash % DIMS as u64) as usize] += 1.0;
            }
        }
        let norm = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            vector.iter_mut().for_each(|x| *x /= norm);
        }
        vector
    }

    /// A loader that counts how often the model was asked for.
    #[derive(Debug, Default)]
    pub struct FakeLoader {
        pub embedder: FakeEmbedder,
        pub loads: usize,
        /// Pretend the model folder is missing.
        pub missing: bool,
    }

    impl FakeLoader {
        pub fn missing() -> Self {
            Self {
                missing: true,
                ..Self::default()
            }
        }
    }

    impl ModelLoader for FakeLoader {
        fn model(&self) -> &str {
            FAKE_MODEL
        }

        fn dims(&self) -> usize {
            DIMS
        }

        fn is_installed(&self) -> bool {
            !self.missing
        }

        fn load(&mut self) -> Result<Option<&mut dyn Embedder>> {
            self.loads += 1;
            Ok((!self.missing).then_some(&mut self.embedder as &mut dyn Embedder))
        }
    }
}
