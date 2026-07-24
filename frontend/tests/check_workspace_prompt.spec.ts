// 文件位置：frontend/tests/check_workspace_prompt.spec.ts
// 用途：验证需求 022「工作空间 Prompt」的后端 API 全链路：
//   1. PUT settings 带 system_prompt → GET 能拿到 system_prompt
//   2. PUT settings 不带 system_prompt → 既有 system_prompt 保持不变（增量语义）
//   3. PUT settings 带 system_prompt="" → 显式清空
//   4. workspace_settings.system_prompt 字段在 GET 响应中存在
//
// 走 APIRequestContext 直连后端 18088，不依赖页面渲染，
// 避免 React 状态/路由带来的 flake。

import { test, expect, type APIRequestContext } from '@playwright/test';

// dev embedded 模式直连端口
const BASE = 'http://localhost:18088';
// 默认测试 workspace（dev 启动会自动 seed workspace 1）
const WS_ID = 1;

interface WorkspaceSettings {
  workspace_id: number;
  default_response_type: 'todo' | 'loop' | 'executor';
  default_response_todo_id: number | null;
  default_response_loop_id: number | null;
  default_response_executor: string | null;
  system_prompt: string | null;
  updated_at: string | null;
}

async function getSettings(request: APIRequestContext): Promise<WorkspaceSettings> {
  const res = await request.get(`${BASE}/api/v1/workspaces/${WS_ID}/settings`);
  expect(res.ok(), `GET settings should return 2xx, got ${res.status()}`).toBeTruthy();
  return (await res.json()).data as WorkspaceSettings;
}

async function putSettings(request: APIRequestContext, body: Record<string, unknown>): Promise<void> {
  const res = await request.put(`${BASE}/api/v1/workspaces/${WS_ID}/settings`, { data: body });
  expect(res.ok(), `PUT settings should return 2xx, got ${res.status()}`).toBeTruthy();
}

test.describe('需求 022：工作空间 Prompt API 全链路', () => {
  // afterEach 兜底：用例结束后清空 system_prompt，避免污染下一个用例
  test.afterEach(async ({ request }) => {
    await putSettings(request, { system_prompt: '' });
  });

  test('GET settings 响应包含 system_prompt 字段', async ({ request }) => {
    const s = await getSettings(request);
    // 字段存在即可（值可能为 null、空串或非空）
    expect(s).toHaveProperty('system_prompt');
    console.log('GET settings 响应:', JSON.stringify(s, null, 2));
  });

  test('PUT 带 system_prompt 后 GET 能读到相同值', async ({ request }) => {
    const prompt = '## 工作空间共识\n- 产物目录：./target/release\n- 认证：token abc123';
    await putSettings(request, { system_prompt: prompt });

    const s = await getSettings(request);
    expect(s.system_prompt).toBe(prompt);
    console.log('写入后读回的 system_prompt:', s.system_prompt);
  });

  test('PUT 不带 system_prompt 时既有 prompt 保持不变（增量语义）', async ({ request }) => {
    // 先写入非空 prompt
    const prompt = '原有共识 prompt，不应被覆盖';
    await putSettings(request, { system_prompt: prompt });

    // 再 PUT 不带 system_prompt（仅改 default_response_type）
    await putSettings(request, { default_response_type: 'todo' });

    const s = await getSettings(request);
    expect(s.system_prompt).toBe(prompt);
    console.log('增量更新后 system_prompt 保持:', s.system_prompt);
  });

  test('PUT 带 system_prompt="" 显式清空', async ({ request }) => {
    // 先写入非空 prompt
    await putSettings(request, { system_prompt: '不应留存的 prompt' });

    // 再 PUT 空串清空
    await putSettings(request, { system_prompt: '' });

    const s = await getSettings(request);
    expect(s.system_prompt).toBe('');
    console.log('清空后 system_prompt:', JSON.stringify(s.system_prompt));
  });

  test('system_prompt 含多行 Markdown 与中文（Unicode 边界）', async ({ request }) => {
    const prompt = [
      '## 工作空间共识',
      '',
      '- 产物目录：编译输出放在 `./target/release`',
      '- 认证：访问内部 GitLab 用 token `glpat-xxxxxxxxxxxx`',
      '- 项目根：/Users/weibh/projects/rust/nothing-todo',
      '- 提交规范：使用 Conventional Commits，禁止 --no-verify',
      '',
      '### 禁止行为',
      '',
      '1. 不得直接 push 到 main 分支',
      '2. 不得跳过 PR review',
    ].join('\n');
    await putSettings(request, { system_prompt: prompt });

    const s = await getSettings(request);
    expect(s.system_prompt).toBe(prompt);
    console.log('多行 Unicode prompt 写入读回长度:', s.system_prompt?.length);
  });
});
