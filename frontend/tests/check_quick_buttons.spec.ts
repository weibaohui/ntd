import { test, expect, type APIRequestContext } from '@playwright/test';

// 直连 embedded dev 服务（前后端同源），不走 vite 5173
const BASE = 'http://localhost:18088';
const NAME = 'PW测试按钮';
const NAME_RENAMED = 'PW测试按钮改名';

interface QuickButton {
  id: number;
  button_name: string;
  prompt_text: string;
}

// 后端响应是 ApiResponse 包裹的 { code, data, message }；这里只取 data
async function listButtons(request: APIRequestContext): Promise<QuickButton[]> {
  const res = await request.get(`${BASE}/api/v1/quick-buttons`);
  expect(res.ok()).toBeTruthy();
  return (await res.json()).data as QuickButton[];
}

// 按名清理，保证用例间独立、不污染全局数据
async function deleteByName(request: APIRequestContext, name: string) {
  for (const b of await listButtons(request)) {
    if (b.button_name === name) {
      await request.delete(`${BASE}/api/v1/quick-buttons/${b.id}`);
    }
  }
}

// 验证 quick_buttons 全链路：DB 迁移建表 → 路由 → handler CRUD → 校验。
// 用 APIRequestContext 直接打 HTTP，不依赖页面渲染（ReplyInput 需 resume record）。
test.describe('快捷话术按钮 API 全链路', () => {
  test.beforeEach(async ({ request }) => {
    await deleteByName(request, NAME);
    await deleteByName(request, NAME_RENAMED);
  });

  // afterEach 兜底：用例中途失败（如断言前抛错）时也清理共享数据，避免污染全局 quick_buttons 表
  test.afterEach(async ({ request }) => {
    await deleteByName(request, NAME);
    await deleteByName(request, NAME_RENAMED);
  });

  test('创建 → 列出 → 改名改话术 → 删除', async ({ request }) => {
    const createRes = await request.post(`${BASE}/api/v1/quick-buttons`, {
      data: { button_name: NAME, prompt_text: '原始话术' },
    });
    expect(createRes.status()).toBe(200);
    const id = (await createRes.json()).data.id;
    expect(typeof id).toBe('number');

    expect((await listButtons(request)).find((b) => b.id === id)?.button_name).toBe(NAME);

    const updRes = await request.put(`${BASE}/api/v1/quick-buttons/${id}`, {
      data: { button_name: NAME_RENAMED, prompt_text: '新话术' },
    });
    expect(updRes.ok()).toBeTruthy();

    const updated = (await listButtons(request)).find((b) => b.id === id);
    expect(updated?.button_name).toBe(NAME_RENAMED);
    expect(updated?.prompt_text).toBe('新话术');

    expect((await request.delete(`${BASE}/api/v1/quick-buttons/${id}`)).ok()).toBeTruthy();
    expect((await listButtons(request)).find((b) => b.id === id)).toBeUndefined();
  });

  test('重名创建被拒（400）', async ({ request }) => {
    expect(
      (
        await request.post(`${BASE}/api/v1/quick-buttons`, {
          data: { button_name: NAME, prompt_text: 'x' },
        })
      ).status(),
    ).toBe(200);
    const dup = await request.post(`${BASE}/api/v1/quick-buttons`, {
      data: { button_name: NAME, prompt_text: 'y' },
    });
    expect(dup.status()).toBe(400);
  });

  test('空名称被拒（400）', async ({ request }) => {
    const res = await request.post(`${BASE}/api/v1/quick-buttons`, {
      data: { button_name: '   ', prompt_text: 'x' },
    });
    expect(res.status()).toBe(400);
  });
});
