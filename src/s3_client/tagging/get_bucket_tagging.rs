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
use aws_sdk_s3::error::ProvideErrorMetadata;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tracing::debug;

use super::super::S3Client;
use super::{UNTAGGED_ERROR_CODE, tag_set_from_sdk};

/// Result payload for reading a bucket's tag set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketTags {
    pub bucket: String,
    /// Empty when the bucket has no tags; sorted by key.
    pub tags: BTreeMap<String, String>,
}

impl S3Client {
    /// Read a bucket's tag set.
    pub async fn get_bucket_tagging(&self, bucket_name: &str) -> Result<BucketTags> {
        debug!("Fetching tags for bucket '{}'", bucket_name);

        let response = match self
            .client
            .get_bucket_tagging()
            .bucket(bucket_name)
            .send()
            .await
        {
            Ok(response) => response,
            Err(err)
                if err.as_service_error().and_then(|e| e.code()) == Some(UNTAGGED_ERROR_CODE) =>
            {
                // A bucket that was never tagged answers with a dedicated
                // error; report that as an empty tag set instead of a failure.
                // Some S3 implementations answer with the same code for a
                // bucket that does not exist, so check whether the bucket is
                // reachable before claiming it is merely untagged. The check
                // is best-effort: an AccessDenied HEAD still counts as
                // reachable, so plain AWS tag reads never need the
                // `s3:ListBucket` permission; a 404 or a missing answer
                // keeps the error path so a typo stays an error.
                if !self.bucket_is_reachable(bucket_name).await {
                    return Err(anyhow::anyhow!(
                        "Failed to get the tags of bucket '{bucket_name}': \
                         the bucket does not exist or is not accessible"
                    ));
                }

                return Ok(BucketTags {
                    bucket: bucket_name.to_string(),
                    tags: BTreeMap::new(),
                });
            }
            Err(err) => {
                return Err(err)
                    .context(format!("Failed to get the tags of bucket '{bucket_name}'"));
            }
        };

        Ok(BucketTags {
            bucket: bucket_name.to_string(),
            tags: tag_set_from_sdk(response.tag_set()),
        })
    }
}
