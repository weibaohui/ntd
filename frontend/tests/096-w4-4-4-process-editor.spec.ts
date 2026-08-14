// 096-W4-4-4 工艺编辑器拆分冒烟。
// 验证 ProcessEditor 收口（useProcessEditorState + useProcessPersistence + useLeaveGuard
// + CollapsiblePropertyPanel + processEditorStyles）后的行为等价。
//
// 两层覆盖：
//  ① 渲染层——Toolbar（返回列表/保存/删除）+ 系统工艺 Alert + 可视化/YAML Tab + 属性面板标题；
//  ② 交互层（对应 101 设计文档验证门禁「编辑器打开/编辑属性/保存」）——
//     可视化↔YAML Tab 往返、属性面板收缩/展开、保存端到端
//     （复制系统工艺为用户副本 → 点保存走 PUT → 清理删除副本）。
// 收口前这些交互由主组件内联 state/handler/effect 驱动，收口后由 hook/子组件驱动，
// 本冒烟为「行为未变」的外层锚点。
import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

// 取一个系统工艺 guid：编辑器按 guid 寻址，系统工艺只读，用于渲染/Tab/收缩冒烟。
// 列表 API 响应可能是 {data:[...]} 或裸数组，两种都兼容。
async function pickSystemProcessGuid(request: import('@playwright/test').APIRequestContext): Promise<string> {
  const res = await request.get(`${BASE}/api/bundled/processes?is_system=true`);
  const body = await res.json();
  const list = body?.data ?? body;
  const guid = list?.[0]?.guid;
  if (!guid) throw new Error('dev 无系统工艺，冒烟前置失败');
  return guid;
}

