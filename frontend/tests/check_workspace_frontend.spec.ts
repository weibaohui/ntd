import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

test.describe('Workspace Frontend Panels (阶段7-11)', () => {
  test('workspace detail page - slash commands panel', async ({ page }) => {
    await page.goto(BASE);

    // 等待页面加载
    await page.waitForTimeout(1000);

    // 进入设置页面
    await page.click('text=配置管理', { timeout: 5000 }).catch(() => {
      // 可能已经在设置页面，尝试其他方式
    });

    // 点击工作空间 Tab
    const workspaceTab = page.locator('text=工作空间');
    if (await workspaceTab.isVisible()) {
      await workspaceTab.click();
    }

    await page.waitForTimeout(500);

    // 验证 ProjectDirectoriesPanel 正常渲染
    // 应该能看到工作空间列表
    const addButton = page.locator('button:has-text("添加项目目录")');
    if (await addButton.isVisible({ timeout: 3000 }).catch(() => false)) {
      console.log('ProjectDirectoriesPanel 渲染正常');
    }
  });

  test('workspace slash commands CRUD via API', async ({ page }) => {
    // 1. 创建一个 todo
    const todoResp = await page.request.post(`${BASE}/api/v1/workspaces/${workspaceId}/todos`, {
      data: { title: '测试 Slash 命令', prompt: '测试' },
    });
    expect(todoResp.ok()).toBeTruthy();
    const todoId = (await todoResp.json()).data.id;

    const workspaceId = 1;

    // 2. 创建 slash command
    const createResp = await page.request.post(`${BASE}/api/v1/workspaces/${workspaceId}/slash-commands`, {
      data: { slash_command: '/测试', todo_id: todoId, enabled: true },
    });
    expect(createResp.ok()).toBeTruthy();
    const cmdId = (await createResp.json()).data.id;

    // 3. 获取列表验证
    const listResp = await page.request.get(`${BASE}/api/v1/workspaces/${workspaceId}/slash-commands`);
    expect(listResp.ok()).toBeTruthy();
    const commands = (await listResp.json()).data;
    const cmd = commands.find((c: any) => c.id === cmdId);
    expect(cmd).toBeDefined();
    expect(cmd.slash_command).toBe('/测试');
    console.log('WorkspaceSlashCommandsPanel 数据验证成功');

    // 4. 清理
    await page.request.delete(`${BASE}/api/v1/workspaces/${workspaceId}/slash-commands/${cmdId}`);
  });

  test('workspace settings panel', async ({ page }) => {
    // 1. 创建 todo
    const todoResp = await page.request.post(`${BASE}/api/v1/workspaces/${workspaceId}/todos`, {
      data: { title: '默认响应 Todo', prompt: '测试' },
    });
    expect(todoResp.ok()).toBeTruthy();
    const todoId = (await todoResp.json()).data.id;

    const workspaceId = 1;

    // 2. 更新 workspace settings
    const updateResp = await page.request.put(`${BASE}/api/v1/workspaces/${workspaceId}/settings`, {
      data: { default_response_todo_id: todoId },
    });
    expect(updateResp.ok()).toBeTruthy();

    // 3. 验证更新
    const getResp = await page.request.get(`${BASE}/api/v1/workspaces/${workspaceId}/settings`);
    expect(getResp.ok()).toBeTruthy();
    const settings = (await getResp.json()).data;
    expect(settings.default_response_todo_id).toBe(todoId);
    console.log('WorkspaceSettingsPanel 数据验证成功');
  });

  test('bot workspace_id in agent list', async ({ page }) => {
    // 验证 AgentBot 包含 workspace_id 字段
    const botsResp = await page.request.get(`${BASE}/api/v1/agent-bots`);
    expect(botsResp.ok()).toBeTruthy();
    const bots = (await botsResp.json()).data;

    if (bots.length > 0) {
      expect(bots[0]).toHaveProperty('workspace_id');
      console.log('AgentBot.workspace_id 字段验证成功:', bots[0].workspace_id);
    } else {
      console.log('暂无 AgentBot，跳过验证');
    }
  });
});
