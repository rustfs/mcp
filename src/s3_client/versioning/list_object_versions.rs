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
use aws_sdk_s3::primitives::{DateTime, DateTimeFormat};
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::super::S3Client;

/// Options for `list_object_versions`.
#[derive(Debug, Clone, Default)]
pub struct ListObjectVersionsOptions {
    pub prefix: Option<String>,
    pub delimiter: Option<String>,
    pub max_keys: Option<i32>,
    pub key_marker: Option<String>,
    pub version_id_marker: Option<String>,
}

/// One entry in an object-versions listing: either a stored version or a
/// delete marker of an object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionEntry {
    pub key: String,
    pub version_id: Option<String>,
    /// Whether the entry is the current one: what a plain read of `key`
    /// resolves to.
    pub is_latest: bool,
    /// Whether the entry is a delete marker, which hides the older versions
    /// of `key`, instead of a stored version.
    pub is_delete_marker: bool,
    pub last_modified: Option<String>,
    /// Size, ETag, and storage class of a stored version; `None` for a
    /// delete marker.
    pub size: Option<i64>,
    pub etag: Option<String>,
    pub storage_class: Option<String>,
}

/// Result payload for `list_object_versions`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListObjectVersionsResult {
    /// Versions and delete markers, grouped by key with the newest entry first.
    pub entries: Vec<VersionEntry>,
    pub common_prefixes: Vec<String>,
    pub is_truncated: bool,
    pub next_key_marker: Option<String>,
    pub next_version_id_marker: Option<String>,
    pub max_keys: Option<i32>,
}

/// The parts of a listing entry other than its key and timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EntryBody {
    version_id: Option<String>,
    is_latest: bool,
    is_delete_marker: bool,
    size: Option<i64>,
    etag: Option<String>,
    storage_class: Option<String>,
}

/// Order listing entries the way S3 interleaves them: by key ascending, then
/// newest first. Both of S3's lists already follow that order and the sort is
/// stable, so entries that share a timestamp keep their relative order.
fn order_by_key_then_newest<T>(entries: &mut [(String, Option<DateTime>, T)]) {
    entries.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)));
}

fn format_timestamp(dt: &DateTime) -> String {
    dt.fmt(DateTimeFormat::DateTime).unwrap()
}

