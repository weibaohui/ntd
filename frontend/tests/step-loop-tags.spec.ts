/**
 * 环节/环路标签功能测试
 *
 * 验证环节和环路使用标签（Tag）替代原有的 color 字段：
 * 1. 标签 CRUD
 * 2. 环节关联标签
 * 3. 环路关联标签
 */

import { test, expect } from '@playwright/test';

const BACKEND_URL = process.env.E2E_BACKEND_URL || 'http://localhost:18088';

// 生成唯一标签名，避免测试间冲突
const uniqueTagName = (prefix: string) => `${prefix}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;

test.describe('环节/环路标签功能', () => {
  // 注意：各测试用例各自管理自己的标签/环节/环路资源清理，
  // 使用 try/finally 确保即使断言失败也能释放资源，避免泄漏到后续测试。

  test('标签 CRUD', async ({ page }) => {
    // 验证标签的创建、列表包含、删除全流程
    const tagName = uniqueTagName('crud');
    let createdTagId: number | undefined;

    try {
      // 创建标签：传入名称和颜色，后端返回包含 id 的标签对象
      const createRes = await page.request.post(`${BACKEND_URL}/api/tags`, {
        data: { name: tagName, color: '#ff6600' },
      });
      expect(createRes.ok()).toBeTruthy();
      const created = await createRes.json();
      createdTagId = created.data.id;
      // id 必须为正整数，说明后端正确写入数据库并返回自增主键
      expect(createdTagId).toBeGreaterThan(0);

      // 列表接口应该包含刚创建的标签 id，验证 C 和 R 链路通
      const listRes = await page.request.get(`${BACKEND_URL}/api/tags`);
      expect(listRes.ok()).toBeTruthy();
      const tags = await listRes.json();
      const ids = tags.data.map((t: any) => t.id);
      expect(ids).toContain(createdTagId);

      // 删除标签
      const delRes = await page.request.delete(`${BACKEND_URL}/api/tags/${createdTagId}`);
      expect(delRes.ok()).toBeTruthy();
      // 删除后 createdTagId 标记已清理，阻止 finally 块重复删除
      createdTagId = undefined;
    } finally {
      // 如果中途失败导致 createdTagId 未被清理，在 finally 中兜底释放资源
      if (createdTagId) {
        await page.request.delete(`${BACKEND_URL}/api/tags/${createdTagId}`);
      }
    }
  });

  test('环节关联标签', async ({ page }) => {
    // 验证步骤：创建标签→创建环节（新环节无标签）→关联标签→验证标签持久化
    let createdTagId: number | undefined;
    let createdStepId: number | undefined;

    try {
      // 创建标签：用唯一名称防止测试间冲突
      const tagRes = await page.request.post(`${BACKEND_URL}/api/tags`, {
        data: { name: uniqueTagName('环节标签'), color: '#1890ff' },
      });
      expect(tagRes.ok()).toBeTruthy();
      const tag = await tagRes.json();
      createdTagId = tag.data.id;

      // 创建环节（直建），新环节的 tag_ids 应为空数组
      const stepRes = await page.request.post(`${BACKEND_URL}/api/steps`, {
        data: { title: uniqueTagName('测试环节'), prompt: 'test prompt' },
      });
      expect(stepRes.ok()).toBeTruthy();
      const step = await stepRes.json();
      createdStepId = step.data.id;
      // 新创建的环节不应该有预绑定标签，验证 tag_ids 默认行为
      expect(step.data.tag_ids).toEqual([]);

      // 通过 PUT /api/steps/{id}/tags 关联标签（全量替换）
      const updateTagsRes = await page.request.put(`${BACKEND_URL}/api/steps/${createdStepId}/tags`, {
        data: { tag_ids: [createdTagId] },
      });
      expect(updateTagsRes.ok()).toBeTruthy();
      const updated = await updateTagsRes.json();
      // 验证标签已持久化到环节
      expect(updated.data.tag_ids).toContain(createdTagId);
    } finally {
      // 资源清理：无论测试成功与否都释放标签和环节
      if (createdTagId) await page.request.delete(`${BACKEND_URL}/api/tags/${createdTagId}`);
      if (createdStepId) await page.request.delete(`${BACKEND_URL}/api/steps/${createdStepId}`);
    }
  });

  test('环路关联标签', async ({ page }) => {
    // 验证步骤：创建标签→创建环路→关联标签→验证详情接口也包含标签
    let createdTagId: number | undefined;
    let createdLoopId: number | undefined;

    try {
      // 创建标签
      const tagRes = await page.request.post(`${BACKEND_URL}/api/tags`, {
        data: { name: uniqueTagName('环路标签'), color: '#52c41a' },
      });
      expect(tagRes.ok()).toBeTruthy();
      const tag = await tagRes.json();
      createdTagId = tag.data.id;

      // 创建环路，新环路初始 tag_ids 应为空
      const loopRes = await page.request.post(`${BACKEND_URL}/api/loops`, {
        data: { name: uniqueTagName('测试环路') },
      });
      expect(loopRes.ok()).toBeTruthy();
      const loop = await loopRes.json();
      createdLoopId = loop.data.id;
      expect(loop.data.tag_ids).toEqual([]);

      // 通过 PUT /api/loops/{id}/tags 关联标签
      const updateTagsRes = await page.request.put(`${BACKEND_URL}/api/loops/${createdLoopId}/tags`, {
        data: { tag_ids: [createdTagId] },
      });
      expect(updateTagsRes.ok()).toBeTruthy();
      const updated = await updateTagsRes.json();
      expect(updated.data.tag_ids).toContain(createdTagId);

      // 验证环路详情 GET /api/loops/{id} 也返回标签，确保详情与列表数据源一致
      const detailRes = await page.request.get(`${BACKEND_URL}/api/loops/${createdLoopId}`);
      expect(detailRes.ok()).toBeTruthy();
      const detail = await detailRes.json();
      expect(detail.data.tag_ids).toContain(createdTagId);
    } finally {
      // 资源清理：先删标签再删环路，避免外键约束问题
      if (createdTagId) await page.request.delete(`${BACKEND_URL}/api/tags/${createdTagId}`);
      if (createdLoopId) await page.request.delete(`${BACKEND_URL}/api/loops/${createdLoopId}`);
    }
  });

  test('环路列表包含标签', async ({ page }) => {
    // 验证步骤：创建标签→创建环路→关联标签→通过列表接口验证标签存在
    let createdTagId: number | undefined;
    let createdLoopId: number | undefined;

    try {
      // 创建标签
      const tagRes = await page.request.post(`${BACKEND_URL}/api/tags`, {
        data: { name: uniqueTagName('列表标签'), color: '#722ed1' },
      });
      expect(tagRes.ok()).toBeTruthy();
      const tag = await tagRes.json();
      createdTagId = tag.data.id;

      // 创建环路
      const loopRes = await page.request.post(`${BACKEND_URL}/api/loops`, {
        data: { name: uniqueTagName('列表测试环路') },
      });
      expect(loopRes.ok()).toBeTruthy();
      const loop = await loopRes.json();
      createdLoopId = loop.data.id;

      // 关联标签
      const updateTagsRes = await page.request.put(`${BACKEND_URL}/api/loops/${createdLoopId}/tags`, {
        data: { tag_ids: [createdTagId] },
      });
      expect(updateTagsRes.ok()).toBeTruthy();

      // 列表接口 GET /api/loops 应包含该环路的标签信息
      const listRes = await page.request.get(`${BACKEND_URL}/api/loops`);
      expect(listRes.ok()).toBeTruthy();
      const list = await listRes.json();
      const target = list.data.find((l: any) => l.id === createdLoopId);
      expect(target).toBeDefined();
      // 验证列表项中包含关联的标签 ID，确保列表接口 N+1 修复后标签数据正确
      expect(target.tag_ids).toContain(createdTagId);
    } finally {
      // 资源清理
      if (createdTagId) await page.request.delete(`${BACKEND_URL}/api/tags/${createdTagId}`);
      if (createdLoopId) await page.request.delete(`${BACKEND_URL}/api/loops/${createdLoopId}`);
    }
  });
});
