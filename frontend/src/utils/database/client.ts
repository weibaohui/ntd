import axios from 'axios';

interface ApiResp<T> {
  code: number;
  data: T | null;
  message: string;
}

export async function checkBackendHealth(): Promise<boolean> {
  try {
    const res = await api.get('/health', { timeout: 3000 });
    return res.status === 200;
  } catch {
    return false;
  }
}

export const api = axios.create({
  baseURL: '',
  headers: { 'Content-Type': 'application/json' },
  timeout: 15000,
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
