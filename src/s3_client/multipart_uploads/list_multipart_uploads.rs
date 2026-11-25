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
use aws_sdk_s3::operation::list_multipart_uploads::ListMultipartUploadsOutput;
use aws_sdk_s3::primitives::{DateTime, DateTimeFormat};
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::super::S3Client;

/// Largest page size the S3 ListMultipartUploads API returns.
const MAX_PAGE_UPLOADS: i32 = 1000;

/// Options for `list_multipart_uploads`.
#[derive(Debug, Clone, Default)]
pub struct ListMultipartUploadsOptions {
    pub prefix: Option<String>,
    pub delimiter: Option<String>,
    pub max_uploads: Option<i32>,
    pub key_marker: Option<String>,
    pub upload_id_marker: Option<String>,
}

/// One in-progress multipart upload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultipartUploadInfo {
    pub key: String,
    pub upload_id: String,
    pub initiated: Option<String>,
    pub storage_class: Option<String>,
}

/// Result payload for one page of `list_multipart_uploads`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListMultipartUploadsResult {
    pub uploads: Vec<MultipartUploadInfo>,
    pub common_prefixes: Vec<String>,
    /// Whether more uploads exist beyond this page.
    pub is_truncated: bool,
    /// Markers selecting the next page when `is_truncated` is set.
    pub next_key_marker: Option<String>,
    pub next_upload_id_marker: Option<String>,
    pub max_uploads: Option<i32>,
}

/// Reject a page size outside the range S3 accepts. Backends differ on larger
/// values (some clamp, some fail), so the range is enforced before the call.
fn ensure_page_size(max_uploads: Option<i32>) -> Result<()> {
    match max_uploads {
        None | Some(1..=MAX_PAGE_UPLOADS) => Ok(()),
        Some(max_uploads) => {
            anyhow::bail!("max_uploads must be between 1 and {MAX_PAGE_UPLOADS}, got {max_uploads}")
        }
    }
}

fn format_timestamp(dt: &DateTime) -> String {
    dt.fmt(DateTimeFormat::DateTime).unwrap()
}

/// Convert one page of S3's listing into the result payload.
fn to_list_result(response: &ListMultipartUploadsOutput) -> ListMultipartUploadsResult {
    let uploads = response
        .uploads()
        .iter()
        .map(|upload| MultipartUploadInfo {
            key: upload.key().unwrap_or("unknown").to_string(),
            upload_id: upload.upload_id().unwrap_or("unknown").to_string(),
            initiated: upload.initiated().map(format_timestamp),
            // Some backends (MinIO) send an empty storage class for an
            // in-progress upload; report that as absent rather than blank.
            storage_class: upload
                .storage_class()
                .map(|sc| sc.as_str())
                .filter(|sc| !sc.is_empty())
                .map(str::to_string),
        })
        .collect();

    let common_prefixes: Vec<String> = response
        .common_prefixes()
        .iter()
        .filter_map(|cp| cp.prefix())
        .map(|p| p.to_string())
        .collect();

    ListMultipartUploadsResult {
        uploads,
        common_prefixes,
        is_truncated: response.is_truncated().unwrap_or(false),
        next_key_marker: response.next_key_marker().map(|m| m.to_string()),
        next_upload_id_marker: response.next_upload_id_marker().map(|m| m.to_string()),
        max_uploads: response.max_uploads(),
    }
}

impl S3Client {
    /// List the in-progress multipart uploads of a bucket, one page per call.
    ///
    /// When the page is truncated the result carries the markers that select
    /// the next page; paging through them is left to the caller.
    pub async fn list_multipart_uploads(
        &self,
        bucket_name: &str,
        options: ListMultipartUploadsOptions,
    ) -> Result<ListMultipartUploadsResult> {
        debug!(
            "Listing multipart uploads in bucket '{}' with options: {:?}",
            bucket_name, options
        );

        ensure_page_size(options.max_uploads)?;

        let mut request = self.client.list_multipart_uploads().bucket(bucket_name);

        if let Some(prefix) = options.prefix {
            request = request.prefix(prefix);
        }

        if let Some(delimiter) = options.delimiter {
            request = request.delimiter(delimiter);
        }

        if let Some(max_uploads) = options.max_uploads {
            request = request.max_uploads(max_uploads);
        }

        if let Some(key_marker) = options.key_marker {
            request = request.key_marker(key_marker);
        }

        if let Some(upload_id_marker) = options.upload_id_marker {
            request = request.upload_id_marker(upload_id_marker);
        }

        let response = request.send().await.context(format!(
            "Failed to list multipart uploads in bucket '{bucket_name}'"
        ))?;

        let result = to_list_result(&response);

        debug!(
            "Found {} in-progress multipart upload(s) and {} common prefixes in bucket '{}'",
            result.uploads.len(),
            result.common_prefixes.len(),
            bucket_name
        );

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_s3::types::{CommonPrefix, MultipartUpload, StorageClass};

    #[test]
    fn test_list_multipart_uploads_options_default() {
        let options = ListMultipartUploadsOptions::default();
        assert!(options.prefix.is_none());
        assert!(options.delimiter.is_none());
        assert!(options.max_uploads.is_none());
        assert!(options.key_marker.is_none());
        assert!(options.upload_id_marker.is_none());
    }

    #[test]
    fn test_multipart_upload_info_serialization() {
        let info = MultipartUploadInfo {
            key: "videos/a.mp4".to_string(),
            upload_id: "upl-1".to_string(),
            initiated: Some("2026-11-25T10:00:00Z".to_string()),
            storage_class: Some("STANDARD".to_string()),
        };

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: MultipartUploadInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, info);
    }

