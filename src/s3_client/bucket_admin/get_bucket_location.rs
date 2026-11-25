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
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::super::S3Client;

/// Result payload for `get_bucket_location`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketLocation {
    pub bucket: String,
    pub region: String,
}

/// Region name for a `LocationConstraint` value. S3 reports its default
/// region `us-east-1` as an empty constraint.
fn region_from_constraint(constraint: Option<&str>) -> String {
    match constraint {
        Some(region) if !region.is_empty() => region.to_string(),
        _ => "us-east-1".to_string(),
    }
}

impl S3Client {
    /// Read the region a bucket lives in.
    pub async fn get_bucket_location(&self, bucket_name: &str) -> Result<BucketLocation> {
        debug!("Fetching the location of bucket '{}'", bucket_name);

        let response = self
            .client
            .get_bucket_location()
            .bucket(bucket_name)
            .send()
            .await
            .context(format!(
                "Failed to get the location of bucket '{bucket_name}'"
            ))?;

        Ok(BucketLocation {
            bucket: bucket_name.to_string(),
            region: region_from_constraint(response.location_constraint().map(|c| c.as_str())),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_region_from_constraint() {
        assert_eq!(
            region_from_constraint(Some("eu-west-1")),
            "eu-west-1".to_string()
        );
        // S3 reports its default region as an empty constraint.
        assert_eq!(region_from_constraint(Some("")), "us-east-1".to_string());
        assert_eq!(region_from_constraint(None), "us-east-1".to_string());
    }
}
