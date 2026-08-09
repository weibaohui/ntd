/**
 * 环节/环路标签功能测试
 *
 * 验证标签（Tag）替代原有 color 字段：
 * 1. 标签 CRUD
 * 2. 环路关联标签（PUT /loops/{id}/tags 全量替换 + 详情回显）
 * 3. 环路列表包含标签
 *
 * 注：044「loop-slim」重构后：
 * - 后端 loop 创建接口（POST /loops）已删（405），故「环路关联/列表含标签」改为
 *   复用 dev 库里已存在的 loop：先记下它原 tag_ids，PUT 新标签做断言，finally 还原，
 *   避免污染共享 dev 数据。
 * - step 实体已无 API（POST /api/steps、PUT /api/steps/{id}/tags 均 404），
 *   原「环节关联标签」用例已删除——无可用接口重写，留待 step API 恢复时补回。
 */

import { test, expect, type APIRequestContext } from '@playwright/test';

const BACKEND_URL = process.env.E2E_BACKEND_URL || 'http://localhost:18088';

// 生成唯一标签名，避免测试间冲突
const uniqueTagName = (prefix: string) =>
  `${prefix}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;

/**
 * 选一个可用于临时改标签的 loop：优先 tag_ids 为空的（减少对已有数据的干扰）。
 * 返回 {id, originalTagIds}；dev 库无 loop 时返回 null，调用方据此 test.skip。
 */
async function pickReusableLoop(request: APIRequestContext) {
  const res = await request.get(`${BACKEND_URL}/api/v1/workspaces/1/loops`);
  const json = await res.json();
  const loops: Array<{ id: number; tag_ids?: number[] }> = json.data ?? [];
  if (loops.length === 0) return null;
  // 优先无标签的 loop，最后兜底取第一个
  const target = loops.find((l) => (l.tag_ids ?? []).length === 0) ?? loops[0];
  return { id: target.id, originalTagIds: target.tag_ids ?? [] };
}

test.describe('环节/环路标签功能', () => {
  // 各用例在 finally 兜底清理，避免泄漏到后续测试。

  test('标签 CRUD', async ({ request }) => {
    // 验证标签的创建、列表包含、删除全流程
    const tagName = uniqueTagName('crud');
    let createdTagId: number | undefined;

    try {
      // 创建标签：传入名称和颜色，后端返回包含 id 的标签对象
      const createRes = await request.post(`${BACKEND_URL}/api/v1/tags`, {
        data: { name: tagName, color: '#ff6600' },
      });
      expect(createRes.ok()).toBeTruthy();
      const created = await createRes.json();
      createdTagId = created.data.id;
      // id 必须为正整数，说明后端正确写入数据库并返回自增主键
      expect(createdTagId).toBeGreaterThan(0);

      // 列表接口应该包含刚创建的标签 id，验证 C 和 R 链路通
      const listRes = await request.get(`${BACKEND_URL}/api/v1/tags`);
      expect(listRes.ok()).toBeTruthy();
      const tags = await listRes.json();
      const ids = tags.data.map((t: { id: number }) => t.id);
      expect(ids).toContain(createdTagId);

      // 删除标签
      const delRes = await request.delete(`${BACKEND_URL}/api/v1/tags/${createdTagId}`);
      expect(delRes.ok()).toBeTruthy();
      createdTagId = undefined; // 已清理，阻止 finally 重复删除
    } finally {
      if (createdTagId) await request.delete(`${BACKEND_URL}/api/v1/tags/${createdTagId}`);
    }
  });

  test('环路关联标签', async ({ request }) => {
    // 复用 dev 库已有 loop：记下原 tag_ids，PUT 新标签断言，finally 还原。
    const target = await pickReusableLoop(request);
    test.skip(!target, 'dev 库无可用 loop，跳过用例');
    let createdTagId: number | undefined;

    try {
      // 创建标签
      const tag = await (
        await request.post(`${BACKEND_URL}/api/v1/tags`, {
          data: { name: uniqueTagName('环路标签'), color: '#52c41a' },
        })
      ).json();
      createdTagId = tag.data.id;

      // PUT /loops/{id}/tags 全量替换为 [新标签]
      const putRes = await request.put(
        `${BACKEND_URL}/api/v1/workspaces/1/loops/${target!.id}/tags`,
        { data: { tag_ids: [createdTagId] } },
      );
      expect(putRes.ok()).toBeTruthy();
      const updated = await putRes.json();
      expect(updated.data.tag_ids).toContain(createdTagId);

      // 详情 GET 也应回显该标签，证明详情与 PUT 数据源一致
      const detail = await (
        await request.get(`${BACKEND_URL}/api/v1/workspaces/1/loops/${target!.id}`)
      ).json();
      expect(detail.data.tag_ids).toContain(createdTagId);
    } finally {
      if (createdTagId) await request.delete(`${BACKEND_URL}/api/v1/tags/${createdTagId}`);
      // 还原 loop 原标签，避免污染共享 dev 数据
      await request.put(`${BACKEND_URL}/api/v1/workspaces/1/loops/${target!.id}/tags`, {
        data: { tag_ids: target!.originalTagIds },
      });
    }
  });

  test('环路列表包含标签', async ({ request }) => {
    // 同样复用 dev 库已有 loop，验证列表接口的 tag_ids 字段正确回显。
    const target = await pickReusableLoop(request);
    test.skip(!target, 'dev 库无可用 loop，跳过用例');
    let createdTagId: number | undefined;

    try {
      // 创建标签
      const tag = await (
        await request.post(`${BACKEND_URL}/api/v1/tags`, {
          data: { name: uniqueTagName('列表标签'), color: '#722ed1' },
        })
      ).json();
      createdTagId = tag.data.id;

      // 关联标签
      const putRes = await request.put(
        `${BACKEND_URL}/api/v1/workspaces/1/loops/${target!.id}/tags`,
        { data: { tag_ids: [createdTagId] } },
      );
      expect(putRes.ok()).toBeTruthy();

      // 列表 GET /loops 应在对应 loop 项里回显标签（验证列表 N+1 修复后 tag_ids 正确）
      const listRes = await request.get(`${BACKEND_URL}/api/v1/workspaces/1/loops`);
      expect(listRes.ok()).toBeTruthy();
      const list = await listRes.json();
      const hit = list.data.find((l: { id: number }) => l.id === target!.id);
      expect(hit).toBeDefined();
      expect(hit.tag_ids).toContain(createdTagId);
    } finally {
      if (createdTagId) await request.delete(`${BACKEND_URL}/api/v1/tags/${createdTagId}`);
      await request.put(`${BACKEND_URL}/api/v1/workspaces/1/loops/${target!.id}/tags`, {
        data: { tag_ids: target!.originalTagIds },
      });
    }
  });
});
