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

//! Bucket replication rules: which object versions are copied to which
//! other bucket.

mod delete_bucket_replication;
mod get_bucket_replication;
mod put_bucket_replication;
mod rule;
mod validate;

pub use get_bucket_replication::BucketReplicationConfig;
pub use rule::{ReplicationDestination, ReplicationRule, ReplicationRuleStatus};

/// Maximum number of rules in one replication configuration.
pub const MAX_REPLICATION_RULES: usize = 1000;
