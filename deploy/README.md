# NTD 部署配置

本目录包含 NTD 应用的容器化部署配置。

## 目录结构

```
deploy/
├── docker/
│   ├── Dockerfile          # 多阶段构建镜像
│   └── .dockerignore       # Docker 构建忽略文件
├── docker-compose/
│   ├── docker-compose.yml      # 基础配置
│   ├── docker-compose.dev.yml  # 开发环境
│   ├── docker-compose.prod.yml # 生产环境
│   └── .env.example            # 环境变量模板
└── k8s/
    ├── deployment.yaml     # Kubernetes 部署配置
    ├── service.yaml        # 服务配置
    ├── pvc.yaml            # 持久化存储
    └── configmap.yaml      # 配置映射
```

## 快速开始

### Docker 构建

```bash
# 在项目根目录执行
docker build -f deploy/docker/Dockerfile -t ntd:latest .
```

### Docker Compose

```bash
# 开发环境
cd deploy/docker-compose
cp .env.example .env
docker compose -f docker-compose.dev.yml up -d

# 生产环境
docker compose -f docker-compose.prod.yml up -d
```

### Kubernetes

```bash
# 部署到 K8s 集群
kubectl apply -f deploy/k8s/

# 查看状态
kubectl get pods -l app=ntd
```

## 配置说明

- **端口**：默认 8088，可通过 `NTD_PORT` 环境变量修改
- **数据持久化**：Docker Compose 使用命名卷，K8s 使用 PVC
- **环境变量**：参考 `.env.example` 文件

## 注意事项

1. 首次部署需要构建镜像，耗时较长
2. 生产环境建议使用 `docker-compose.prod.yml` 或 K8s 配置
3. 数据目录 `/root/.ntd` 需要持久化存储