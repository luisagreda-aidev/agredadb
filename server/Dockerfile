# Stage 1: Builder (Heavy)
FROM rust:1.75-bullseye as builder

# Instalar compilador de Protobuf
RUN apt-get update && apt-get install -y protobuf-compiler libclang-dev

WORKDIR /app
COPY . .

# Compilación optimizada
RUN cargo build --release

# Stage 2: Runtime (Lightweight)
FROM debian:bullseye-slim

# Dependencias mínimas para io_uring y SSL
RUN apt-get update && apt-get install -y \
    libssl1.1 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/local/bin
COPY --from=builder /app/target/release/agredadb_server .

# Puerto gRPC
EXPOSE 19999

# Comando de arranque
CMD ["./agredadb_server"]
