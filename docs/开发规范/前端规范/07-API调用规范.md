# 前端规范 07：API 调用规范

> 定义后端接口调用规范。

---

## 1. HTTP 客户端

使用 `fetch` API 封装在 `src/api/client.ts` 中：

```ts
// HTTP 客户端统一管理 baseURL、请求头、错误处理。
// 错误响应统一抛出自定义错误类型，调用方通过 try-catch 或 error 状态处理。
const BASE_URL = '/api/v1';

export async function request<T>(path: string, options?: RequestInit): Promise<T> {
  const response = await fetch(`${BASE_URL}${path}`, {
    headers: {
      'Content-Type': 'application/json',
      ...options?.headers,
    },
    ...options,
  });

  if (!response.ok) {
    // 将 HTTP 错误统一封装为包含状态码和错误消息的 ApiError，
    // 避免在每个调用方重复处理 4xx/5xx 的差异。
    const error = await response.json().catch(() => ({ error: response.statusText }));
    throw new ApiError(response.status, error.error);
  }

  return response.json();
}
```

---

## 2. API 模块

每个资源模块一个文件：

```ts
// api/todos.ts
// 每个 API 模块导出类型安全的函数，接收明确参数、返回明确类型。
// 不暴露 request 的直接调用细节给业务组件。
import { request } from './client';
import type { Todo, PaginatedResponse } from '@/types/todo';

export function fetchTodos(params: { page: number; limit: number }) {
  return request<PaginatedResponse<Todo>>(`/todos?page=${params.page}&limit=${params.limit}`);
}

export function fetchTodo(id: string) {
  return request<Todo>(`/todos/${id}`);
}

export function createTodo(data: { title: string; description?: string }) {
  return request<Todo>('/todos', {
    method: 'POST',
    body: JSON.stringify(data),
  });
}
```

---

## 3. 错误处理

```tsx
// 在组件中捕获 API 错误，转换为 UI 状态：
// - 将 error 状态传递给 UI 组件（如 antd Alert / message.error）
// - 不吞没错误：用户应能感知到"请求失败"
const { data, error } = useFetch<Todo>(`/todos/${id}`);

if (error) {
  return <Alert type="error" message={error.message} />;
}
```

---

## 4. 禁止行为

- ❌ 在组件中直接使用 `fetch` 或 `axios`（通过 `api/` 模块统一调用）
- ❌ 将 API URL 硬编码在组件中（通过 `api/client.ts` 的 BASE_URL 管理）
- ❌ 忽略 API 错误（空 catch 块）
