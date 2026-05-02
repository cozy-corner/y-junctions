//! URI-keyed I/O helpers shared by pipeline binaries.
//!
//! Supported schemes:
//! - `https://` / `http://` — remote download via reqwest (read only)
//! - `gs://bucket/path` — Google Cloud Storage via object_store
//! - `file:///abs/path` — local filesystem via object_store
//!
//! `gs://` requires Application Default Credentials at runtime
//! (`gcloud auth application-default login` locally, metadata server in
//! Cloud Run).

use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use object_store::ObjectStoreExt;
use url::Url;

/// Read the contents of a URI fully into memory.
///
/// Used by walking-skeleton binaries where input artifacts are small
/// enough (≤ a few hundred MB) to fit comfortably in Cloud Run memory.
/// Streaming variants are intentionally deferred; see issue #229.
pub async fn read_uri(uri: &str) -> Result<Bytes> {
    if uri.starts_with("http://") || uri.starts_with("https://") {
        let response = reqwest::get(uri)
            .await
            .with_context(|| format!("HTTP GET failed: {uri}"))?
            .error_for_status()?;
        Ok(response.bytes().await?)
    } else {
        let url = Url::parse(uri).with_context(|| format!("invalid URI: {uri}"))?;
        let (store, path) = object_store::parse_url(&url)?;
        let result = store.get(&path).await?;
        Ok(result.bytes().await?)
    }
}

/// Write a byte buffer to a URI.
///
/// For `file://` outputs, parent directories are created automatically
/// because `object_store::local::LocalFileSystem::put` does not create
/// parents on its own.
pub async fn write_uri(uri: &str, body: Bytes) -> Result<()> {
    let url = Url::parse(uri).with_context(|| format!("invalid URI: {uri}"))?;

    if url.scheme() == "file" {
        let path = url
            .to_file_path()
            .map_err(|_| anyhow!("invalid file URL: {uri}"))?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("failed to create parent dir for {uri}"))?;
        }
    }

    let (store, path) = object_store::parse_url(&url)?;
    store.put(&path, body.into()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn roundtrip_local_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("nested/dir/payload.bin");
        let uri = format!("file://{}", target.display());

        let body = Bytes::from_static(b"hello pipeline");
        write_uri(&uri, body.clone()).await.unwrap();

        let read = read_uri(&uri).await.unwrap();
        assert_eq!(read, body);
    }
}
