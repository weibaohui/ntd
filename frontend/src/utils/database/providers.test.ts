// providers Facade 单元测试。
//
// vi.mock 替换 client 模块的 api/unwrap：断言每个 Facade 函数以正确的
// method/URL/body 调用 axios、返回值经 unwrap 流回、错误能透传（覆盖设计 §5
// 的假成功修复）。exportProviders 单独验证它走 text 通路、不经过 unwrap。

import { describe, it, expect, beforeEach, vi } from 'vitest';
import type { Mock } from 'vitest';

// vi.mock 提升到文件顶部执行，确保 providers.ts 导入的 client 是 mock 版。
vi.mock('@/utils/database/client', () => ({
  api: {
    get: vi.fn(),
    post: vi.fn(),
    put: vi.fn(),
    delete: vi.fn(),
  },
  unwrap: vi.fn(),
}));

// @/ 绝对路径块须连续：client 是被 mock 的实例、ProviderInput 是其入参类型契约，
// 两者同属 @/ 块紧邻，再接 ./ 相对导入（前端 @/ 导入规范）。
import { api, unwrap } from '@/utils/database/client';
import type { ProviderInput } from '@/types/provider';
import {
  listProviders,
  getSupportedExecutors,
  getProvider,
  createProvider,
  updateProvider,
  deleteProvider,
  exportProviders,
  importProviders,
  previewProviderToExecutors,
  applyProviderToExecutors,
} from './providers';

// TS 仍按真实 axios 类型看 api.*，这里统一转成 vitest Mock 以便断言（不经 any）。
const mockGet = api.get as unknown as Mock;
const mockPost = api.post as unknown as Mock;
const mockPut = api.put as unknown as Mock;
const mockDelete = api.delete as unknown as Mock;
const mockUnwrap = unwrap as unknown as Mock;

// 还原真实 unwrap 语义：取 res.data.data。错误分支的用例会临时改写为抛错。
const unwrapData = (res: { data: { data: unknown } }) => res.data.data;

beforeEach(() => {
  vi.clearAllMocks();
  mockUnwrap.mockImplementation(unwrapData);
});

describe('providers Facade', () => {
  const INPUT: ProviderInput = {
    display_name: 'DeepSeek',
    api_key: 'sk-x',
    base_url: 'https://a/v1',
    protocol: 'openai',
    models: [],
  };

  it('test_listProviders_走GET并解包data数组', async () => {
    mockGet.mockResolvedValue({ data: { data: [{ name: 'a' }] } });
    await expect(listProviders()).resolves.toEqual([{ name: 'a' }]);
    expect(mockGet).toHaveBeenCalledWith('/api/v1/providers');
  });

  it('test_getSupportedExecutors_走supported_executors端点', async () => {
    mockGet.mockResolvedValue({ data: { data: [] } });
    await getSupportedExecutors();
    expect(mockGet).toHaveBeenCalledWith('/api/v1/providers/supported-executors');
  });

  it('test_getProvider_对name做encodeURIComponent', async () => {
    mockGet.mockResolvedValue({ data: { data: { name: 'a b/c' } } });
    await getProvider('a b/c');
    expect(mockGet).toHaveBeenCalledWith('/api/v1/providers/a%20b%2Fc');
  });

  it('test_createProvider_POST_body到根路径', async () => {
    mockPost.mockResolvedValue({});
    await createProvider(INPUT);
    expect(mockPost).toHaveBeenCalledWith('/api/v1/providers', INPUT);
  });

  it('test_updateProvider_PUT_编码name并带body', async () => {
    mockPut.mockResolvedValue({});
    await updateProvider('a b', INPUT);
    expect(mockPut).toHaveBeenCalledWith('/api/v1/providers/a%20b', INPUT);
  });

  it('test_deleteProvider_DELETE_编码name', async () => {
    mockDelete.mockResolvedValue({});
    await deleteProvider('a b');
    expect(mockDelete).toHaveBeenCalledWith('/api/v1/providers/a%20b');
  });

  it('test_importProviders_POST_yaml与strategy并解包', async () => {
    mockPost.mockResolvedValue({ data: { data: { imported: ['x'], errors: [] } } });
    await expect(importProviders('yaml: ...', 'replace')).resolves.toEqual({ imported: ['x'], errors: [] });
    expect(mockPost).toHaveBeenCalledWith('/api/v1/providers/import', { yaml: 'yaml: ...', strategy: 'replace' });
  });

  it('test_previewProviderToExecutors_POST_executor_models到preview', async () => {
    mockPost.mockResolvedValue({ data: { data: [] } });
    await previewProviderToExecutors('p', { codex: 'deepseek-chat' });
    expect(mockPost).toHaveBeenCalledWith('/api/v1/providers/p/preview', { executor_models: { codex: 'deepseek-chat' } });
  });

  it('test_applyProviderToExecutors_POST_executor_models到apply', async () => {
    mockPost.mockResolvedValue({ data: { data: { applied: [], errors: [] } } });
    await applyProviderToExecutors('p', { codex: 'm' });
    expect(mockPost).toHaveBeenCalledWith('/api/v1/providers/p/apply', { executor_models: { codex: 'm' } });
  });

  it('test_exportProviders_走text通路不被unwrap', async () => {
    mockGet.mockResolvedValue({ data: 'yaml-content' });
    await expect(exportProviders()).resolves.toBe('yaml-content');
    expect(mockGet).toHaveBeenCalledWith('/api/v1/providers/export', { responseType: 'text' });
    // 文本端点直接返回 res.data，绝不能进 unwrap（unwrap 期望 {code,data,message}）。
    expect(mockUnwrap).not.toHaveBeenCalled();
  });

  it('test_createProvider_后端拒绝时透传错误不假成功', async () => {
    // 设计 §5 修复：createProvider 不走 unwrap，依赖 client.ts 拦截器把 code!==0/非 2xx
    // 转成 reject。Facade 必须把这个 reject 透传出去，组件才能落到 catch 显示「操作失败」，
    // 而不是原先的不校验响应直接 message.success 假成功。
    mockPost.mockRejectedValue(new Error('Provider already exists'));
    await expect(createProvider(INPUT)).rejects.toThrow('Provider already exists');
  });

  it('test_getProvider_unwrap抛错时透传', async () => {
    // 走 unwrap 的函数同理：业务错（code!==0）经 unwrap 抛出，Facade 透传。
    mockGet.mockResolvedValue({ data: { data: null } });
    mockUnwrap.mockImplementation(() => {
      throw new Error('not found');
    });
    await expect(getProvider('x')).rejects.toThrow('not found');
  });
});
