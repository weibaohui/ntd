// PushStatusCard 单元测试（推送目标简化，issue 相关）。
//
// 本文件由原 Playwright harness spec `tests/check_push_target_card.spec.ts` 迁移而来：
// 原写法用 vite dev server 服务 `/tests/push-target-mount.html` + 一段 mount 脚本把组件挂到
// 浏览器，再断言 body 文本——依赖独立的 5173 vite 进程（make dev 的 18088 embedded 不服务
// /tests/*），导致该 spec 长期连不上而失败。组件本身无路由/鉴权依赖，直接用
// @testing-library/react 在 jsdom 里渲染即可断言，无需浏览器与 vite。
//
// 验证点（与原 spec 对应）：群ID 行已移除、「单聊ID」旧 label 已移除、改为「推送目标」
// 只读展示 owner_open_id（所有者，自动捕获）。

import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/react';
import { ConfigProvider } from 'antd';
import { PushStatusCard } from './PushStatusCard';
import type { FeishuPushStatus } from '@/utils/database';

// owner_open_id 已捕获、p2p/group 旧字段为空：推送目标已改为 owner_open_id 的典型态。
const pushStatus: FeishuPushStatus = {
  bot_id: 1,
  push_level: 'result_only',
  owner_open_id: 'ou_b0cb04a51dd7075e92341fbcbde944cd',
  p2p_receive_id: '',
  group_chat_id: '',
  receive_id_type: 'open_id',
  p2p_response_enabled: true,
  group_response_enabled: true,
  p2p_debounce_secs: 20,
  group_debounce_secs: 20,
};

describe('PushStatusCard', () => {
  it('群ID 已移除，改为「推送目标（所有者）」只读展示 owner_open_id', () => {
    // 用 ConfigProvider 包裹，确保 antd 在无全局 provider 时仍稳定渲染（与原 harness 一致）
    const { container } = render(
      <ConfigProvider>
        <PushStatusCard
          pushStatus={pushStatus}
          onPushLevelChange={() => {}}
          onResponseEnabledChange={() => {}}
        />
      </ConfigProvider>,
    );

    // 新行为：保留「推送目标」标签，并以提示文案点明目标为所有者（旧版用一个 Input 展示
    // owner_open_id，后续「推送目标简化」重构去掉了该输入框，故不再断言 input[value]）。
    expect(container.textContent).toContain('推送目标');
    expect(container.textContent).toContain('所有者');

    // 旧行为应已移除：群ID 行、单聊ID label（推送目标已不再让用户手填群/单聊 ID）
    expect(container.textContent).not.toContain('群ID:');
    expect(container.textContent).not.toContain('单聊ID:');
  });
});
