import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

test.describe('Workspace API (阶段5-6)', () => {
  test('workspace slash commands CRUD', async ({ page }) => {
    // 1. 创建一个 todo 作为 slash command 目标
    const todoResp = await page.request.post(`${BASE}/api/todos`, {
      data: {
        title: '测试 Slash 命令目标',
        prompt: '这是一个用于斜杠命令测试的 todo',
      },
    });
    expect(todoResp.ok()).toBeTruthy();
    const todoData = await todoResp.json();
    const todoId = todoData.data.id;
    console.log('创建 todo 成功, id:', todoId);

    const workspaceId = 1;

    // 2. 创建 workspace slash command
    const createResp = await page.request.post(`${BASE}/api/workspace/${workspaceId}/slash-commands`, {
      data: {
        slash_command: '/测试命令',
        todo_id: todoId,
        enabled: true,
      },
    });
    expect(createResp.ok()).toBeTruthy();
    const createData = await createResp.json();
    const cmdId = createData.data.id;
    console.log('创建斜杠命令成功, id:', cmdId);

    // 3. 获取 workspace slash commands 列表
    const listResp = await page.request.get(`${BASE}/api/workspace/${workspaceId}/slash-commands`);
    expect(listResp.ok()).toBeTruthy();
    const listData = await listResp.json();
    console.log('斜杠命令列表:', JSON.stringify(listData.data, null, 2));
    expect(listData.data.length).toBeGreaterThan(0);

    // 4. 更新 workspace slash command
    const updateResp = await page.request.put(
      `${BASE}/api/workspace/${workspaceId}/slash-commands/${cmdId}`,
      {
        data: {
          enabled: false,
        },
      }
    );
    expect(updateResp.ok()).toBeTruthy();
    console.log('更新斜杠命令成功');

    // 5. 删除 workspace slash command
    const deleteResp = await page.request.delete(
      `${BASE}/api/workspace/${workspaceId}/slash-commands/${cmdId}`
    );
    expect(deleteResp.ok()).toBeTruthy();
    console.log('删除斜杠命令成功');
  });

  test('workspace settings CRUD', async ({ page }) => {
    const workspaceId = 1;

    // 1. 先创建一个 todo 作为默认响应目标
    const todoResp = await page.request.post(`${BASE}/api/todos`, {
      data: {
        title: '默认响应目标 Todo',
        prompt: '这是工作空间的默认响应 todo',
      },
    });
    expect(todoResp.ok()).toBeTruthy();
    const todoData = await todoResp.json();
    const todoId = todoData.data.id;
    console.log('创建默认响应 todo 成功, id:', todoId);

    // 2. 获取 workspace settings
    const getResp = await page.request.get(`${BASE}/api/workspace/${workspaceId}/settings`);
    expect(getResp.ok()).toBeTruthy();
    const getData = await getResp.json();
    console.log('获取 workspace settings 成功:', JSON.stringify(getData.data, null, 2));

    // 3. 更新 workspace settings
    const updateResp = await page.request.put(`${BASE}/api/workspace/${workspaceId}/settings`, {
      data: {
        default_response_todo_id: todoId,
      },
    });
    expect(updateResp.ok()).toBeTruthy();
    console.log('更新 workspace settings 成功');

    // 4. 再次获取验证更新
    const verifyResp = await page.request.get(`${BASE}/api/workspace/${workspaceId}/settings`);
    expect(verifyResp.ok()).toBeTruthy();
    const verifyData = await verifyResp.json();
    expect(verifyData.data.default_response_todo_id).toBe(todoId);
    console.log('验证更新成功: default_response_todo_id =', verifyData.data.default_response_todo_id);
  });

  test('workspace slash command validation', async ({ page }) => {
    const workspaceId = 1;

    // 测试 slash_command 必须以 / 开头
    const badResp = await page.request.post(`${BASE}/api/workspace/${workspaceId}/slash-commands`, {
      data: {
        slash_command: 'bad_command', // 缺少 /
        todo_id: 1,
        enabled: true,
      },
    });
    expect(badResp.ok()).toBeFalsy();
    console.log('验证 slash_command 格式校验成功 (缺少 / 被拒绝)');
  });
});
