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
use tracing::debug;

use super::S3Client;

impl S3Client {
    /// Verify S3 connectivity by performing a lightweight list-buckets call.
    pub async fn health_check(&self) -> Result<()> {
        debug!("Performing S3 health check");

        self.client
            .list_buckets()
            .send()
            .await
            .context("S3 health check failed")?;

        debug!("S3 health check passed");
        Ok(())
    }
}
