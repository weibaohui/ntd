import axios from 'axios';

interface ApiResp<T> {
  code: number;
  data: T | null;
  message: string;
}

// api 对象必须最先初始化，避免其他函数引用时的 TDZ 错误
export const api = axios.create({
  baseURL: '',
  headers: { 'Content-Type': 'application/json' },
  timeout: 15000,
});

// ─── v1 路径重写拦截器 ─────────────────────────────────────────
//
// 后端已完成 K8s 风格的 v1 路由迁移（ADR-7），所有业务端点统一挂在 /api/v1/ 下。
// 为避免逐处手改约 144 个全局调用 + blackboard/wiki 嵌套路径，这里在请求拦截器中
// 统一把 /api/ 前缀重写为 /api/v1/。
//
// 排除项：
// - /api/events：WebSocket 升级端点，后端不版本化，保持原路径。
// - /api/v1/...：已是 v1 路径（手动迁移的 workspace-scoped 调用），跳过避免双前缀。
api.interceptors.request.use((config) => {
  const url = config.url;
  if (!url || typeof url !== 'string') return config;
  // WebSocket 升级端点不走版本化，原样放行
  if (url === '/api/events' || url.startsWith('/api/events?')) return config;
  // 已是 v1 路径，不重复重写（workspace-scoped 调用手写的 /api/v1/workspaces/...）
  if (url.startsWith('/api/v1/')) return config;
  // /api/ → /api/v1/，覆盖全部全局资源 + 已 path-nested 的 blackboard/wiki
  if (url.startsWith('/api/')) {
    config.url = '/api/v1/' + url.slice('/api/'.length);
  }
  return config;
});

/** Retry config: max 3 retries on network errors (no response), not on 4xx/5xx */
const MAX_RETRIES = 3;

api.interceptors.response.use(
  (res) => {
    if (typeof res.data !== 'object' || res.data === null || res.data instanceof Blob) {
      return res;
    }
    const body = res.data as ApiResp<unknown>;
    if (body && body.code !== 0) {
      return Promise.reject(new Error(body.message || `Error ${body.code}`));
    }
    return res;
  },
  async (error) => {
    if (!error.response && error.config) {
      const method = (error.config.method || 'get').toUpperCase();
      const isIdempotent = ['GET', 'HEAD', 'OPTIONS'].includes(method);
      if (isIdempotent) {
        const cfg = error.config as Record<string, unknown>;
        const retryCount = (cfg.__retryCount as number) || 0;
        if (retryCount < MAX_RETRIES) {
          cfg.__retryCount = retryCount + 1;
          const delay = Math.min(Math.pow(2, retryCount + 1) * 500, 8000) + Math.floor(Math.random() * 500);
          await new Promise(resolve => setTimeout(resolve, delay));
          return api(error.config);
        }
      }
    }

    if (error.response?.data?.message) {
      return Promise.reject(new Error(error.response.data.message));
    }
    return Promise.reject(error);
  },
);

export function unwrap<T>(res: { data: ApiResp<T> }): T {
  if (res.data.code !== 0) {
    throw new Error(res.data.message || `Error ${res.data.code}`);
  }
  if (res.data.data === null || res.data.data === undefined) {
    throw new Error(res.data.message || 'API 返回数据为空');
  }
  return res.data.data;
}
