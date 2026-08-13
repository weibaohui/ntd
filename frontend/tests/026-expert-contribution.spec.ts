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
});

test('专家详情 Modal 分享 → Drawer 渲染的提示词用 ~/.ntd 家目录相对路径，无 username / 绝对路径', async ({ page }) => {
  // 进入专家页。
  await page.goto('/#/experts');

  // 专家卡片用 role="button" 渲染；等待至少一张卡片可见。
  const firstCard = page.locator('div[role="button"]').first();
  await firstCard.waitFor({ state: 'visible', timeout: 15000 });

  // 点击第一张专家卡片，打开详情 Modal。
  await firstCard.click();

  // 详情 Modal 的操作区应出现「分享」按钮。
  const shareButton = page.getByRole('button', { name: '分享' });
  await shareButton.waitFor({ state: 'visible', timeout: 5000 });

  // 第一次点击：查询 PAT 配置态。本机 ~/.ntd/contribution_pat.json 已配置，
  // 组件会从普通按钮切换到 ActionButton 分支（带「更换 PAT」入口）。
  // 先注册响应等待再点击，避免响应先到导致竞态（.anticon-reload 在页面上
  // 本就存在，不能用作切换完成的信号）。
  const statusResp = page.waitForResponse((r) => r.url().includes('/contribution/auth/status'));
  await shareButton.click();
  await statusResp;
  // 等 React 完成分支切换渲染后再点第二次。
  await page.waitForTimeout(300);

  // 第二次点击 ActionButton 的「分享」：打开提交 Drawer。
  await page.getByRole('button', { name: '分享' }).click();

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
});

test('设置-第三方授权：GitCode PAT 表单按配置态区分——已配置时输入禁用、仅留清空', async ({ page }) => {
  // 直接带 tab 参数进入设置-第三方授权（ContributeButton 未配置时也跳这个 URL）。
  await page.goto('/#/settings?tab=thirdParty');

  // 内嵌子 Tab 第一个为 GitCode。
  await expect(page.getByRole('tab', { name: /GitCode/ })).toBeVisible();

  // PAT 密码输入框（Form 内唯一 input[type=password]）。
  const patInput = page.locator('.ant-form-item input[type="password"]');
  await patInput.waitFor({ state: 'visible', timeout: 8000 });

  // 本机已配置 PAT（~/.ntd/contribution_pat.json 存在）→ 状态展示「已配置」。
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
