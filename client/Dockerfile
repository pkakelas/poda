# Multi-stage build for all Poda services
FROM rust:latest as builder

WORKDIR /app

# Copy the entire workspace
COPY . .

# Build all binaries in one stage
RUN cargo build --release --bin dispencer && \
    cargo build --release --bin storage-provider && \
    cargo build --release --bin challenger

# Single runtime stage with all binaries
FROM rust:latest as runtime

WORKDIR /app

# Copy all binaries from builder
COPY --from=builder /app/target/release/dispencer /app/dispencer
COPY --from=builder /app/target/release/storage-provider /app/storage-provider
COPY --from=builder /app/target/release/challenger /app/challenger

# Create kzg directory and copy the ethereum ceremony data file
RUN mkdir -p /app/kzg
COPY --from=builder /app/kzg/ethereum_ceremony.json /app/kzg/ethereum_ceremony.json

# Create data directory for storage providers
RUN mkdir -p /data

# Create a non-root user
RUN useradd -r -s /bin/false app && chown -R app:app /app /data
USER app

EXPOSE 3000

# Default command (can be overridden in docker-compose)
CMD ["/app/dispencer"] 