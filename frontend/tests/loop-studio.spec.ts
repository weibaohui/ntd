/**
 * Loop Studio API 测试
 *
 * 通过直接调用 API 验证后端功能，避免前端渲染差异。
 *
 * 注：044「loop-slim」重构后，后端仅保留 loop 的只读 + 运行态接口——
 * 创建/更新/复制 loop、promote todo 为 step、stage CRUD 等写入接口均已删除
 *（见 backend/src/handlers/loop_.rs 的接口收口注释：POST /loops→405、
 *  POST /todos/{id}/promote→404、POST /loops/{id}/stages→404）。
 * 故原有的「新建 loop→详情→删除」「创建环节(promote)」「loop 添加环节」
 * 三个用例已移除（依赖已删 API，无可重写的现状）；这里只保留只读的「列表可达」冒烟。
 * 若后续恢复这些写入接口，再补回对应用例。
 */

import { test, expect } from '@playwright/test';

const BACKEND_URL = process.env.E2E_BACKEND_URL || 'http://localhost:18088';

test.describe('Loop Studio API', () => {
  test('loops 列表接口可达（只读冒烟）', async ({ request }) => {
    // 044 后 loop 写入接口已删，只保留 GET 列表/详情；这里冒烟验证只读链路通。
    const res = await request.get(`${BACKEND_URL}/api/v1/workspaces/1/loops`);
    expect(res.ok()).toBeTruthy();
  });
});