impl S3Client {
    /// List every version and delete marker in a bucket, newest first per key.
    pub async fn list_object_versions(
        &self,
        bucket_name: &str,
        options: ListObjectVersionsOptions,
    ) -> Result<ListObjectVersionsResult> {
        debug!(
            "Listing object versions in bucket '{}' with options: {:?}",
            bucket_name, options
        );

        let mut request = self.client.list_object_versions().bucket(bucket_name);

        if let Some(prefix) = options.prefix {
            request = request.prefix(prefix);
        }

        if let Some(delimiter) = options.delimiter {
            request = request.delimiter(delimiter);
        }

        if let Some(max_keys) = options.max_keys {
            request = request.max_keys(max_keys);
        }

        if let Some(key_marker) = options.key_marker {
            request = request.key_marker(key_marker);
        }

        if let Some(version_id_marker) = options.version_id_marker {
            request = request.version_id_marker(version_id_marker);
        }

        let response = request.send().await.context(format!(
            "Failed to list object versions in bucket '{bucket_name}'"
        ))?;

        // S3 returns versions and delete markers as two parallel lists, each
        // ordered by key with the newest entry first. Collect both, then put
        // them back into one listing.
        let mut ordered: Vec<(String, Option<DateTime>, EntryBody)> = Vec::new();

        ordered.extend(response.versions().iter().map(|version| {
            (
                version.key().unwrap_or("unknown").to_string(),
                version.last_modified().copied(),
                EntryBody {
                    version_id: version.version_id().map(|v| v.to_string()),
                    is_latest: version.is_latest().unwrap_or(false),
                    is_delete_marker: false,
                    size: version.size(),
                    etag: version.e_tag().map(|e| e.to_string()),
                    storage_class: version.storage_class().map(|sc| sc.as_str().to_string()),
                },
            )
        }));

        ordered.extend(response.delete_markers().iter().map(|marker| {
            (
                marker.key().unwrap_or("unknown").to_string(),
                marker.last_modified().copied(),
                EntryBody {
                    version_id: marker.version_id().map(|v| v.to_string()),
                    is_latest: marker.is_latest().unwrap_or(false),
                    is_delete_marker: true,
                    size: None,
                    etag: None,
                    storage_class: None,
                },
            )
        }));

        order_by_key_then_newest(&mut ordered);

        let entries = ordered
            .into_iter()
            .map(|(key, last_modified, body)| VersionEntry {
                key,
                version_id: body.version_id,
                is_latest: body.is_latest,
                is_delete_marker: body.is_delete_marker,
                last_modified: last_modified.as_ref().map(format_timestamp),
                size: body.size,
                etag: body.etag,
                storage_class: body.storage_class,
            })
            .collect();

        let common_prefixes: Vec<String> = response
            .common_prefixes()
            .iter()
            .filter_map(|cp| cp.prefix())
            .map(|p| p.to_string())
            .collect();

        let result = ListObjectVersionsResult {
            entries,
            common_prefixes,
            is_truncated: response.is_truncated().unwrap_or(false),
            next_key_marker: response.next_key_marker().map(|m| m.to_string()),
            next_version_id_marker: response.next_version_id_marker().map(|m| m.to_string()),
            max_keys: response.max_keys(),
        };

        debug!(
            "Found {} version(s)/delete marker(s) and {} common prefixes in bucket '{}'",
            result.entries.len(),
            result.common_prefixes.len(),
            bucket_name
        );

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timestamp(secs: i64) -> Option<DateTime> {
        Some(DateTime::from_secs(secs))
    }

    #[test]
    fn test_list_object_versions_options_default() {
        let options = ListObjectVersionsOptions::default();
        assert!(options.prefix.is_none());
        assert!(options.delimiter.is_none());
        assert!(options.max_keys.is_none());
        assert!(options.key_marker.is_none());
        assert!(options.version_id_marker.is_none());
    }

    #[test]
    fn test_order_by_key_then_newest_interleaves_versions_and_markers() {
        // Versions and delete markers arrive in separate lists; each list is
        // ordered newest first.
        let mut entries = vec![
            ("a.txt".to_string(), timestamp(300), "a v3"),
            ("a.txt".to_string(), timestamp(100), "a v1"),
            ("b.txt".to_string(), timestamp(50), "b v1"),
            ("a.txt".to_string(), timestamp(400), "a marker"),
            ("a.txt".to_string(), timestamp(200), "a marker old"),
        ];

        order_by_key_then_newest(&mut entries);

        let labels: Vec<&str> = entries.iter().map(|(_, _, label)| *label).collect();
        assert_eq!(labels, ["a marker", "a v3", "a marker old", "a v1", "b v1"]);
    }

    #[test]
    fn test_order_by_key_then_newest_keeps_relative_order_on_ties() {
        let mut entries = vec![
            ("k".to_string(), timestamp(10), "first listed"),
            ("k".to_string(), timestamp(10), "second listed"),
            ("k".to_string(), timestamp(20), "newest"),
        ];

        order_by_key_then_newest(&mut entries);

        let labels: Vec<&str> = entries.iter().map(|(_, _, label)| *label).collect();
        assert_eq!(labels, ["newest", "first listed", "second listed"]);
    }

    #[test]
    fn test_order_by_key_then_newest_sorts_missing_timestamps_last() {
        let mut entries = vec![
            ("k".to_string(), None, "no timestamp"),
            ("k".to_string(), timestamp(1), "oldest"),
        ];

        order_by_key_then_newest(&mut entries);

        let labels: Vec<&str> = entries.iter().map(|(_, _, label)| *label).collect();
        assert_eq!(labels, ["oldest", "no timestamp"]);
    }

    #[test]
    fn test_version_entry_serialization() {
        let entry = VersionEntry {
            key: "docs/report.pdf".to_string(),
            version_id: Some("v2".to_string()),
            is_latest: true,
            is_delete_marker: false,
            last_modified: Some("2026-11-25T10:00:00Z".to_string()),
            size: Some(2048),
            etag: Some("\"abc\"".to_string()),
            storage_class: Some("STANDARD".to_string()),
        };

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: VersionEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, entry);
    }
}
