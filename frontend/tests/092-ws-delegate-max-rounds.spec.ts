// 092-任务委派执行·护栏配置化：工作空间级「委派接力上限」默认 设置接口契约 e2e。
// 验证点（对应需求 092 护栏配置化 + Spec 评审「工作空间级编辑入口缺 Playwright 覆盖」）：
//  PUT /api/v1/workspaces/{ws}/settings 的 delegate_max_rounds 可写、可回读、effective 随之变化；
//  null=清除回退系统兜底（effective 回到 10）；越界(51)被端点拒 400 且不改既有值。
//  收尾恢复原值，避免污染开发库影响其它用例的 fallback 断言。
//
// 走 API 契约而非 UI 导航：ws 级持久化是后端行为，UI(DefaultResponseConfigPanel) 仅是
// InputNumber→PUT 的薄封装（同模式已由 task 级 config spec 覆盖）；API 路径最稳、直击新增的
// PUT handler + workspace_effective_max 解析 + null=清除语义——即评审指出的「缺覆盖」之实质。

import { test, expect, type APIRequestContext } from '@playwright/test';

const BASE = process.env.E2E_BASE_URL ?? 'http://localhost:18088';
const WS = 1;

// 整表保存（与 UI DefaultResponseConfigPanel.handleSave 同口径）：回读当前设置，仅覆盖 relay 上限后整体回写。
// 这样其余字段保持不变，避免误清 system_prompt / 默认响应等；也规避 serde 对缺失字段的严格校验。
async function putDelegateMax(request: APIRequestContext, max: number | null) {
  const cur = await fetchSettings(request);
  return request.put(`${BASE}/api/v1/workspaces/${WS}/settings`, {
    data: {
      default_response_type: cur.default_response_type,
      default_response_todo_id: cur.default_response_todo_id,
      default_response_loop_id: cur.default_response_loop_id,
      default_response_executor: cur.default_response_executor,
      system_prompt: cur.system_prompt,
      delegate_max_rounds: max,
    },
  });
}

// ApiResponse 把业务体包在 data 字段里（前端统一 unwrap）；这里取 .data 便于直接读字段。
async function fetchSettings(request: APIRequestContext) {
  const r = await request.get(`${BASE}/api/v1/workspaces/${WS}/settings`);
  expect(r.ok(), 'GET settings 应成功').toBeTruthy();
  const body = await r.json();
  return body.data ?? body;
}

test.describe('092 工作空间接力上限默认（API 契约）', () => {
  test('PUT delegate_max_rounds 可写持久、effective 跟随、null 清除回退兜底、越界 400', async ({ request }) => {
    // 记录原值用于收尾恢复，避免污染开发库。
    const before = await fetchSettings(request);
    const original: number | null = before.delegate_max_rounds ?? null;

    // 置 12 → raw 与 effective 均为 12（工作空间默认即生效）。
    let r = await putDelegateMax(request, 12);
    expect(r.ok(), 'PUT 12 应成功').toBeTruthy();
    let s = await fetchSettings(request);
    expect(s.delegate_max_rounds).toBe(12);
    expect(s.delegate_max_rounds_effective).toBe(12);

    // 越界 51 → 400（端点侧 validate_delegate_max_rounds），且不改既有值（仍 12）。
    r = await putDelegateMax(request, 51);
    expect(r.status(), 'PUT 51 应 400').toBe(400);
    s = await fetchSettings(request);
    expect(s.delegate_max_rounds, '越界请求不应改动既有值').toBe(12);

    // null=清除 → raw=null，effective 回退终极兜底常量 10。
    r = await putDelegateMax(request, null);
    expect(r.ok(), 'PUT null(清除) 应成功').toBeTruthy();
    s = await fetchSettings(request);
    expect(s.delegate_max_rounds).toBeNull();
    expect(s.delegate_max_rounds_effective, '清除后 effective 回退兜底 10').toBe(10);

    // 收尾：恢复原值，开发库自洽。
    await putDelegateMax(request, original);
  });
});
