// 专家贡献功能 UI 验证（需求 026，PAT + ActionButton 提示词驱动版）。
// 覆盖：
// 1. 提示词模板本身：不含 username、不再把专家目录标为「绝对路径」、
//    不含家目录绝对路径，且明确 ~ 为家目录、指示先展开再遍历（对应评审要求）。
// 2. 端到端：专家详情 Modal → 分享 →（PAT 已配置）ActionButton → Drawer 中
//    渲染出的提示词必须把 expert_dir 显示为 ~/.ntd/... 家目录相对路径，
//    不能出现 /Users/... 之类绝对路径，也不能出现 username。
// 说明：实际提交 PR 依赖真实 GitCode 账号与 AI 执行器，无法自动化；
// 本用例只验证提示词内容与 Drawer 渲染，不触发执行。

import { test, expect } from '@playwright/test';
// 直接导入纯函数模板做内容断言：不依赖页面渲染，改动提示词即可在此处回归。
import { buildContributePrompt } from '../src/components/settings/experts/contributePrompt';
// 专家 mock 载荷构造复用共享 helper（与 check_expert_source_filter 同源）
import { mockExpert, mockExpertsResponse } from './helpers/expertMock';

test('提示词模板：不含 username / 绝对路径标签，且使用 ~ 家目录相对路径', () => {
  const prompt = buildContributePrompt();

  // PAT 位置描述必须指向家目录相对路径，且 JSON 结构只提 pat 字段。
  expect(prompt).toContain('~/.ntd/contribution_pat.json');
  expect(prompt).toContain('{"pat":"..."}');
  // 评审要求：提示词中不得出现 username（账号身份由 AI 用 /user 接口实时获取）。
  expect(prompt.toLowerCase()).not.toContain('username');
  // 评审要求：不得再把专家目录标为「（绝对路径）」——expert_dir 现在是 ~ 相对路径；
  // 也不得硬编码家目录绝对路径（/Users/...）。
  expect(prompt).not.toContain('专家目录（绝对路径）');
  expect(prompt).not.toContain('/Users/');
  // 明确 ~ 的含义并指示先展开再遍历，避免执行器把 ~ 当字面目录名。
  // 注意这里出现「展开为绝对路径」是「展开 ~」的指令，不是暴露绝对路径，属预期。
  expect(prompt).toContain('~ 表示当前用户家目录');
  expect(prompt).toContain('展开为绝对路径');
  // 上传路径必须落在仓库 experts/<专家名>/ 下（与 bundled 同步源结构一致），
  // 不能写到仓库根目录；前缀固定为 contents/experts/{{expert_name}}/。
  expect(prompt).toContain('contents/experts/{{expert_name}}/');
  expect(prompt).toContain('不能写到仓库根目录');
});

test('系统专家详情 Modal 不出现「分享」按钮（分享仅限用户自定义专家）', async ({ page }) => {
  // 进入专家页。
  await page.goto('/#/experts');

  // 点第一张「系统」来源的卡片（data-source="system"）——系统/模板专家不渲染分享入口。
  const firstSystemCard = page.locator('div[role="button"][data-source="system"]').first();
  await firstSystemCard.waitFor({ state: 'visible', timeout: 15000 });
  await firstSystemCard.click();

  // 详情 Modal 打开后，操作区不应出现「分享」按钮（系统专家只读、不可分享）。
  await page.getByRole('dialog').waitFor({ state: 'visible', timeout: 5000 });
  await expect(page.getByRole('button', { name: '分享' })).toHaveCount(0);
});

