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

//! Bucket lifecycle rules: when objects expire, move to another storage
//! class, or get cleaned up.

mod delete_bucket_lifecycle;
mod get_bucket_lifecycle;
mod put_bucket_lifecycle;
mod rule;
mod validate;

pub use get_bucket_lifecycle::BucketLifecycleConfig;
pub use rule::{
    LifecycleExpiration, LifecycleRule, LifecycleRuleStatus, LifecycleTransition,
    NoncurrentVersionExpiration, NoncurrentVersionTransition,
};

/// Maximum number of rules in one lifecycle configuration.
pub const MAX_LIFECYCLE_RULES: usize = 1000;

/// Maximum length of a rule ID, in Unicode characters.
pub const MAX_RULE_ID_LENGTH: usize = 255;
