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
WORKDIR /app
# 复制 .git 目录（build.rs 需要 git 元数据获取版本信息）
COPY .git ./.git
# 复制前端构建产物（backend/src/lib.rs 的 Assets 嵌入需要 frontend/dist）
COPY --from=frontend-builder /app/frontend/dist ./frontend/dist
# 复制后端源码和构建配置
WORKDIR /app/backend
COPY backend/Cargo.toml Cargo.lock ./
COPY backend/build.rs ./
# fetch 依赖（利用 Docker 层缓存）
RUN cargo fetch
# 复制后端全部源码
COPY backend/ ./
RUN cargo build --release

# 阶段 3：运行镜像
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*
# 创建非 root 用户和组
RUN groupadd -r ntd && useradd -r -g ntd -u 1000 -d /home/ntd -m -s /bin/bash ntd
# 创建数据目录并设置权限
RUN mkdir -p /home/ntd/.ntd && chown -R ntd:ntd /home/ntd/.ntd
WORKDIR /app
COPY --from=backend-builder /app/backend/target/release/ntd /usr/local/bin/
COPY --from=frontend-builder /app/frontend/dist /app/frontend/dist
ENV NTD_MODE=production
ENV RUST_LOG=info
EXPOSE 8088
# 切换到非 root 用户运行
USER ntd
CMD ["ntd", "daemon", "run"]