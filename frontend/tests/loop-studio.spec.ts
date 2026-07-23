/**
 * Loop Studio API 测试
 *
 * 通过直接调用 API 验证后端功能，避免前端渲染差异。
 */

import { test, expect } from '@playwright/test';

const BACKEND_URL = process.env.E2E_BACKEND_URL || 'http://localhost:18088';

test.describe('Loop Studio API', () => {
  test('API 正常响应', async ({ page }) => {
    const res = await page.request.get(`${BACKEND_URL}/api/v1/workspaces/1/loops`);
    expect(res.ok()).toBeTruthy();
  });

  test('新建 loop → 详情 → 删除', async ({ page }) => {
    // 新建
    const createRes = await page.request.post(`${BACKEND_URL}/api/v1/workspaces/1/loops`, {
      data: { name: 'playwright-test-loop' },
    });
    expect(createRes.ok()).toBeTruthy();
    const created = await createRes.json();
    const loopId = created.data.id;
    expect(loopId).toBeGreaterThan(0);

    // 详情
    const detailRes = await page.request.get(`${BACKEND_URL}/api/v1/workspaces/1/loops/${loopId}`);
    expect(detailRes.ok()).toBeTruthy();

    // 列表包含
    const listRes = await page.request.get(`${BACKEND_URL}/api/v1/workspaces/1/loops`);
    const list = await listRes.json();
    const ids = list.data.map((l: any) => l.id);
    expect(ids).toContain(loopId);

    // 删除
    const delRes = await page.request.delete(`${BACKEND_URL}/api/v1/workspaces/1/loops/${loopId}`);
    expect(delRes.ok()).toBeTruthy();
  });

  test('创建环节（promote）', async ({ page }) => {
    // 先建一个 todo
    const todoRes = await page.request.post(`${BACKEND_URL}/api/v1/workspaces/1/todos`, {
      data: { title: 'promote-test-todo', prompt: 'test prompt' },
    });
    expect(todoRes.ok()).toBeTruthy();
    const todo = await todoRes.json();
    const todoId = todo.data.id;

    // promote
    const promoteRes = await page.request.post(`${BACKEND_URL}/api/v1/workspaces/1/todos/${todoId}/promote`);
    expect(promoteRes.ok()).toBeTruthy();
    const step = await promoteRes.json();
    expect(step.data.title).toBe('promote-test-todo');
    expect(step.data.prompt).toBe('test prompt');
    expect(step.data.source_todo_id).toBe(todoId);

    // steps 列表包含
    const stepsRes = await page.request.get(`${BACKEND_URL}/api/steps`);
    const steps = await stepsRes.json();
    const stepIds = steps.data.map((s: any) => s.id);
    expect(stepIds).toContain(step.data.id);
  });

  test('loop 添加环节', async ({ page }) => {
    // 建 loop
    const loopRes = await page.request.post(`${BACKEND_URL}/api/v1/workspaces/1/loops`, {
      data: { name: 'stage-test-loop' },
    });
    const loop = await loopRes.json();
    const loopId = loop.data.id;

    // 建 todo → promote 为 step
    const todoRes = await page.request.post(`${BACKEND_URL}/api/v1/workspaces/1/todos`, {
      data: { title: 'stage-step', prompt: 'do something' },
    });
    const todo = await todoRes.json();
    const promoteRes = await page.request.post(`${BACKEND_URL}/api/v1/workspaces/1/todos/${todo.data.id}/promote`);
    const step = await promoteRes.json();

    // 给 loop 添加 stage
    const stageRes = await page.request.post(`${BACKEND_URL}/api/v1/workspaces/1/loops/${loopId}/stages`, {
      data: { name: '我的环节', todo_id: step.data.id },
    });
    expect(stageRes.ok()).toBeTruthy();
    const stage = await stageRes.json();
    expect(stage.data.name).toBe('我的环节');

    // 验证 stages 列表
    const stagesRes = await page.request.get(`${BACKEND_URL}/api/v1/workspaces/1/loops/${loopId}/stages`);
    const stages = await stagesRes.json();
    expect(stages.data.length).toBeGreaterThanOrEqual(1);

    // 清理
    await page.request.delete(`${BACKEND_URL}/api/v1/workspaces/1/loops/${loopId}`);
  });
});
