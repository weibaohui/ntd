# 多阶段构建：前端 + 后端
# 阶段 1：构建前端静态资源
FROM node:20-alpine AS frontend-builder
WORKDIR /app/frontend
COPY frontend/package*.json ./
RUN npm ci --legacy-peer-deps
COPY frontend/ ./
RUN npm run build

# 阶段 2：构建 Rust 后端
FROM rust:1.81-slim AS backend-builder
RUN apt-get update && apt-get install -y git && rm -rf /var/lib/apt/lists/*
WORKDIR /app/backend
# 复制 Cargo 配置和 lock 文件，建立依赖缓存层
COPY backend/Cargo.toml Cargo.lock ./
# 复制 build.rs（vergen-gitcl 需要 git 获取版本信息）
COPY backend/build.rs ./
# fetch 依赖（利用 Docker 层缓存）
RUN cargo fetch
# 复制全部源码（包括 .git 目录，build.rs 需要）
COPY backend/ ./
RUN cargo build --release

# 阶段 3：运行镜像
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=backend-builder /app/backend/target/release/ntd /usr/local/bin/
COPY --from=frontend-builder /app/frontend/dist /app/frontend/dist
ENV NTD_MODE=production
ENV RUST_LOG=info
EXPOSE 8088
CMD ["ntd", "daemon", "run"]