// 以下两个用例依赖「PAT 已配置」态：用 page.route 拦截 auth/status 返回 configured:true。
// 不再写真实 ~/.ntd/contribution_pat.json——避免污染用户真实凭据，
// 也避免与 027 等其它 spec 并行运行时互相覆盖/还原 PAT 文件的竞态。
test.describe('PAT 已配置态（route mock）', () => {
  test.beforeEach(async ({ page }) => {
    await page.route('**/api/v1/contribution/auth/status', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ code: 0, data: { configured: true } }),
      }),
    );
    // 分享用例需要「用户来源」专家卡片：真实后端只保证系统专家（用户专家目录
    // ~/.ntd/experts/ 在干净环境不存在），route mock 注入一个确定性用户专家。
    // 载荷构造复用 helpers/expertMock（与 check_expert_source_filter 同源，防两处漂移）；
    // definition_dir 带 /.ntd/ 标记，让 toHomePath 能转成 ~/ 相对路径（提示词断言依赖）。
    await page.route('**/api/v1/experts', (route) =>
      route.fulfill(mockExpertsResponse([mockExpert('mock-user-expert', 'user')])),
    );
  });

  test('专家详情 Modal 分享 → Drawer 渲染的提示词用 ~/.ntd 家目录相对路径，无 username / 绝对路径', async ({ page }) => {
    // 进入专家页。
    await page.goto('/#/experts');

    // 专家卡片用 role="button" 渲染；等待至少一张卡片可见。
    // 分享只对用户自定义专家开放：系统/模板来源的卡片不渲染分享按钮，
    // 必须点「用户」来源的卡片（卡片根节点带 data-source=user）才有分享入口。
    const firstUserCard = page.locator('div[role="button"][data-source="user"]').first();
    await firstUserCard.waitFor({ state: 'visible', timeout: 15000 });

    // 点击第一张用户专家卡片，打开详情 Modal。
    await firstUserCard.click();

    // 详情 Modal 的操作区应出现「分享」按钮。
    const shareButton = page.getByRole('button', { name: '分享' });
    await shareButton.waitFor({ state: 'visible', timeout: 5000 });

    // 第一次点击：查询 PAT 配置态（route mock 返回 configured=true），
    // 组件会从普通「分享」按钮切换到 ActionButton 分支。
    // 先注册响应等待再点击，避免响应先到导致竞态。
    const statusResp = page.waitForResponse((r) =>
      r.url().includes('/contribution/auth/status'),
    );
    await shareButton.click();
    await statusResp;
    // autoOpen：ActionButton 挂载后自动打开 Drawer，首次点击即可看到提交面板，无需再点第二次。

    // Drawer 中 prompt 编辑区是 textarea（模板参数输入是 input，不会误命中）。
    const promptBox = page.locator('.ant-drawer textarea');
    await promptBox.waitFor({ state: 'visible', timeout: 8000 });
    const promptText = await promptBox.inputValue();

    // 渲染出的 expert_dir 必须是 ~/.ntd/... 家目录相对路径。
    expect(promptText).toContain('~/.ntd/');
    // 不得出现家目录绝对路径（含用户名）与 username 字样。
    expect(promptText).not.toContain('/Users/');
    expect(promptText.toLowerCase()).not.toContain('username');
    // 不得再把专家目录标为「绝对路径」，与 ~ 相对路径的实际值保持一致。
    expect(promptText).not.toContain('专家目录（绝对路径）');
    // PAT 位置与 JSON 结构描述应保持（只提 pat 字段）。
    expect(promptText).toContain('~/.ntd/contribution_pat.json');
    expect(promptText).toContain('{"pat":"..."}');
    // 上传路径带仓库前缀 experts/<专家名>/（渲染后占位符已替换），落位与同步源一致。
    expect(promptText).toContain('contents/experts/');
  });

  test('设置-第三方授权：GitCode PAT 表单按配置态区分——已配置时输入禁用、仅留清空', async ({ page }) => {
    // 直接带 tab 参数进入设置-第三方授权（ContributeButton 未配置时也跳这个 URL）。
    await page.goto('/#/settings?tab=thirdParty');

    // 内嵌子 Tab 第一个为 GitCode。
    await expect(page.getByRole('tab', { name: /GitCode/ })).toBeVisible();

    // PAT 密码输入框（Form 内唯一 input[type=password]）。
    const patInput = page.locator('.ant-form-item input[type="password"]');
    await patInput.waitFor({ state: 'visible', timeout: 8000 });

    // route mock 的 configured=true → 状态展示「已配置」。
    await expect(page.getByText('已配置')).toBeVisible({ timeout: 8000 });

    // 有值（已配置）时：输入框禁用、占位符提示「已配置」、保存按钮不出现、只留清空。
    await expect(patInput).toBeDisabled();
    await expect(patInput).toHaveAttribute('placeholder', /已配置/);
    await expect(page.getByRole('button', { name: /保\s*存/ })).toHaveCount(0);
    const clearButton = page.getByRole('button', { name: /清\s*空/ });
    await expect(clearButton).toBeVisible();
    await expect(clearButton).toBeEnabled();

    // 权限提示必须明确要求 PR 相关权限（创建 PR 需要 fork/建分支/写文件/建 PR 权限），
    // 而不是误导用户用最小权限令牌。
    await expect(page.getByText(/仓库读写与 PR 相关权限/)).toBeVisible();
  });
});