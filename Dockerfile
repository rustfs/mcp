# Copyright 2024 RustFS Team
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

FROM rust:1.93-bookworm AS builder

WORKDIR /build

ARG ENABLE_IO_URING=1
ENV RUSTFLAGS="--cfg tokio_unstable"
ENV RUSTDOCFLAGS="--cfg tokio_unstable"

# Copy project and build a reproducible release binary.
COPY . .
RUN if [ "$ENABLE_IO_URING" = "1" ]; then \
      cargo build --release --locked -p rustfs-mcp; \
    else \
      cargo build --release --locked --no-default-features -p rustfs-mcp; \
    fi

FROM debian:bookworm-slim AS runtime

WORKDIR /app

# Install certificates only, then clean apt metadata to keep the image small.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && update-ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/rustfs-mcp /app/rustfs-mcp

# Run as non-root in runtime container.
USER nobody

ENTRYPOINT ["/app/rustfs-mcp"]
