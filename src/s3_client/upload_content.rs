// Copyright 2024 RustFS Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use anyhow::{Context, Result};
use aws_sdk_s3::primitives::ByteStream;
use tracing::{debug, info};

use super::S3Client;
use super::types::{UploadFileOptions, UploadResult};
use super::upload_options::apply_put_object_options;

/// Largest payload accepted by [`S3Client::upload_content`]. Larger content
/// must be written to a local file and sent with [`S3Client::upload_file`].
pub const MAX_INLINE_CONTENT_BYTES: usize = 5 * 1024 * 1024;

/// Content type used when neither an explicit content type nor the object
/// key's extension identifies one. Inline content is text, so `text/plain`
/// keeps `get_object` able to display it.
const DEFAULT_INLINE_CONTENT_TYPE: &str = "text/plain";

impl S3Client {
    /// Upload inline content, creating or replacing the object at `object_key`.
    pub async fn upload_content(
        &self,
        content: &str,
        bucket_name: &str,
        object_key: &str,
        options: UploadFileOptions,
    ) -> Result<UploadResult> {
        info!(
            "Uploading inline content ({} bytes) to s3://{}/{}",
            content.len(),
            bucket_name,
            object_key
        );

        ensure_inline_size(content.len())?;

        let content_type = resolve_content_type(options.content_type.as_deref(), object_key);
        debug!("Content type for inline upload: {content_type}");

        let size = content.len();

        let request = apply_put_object_options(
            self.client
                .put_object()
                .bucket(bucket_name)
                .key(object_key)
                .body(ByteStream::from(content.as_bytes().to_vec()))
                .content_type(&content_type)
                .content_length(size as i64),
            &options,
        );

        debug!("Executing S3 put_object request");
        let response = request.send().await.context(format!(
            "Failed to upload content to s3://{bucket_name}/{object_key}"
        ))?;

        let etag = response.e_tag().unwrap_or("unknown").to_string();
        let version_id = response.version_id().map(|v| v.to_string());

        info!(
            "Inline content uploaded successfully: {} bytes to s3://{}/{}",
            size, bucket_name, object_key
        );

        Ok(UploadResult {
            bucket: bucket_name.to_string(),
            key: object_key.to_string(),
            etag,
            location: format!("s3://{bucket_name}/{object_key}"),
            version_id,
            file_size: size as u64,
            content_type,
            upload_id: None,
        })
    }
}

/// Reject payloads too large for an inline upload.
fn ensure_inline_size(len: usize) -> Result<()> {
    if len > MAX_INLINE_CONTENT_BYTES {
        anyhow::bail!(
            "Content is {len} bytes, exceeding the {MAX_INLINE_CONTENT_BYTES}-byte limit for inline \
             uploads; write it to a local file and upload that instead"
        );
    }
    Ok(())
}

/// Pick the content type for inline content: an explicit value wins, then the
/// object key's extension, then the plain-text default.
fn resolve_content_type(explicit: Option<&str>, object_key: &str) -> String {
    if let Some(content_type) = explicit {
        return content_type.to_string();
    }

    match mime_guess::from_path(object_key).first() {
        // An unknown extension yields nothing and an uninformative one yields
        // octet-stream; neither is a better match for text than the default.
        Some(mime) if mime.essence_str() != "application/octet-stream" => mime.to_string(),
        _ => DEFAULT_INLINE_CONTENT_TYPE.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ensure_inline_size_accepts_limit_and_rejects_more() {
        assert!(ensure_inline_size(0).is_ok());
        assert!(ensure_inline_size(MAX_INLINE_CONTENT_BYTES).is_ok());

        let error = ensure_inline_size(MAX_INLINE_CONTENT_BYTES + 1).unwrap_err();
        assert!(error.to_string().contains("exceeding"));
        assert!(error.to_string().contains("local file"));
    }

    #[test]
    fn test_resolve_content_type_prefers_explicit_value() {
        assert_eq!(
            resolve_content_type(Some("application/yaml"), "report.json"),
            "application/yaml"
        );
    }

    #[test]
    fn test_resolve_content_type_guesses_from_key_extension() {
        assert_eq!(
            resolve_content_type(None, "report.json"),
            "application/json"
        );
        assert_eq!(
            resolve_content_type(None, "docs/readme.md"),
            "text/markdown"
        );
        assert_eq!(resolve_content_type(None, "data/report.csv"), "text/csv");
    }

    #[test]
    fn test_resolve_content_type_falls_back_to_plain_text() {
        // No extension.
        assert_eq!(resolve_content_type(None, "notes/today"), "text/plain");
        // An extension with no known type.
        assert_eq!(resolve_content_type(None, "blob.unknownext"), "text/plain");
        // An extension whose only guess is octet-stream.
        assert_eq!(resolve_content_type(None, "blob.bin"), "text/plain");
    }
}
