//! Where nv keeps its data.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

/// Folder name of the embedding model inside `models/`.
pub const MODEL_NAME: &str = "bge-small-en-v1.5";

/// The one folder that holds the database and the embedding model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvHome {
    root: PathBuf,
}

impl NvHome {
    /// `nv_home_env` is the value of the `NV_HOME` env var, `user_home` the user's home folder.
    pub fn resolve(nv_home_env: Option<OsString>, user_home: Option<PathBuf>) -> Result<Self> {
        let root = match (nv_home_env.filter(|value| !value.is_empty()), user_home) {
            (Some(nv_home), _) => PathBuf::from(nv_home),
            (None, Some(user_home)) => user_home.join(".nv"),
            (None, None) => bail!("cannot find the nv folder: set the NV_HOME env var"),
        };
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn db_path(&self) -> PathBuf {
        self.root.join("nv.db")
    }

    pub fn model_dir(&self) -> PathBuf {
        self.root.join("models").join(MODEL_NAME)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_nv_home_env_var_when_set() {
        let home = NvHome::resolve(Some("/data/nv".into()), Some("/home/me".into())).unwrap();
        assert_eq!(home.root(), Path::new("/data/nv"));
    }

    #[test]
    fn defaults_to_home_dot_nv() {
        let home = NvHome::resolve(None, Some("/home/me".into())).unwrap();
        assert_eq!(home.root(), Path::new("/home/me/.nv"));
    }

    #[test]
    fn empty_nv_home_env_var_counts_as_not_set() {
        let home = NvHome::resolve(Some("".into()), Some("/home/me".into())).unwrap();
        assert_eq!(home.root(), Path::new("/home/me/.nv"));
    }

    #[test]
    fn fails_when_no_env_var_and_no_home_directory() {
        let error = NvHome::resolve(None, None).unwrap_err();
        assert!(error.to_string().contains("NV_HOME"), "{error}");
    }

    #[test]
    fn database_and_model_live_inside_nv_home() {
        let home = NvHome::resolve(Some("/data/nv".into()), None).unwrap();
        assert_eq!(home.db_path(), Path::new("/data/nv/nv.db"));
        assert_eq!(
            home.model_dir(),
            Path::new("/data/nv/models/bge-small-en-v1.5")
        );
    }
}