    #[test]
    fn test_ensure_page_size_accepts_one_to_max_and_none() {
        assert!(ensure_page_size(None).is_ok());
        assert!(ensure_page_size(Some(1)).is_ok());
        assert!(ensure_page_size(Some(MAX_PAGE_UPLOADS)).is_ok());
    }

    #[test]
    fn test_ensure_page_size_rejects_other_values() {
        for max_uploads in [0, -1, MAX_PAGE_UPLOADS + 1] {
            let error = ensure_page_size(Some(max_uploads)).unwrap_err();
            assert!(
                error.to_string().contains("between 1 and 1000"),
                "{max_uploads}: {error}"
            );
        }
    }

    #[test]
    fn test_to_list_result_maps_one_page() {
        let first = MultipartUpload::builder()
            .key("videos/a.mp4")
            .upload_id("upl-1")
            .initiated(DateTime::from_secs(1_000_000_000))
            .storage_class(StorageClass::Standard)
            .build();
        // S3 may leave out the timestamp and the storage class.
        let second = MultipartUpload::builder()
            .key("videos/b.mp4")
            .upload_id("upl-2")
            .build();
        // MinIO sends the storage class as an empty element.
        let third = MultipartUpload::builder()
            .key("videos/c.mp4")
            .upload_id("upl-3")
            .storage_class(StorageClass::from(""))
            .build();

        let output = ListMultipartUploadsOutput::builder()
            .set_uploads(Some(vec![first, second, third]))
            .common_prefixes(CommonPrefix::builder().prefix("backups/").build())
            .is_truncated(true)
            .next_key_marker("videos/c.mp4")
            .next_upload_id_marker("upl-3")
            .max_uploads(3)
            .build();

        assert_eq!(
            to_list_result(&output),
            ListMultipartUploadsResult {
                uploads: vec![
                    MultipartUploadInfo {
                        key: "videos/a.mp4".to_string(),
                        upload_id: "upl-1".to_string(),
                        initiated: Some("2001-09-09T01:46:40Z".to_string()),
                        storage_class: Some("STANDARD".to_string()),
                    },
                    MultipartUploadInfo {
                        key: "videos/b.mp4".to_string(),
                        upload_id: "upl-2".to_string(),
                        initiated: None,
                        storage_class: None,
                    },
                    MultipartUploadInfo {
                        key: "videos/c.mp4".to_string(),
                        upload_id: "upl-3".to_string(),
                        initiated: None,
                        storage_class: None,
                    },
                ],
                common_prefixes: vec!["backups/".to_string()],
                is_truncated: true,
                next_key_marker: Some("videos/c.mp4".to_string()),
                next_upload_id_marker: Some("upl-3".to_string()),
                max_uploads: Some(3),
            }
        );
    }

    #[test]
    fn test_to_list_result_of_an_empty_page() {
        let output = ListMultipartUploadsOutput::builder()
            .max_uploads(1000)
            .build();

        let result = to_list_result(&output);
        assert!(result.uploads.is_empty());
        assert!(result.common_prefixes.is_empty());
        assert!(!result.is_truncated);
        assert!(result.next_key_marker.is_none());
        assert!(result.next_upload_id_marker.is_none());
        assert_eq!(result.max_uploads, Some(1000));
    }

    #[test]
    fn test_to_list_result_falls_back_for_missing_fields() {
        let output = ListMultipartUploadsOutput::builder()
            .uploads(MultipartUpload::builder().build())
            .build();

        let result = to_list_result(&output);
        assert_eq!(result.uploads[0].key, "unknown");
        assert_eq!(result.uploads[0].upload_id, "unknown");
        assert!(result.uploads[0].initiated.is_none());
        assert!(result.uploads[0].storage_class.is_none());
    }
}
