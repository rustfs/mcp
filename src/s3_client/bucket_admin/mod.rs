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

//! Bucket inspection, bucket policies, and request payment.

mod delete_bucket_policy;
mod get_bucket_location;
mod get_bucket_policy;
mod get_bucket_request_payment;
mod head_bucket;
mod put_bucket_policy;
mod put_bucket_request_payment;

pub use get_bucket_location::BucketLocation;
pub use get_bucket_policy::BucketPolicy;
pub use get_bucket_request_payment::{BucketPayer, BucketRequestPayment};
pub use head_bucket::HeadBucketResult;