// 工艺编辑器 tab 为全局配置（不按 workspace 归属），用例无需 pin selected_workspace。
test.describe('096-W4-4-4 工艺编辑器拆分冒烟', () => {
  test('渲染：Toolbar + 系统Alert + 可视化/YAML Tab + 属性面板标题', async ({ page, request }) => {
    const guid = await pickSystemProcessGuid(request);
    await page.goto(`${BASE}/#/processes?processMode=edit&guid=${guid}`);
    // 等「返回列表」出现 = useProcessEditorState 加载完成、主渲染就绪。
    await expect(page.getByRole('button', { name: /返回列表/ })).toBeVisible({ timeout: 10000 });
    // 系统工艺：保存按钮禁用、删除按钮不渲染（Toolbar 约定）。
    // 用 testid 圈定工具栏，避开属性面板内同名「删除」按钮。
    const toolbar = page.getByTestId('process-editor-toolbar');
    await expect(toolbar.getByRole('button', { name: /保存/ })).toBeDisabled();
    await expect(toolbar.getByRole('button', { name: /删除/ })).toHaveCount(0);
    // 系统 Alert 文案。
    await expect(page.getByText('这是系统工艺，编辑后会被同步覆盖')).toBeVisible();
    // 可视化 / YAML 两个 Tab 按钮。
    await expect(page.getByRole('button', { name: '可视化', exact: true })).toBeVisible();
    await expect(page.getByRole('button', { name: 'YAML', exact: true })).toBeVisible();
    // 属性面板标题：首屏未选节点 = 全局「工艺属性」。
    await expect(page.getByText('工艺属性').first()).toBeVisible();
  });

  test('交互：可视化↔YAML Tab 往返切换（activeTab 驱动 display）', async ({ page, request }) => {
    const guid = await pickSystemProcessGuid(request);
    await page.goto(`${BASE}/#/processes?processMode=edit&guid=${guid}`);
    await expect(page.getByRole('button', { name: /返回列表/ })).toBeVisible({ timeout: 10000 });

    // 切 YAML：Monaco 编辑器可见。
    await page.getByRole('button', { name: 'YAML', exact: true }).click();
    await expect(page.locator('.monaco-editor').first()).toBeVisible({ timeout: 8000 });

    // 切回可视化：Monaco 容器 display:none → 不可见（证明 activeTab 切换生效）。
    await page.getByRole('button', { name: '可视化', exact: true }).click();
    await expect(page.locator('.monaco-editor').first()).toBeHidden({ timeout: 8000 });
  });

  test('交互：属性面板收缩/展开往返（CollapsiblePropertyPanel 内部 state）', async ({ page, request }) => {
    const guid = await pickSystemProcessGuid(request);
    await page.goto(`${BASE}/#/processes?processMode=edit&guid=${guid}`);
    // 展开态：收起按钮可见。
    await expect(page.getByRole('button', { name: '收起属性面板' })).toBeVisible({ timeout: 10000 });

    await page.getByRole('button', { name: '收起属性面板' }).click();
    // 收起态：切到展开按钮（窄条）。
    await expect(page.getByRole('button', { name: '展开属性面板' })).toBeVisible();
    await expect(page.getByRole('button', { name: '收起属性面板' })).toHaveCount(0);

    await page.getByRole('button', { name: '展开属性面板' }).click();
    // 回到展开态。
    await expect(page.getByRole('button', { name: '收起属性面板' })).toBeVisible();
  });

  test('交互：保存端到端——复制系统工艺为用户副本→PUT保存→清理删除', async ({ page, request }) => {
    const sysGuid = await pickSystemProcessGuid(request);
    await page.goto(`${BASE}/#/processes?processMode=edit&guid=${sysGuid}`);
    // 系统 Alert 内的「复制到用户层后编辑」按钮（图标 Button，用子串正则）。
    await expect(page.getByRole('button', { name: /复制到用户层后编辑/ })).toBeVisible({ timeout: 10000 });

    // 复制 → handleCopyToUser 跳用户副本编辑器（URL 含新 guid）。
    // 直接从 copy-to-user 响应体取新 guid，避免 waitForURL 正则命中「跳转前旧 URL」的竞态。
    const [copyResp] = await Promise.all([
      page.waitForResponse(
        (r) => r.url().includes('/copy-to-user') && r.request().method() === 'POST',
      ),
      page.getByRole('button', { name: /复制到用户层后编辑/ }).click(),
    ]);
    expect(copyResp.ok()).toBeTruthy();
    const copyBody = await copyResp.json();
    const userGuid = copyBody?.data?.guid ?? copyBody?.guid;
    expect(userGuid).toBeTruthy();
    // 副本 guid 必须是新值，不等于原系统 guid。
    expect(userGuid).not.toBe(sysGuid);
    // 等编辑器 URL 落到副本 guid。
    await page.waitForURL((url) => url.hash.includes(`guid=${userGuid}`), { timeout: 10000 });

    try {
      await expect(page.getByRole('button', { name: /返回列表/ })).toBeVisible({ timeout: 10000 });
      // 用户工艺：系统 Alert 不再出现、保存按钮可用、删除按钮渲染。
      await expect(page.getByText('这是系统工艺，编辑后会被同步覆盖')).toHaveCount(0);
      const toolbar = page.getByTestId('process-editor-toolbar');
      await expect(toolbar.getByRole('button', { name: /保存/ })).toBeEnabled();
      await expect(toolbar.getByRole('button', { name: /删除/ })).toBeVisible();

      // 点保存 → useProcessPersistence.handleSave → PUT /api/v1/processes/{guid}。
      // 用户工艺保存按钮不依赖 isDirty（仅 isSaving||isSystem 禁用），故直接存当前 yamlText。
      const [resp] = await Promise.all([
        page.waitForResponse(
          (r) => r.url().includes('/api/v1/processes/') && r.request().method() === 'PUT',
        ),
        page.getByRole('button', { name: /保存/ }).click(),
      ]);
      expect(resp.ok()).toBeTruthy();
      await expect(page.getByText('工艺已保存')).toBeVisible({ timeout: 5000 });
    } finally {
      // 清理：删掉用户副本，避免污染 dev ~/.ntd/processes。
      await request.delete(`${BASE}/api/v1/processes/${userGuid}`);
    }
  });
});
