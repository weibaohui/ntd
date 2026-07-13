/**
 * 推送目标卡片 mount：验证 PushStatusCard 在「推送目标简化」后的渲染——
 * 群ID 行已移除，改为「推送目标（所有者）」只读展示 owner_open_id。
 *
 * 流程：构造一个带 owner_open_id 的 pushStatus，挂到 ConfigProvider 下，
 * 写 window.__renderDone = true 让 Playwright 收尾。
 */
import React from 'react';
import { createRoot } from 'react-dom/client';
import { ConfigProvider } from 'antd';
import { PushStatusCard } from '../src/components/settings/assistant/PushStatusCard.tsx';

interface MountWindow extends Window {
  __renderDone?: boolean;
  __renderError?: string;
}
const w = window as MountWindow;

// 构造推送状态：owner_open_id 已捕获，p2p/group 旧字段为空（推送目标已改为 owner_open_id）
const pushStatus = {
  bot_id: 1,
  push_level: 'result_only' as const,
  owner_open_id: 'ou_b0cb04a51dd7075e92341fbcbde944cd',
  p2p_receive_id: '',
  group_chat_id: '',
  receive_id_type: 'open_id',
  p2p_response_enabled: true,
  group_response_enabled: true,
  p2p_debounce_secs: 20,
  group_debounce_secs: 20,
};

try {
  const container = document.createElement('div');
  container.id = 'test-push-card';
  container.style.padding = '20px';
  document.body.appendChild(container);
  const root = createRoot(container);
  // 用 ConfigProvider 包裹，确保 antd 在无全局 provider 时仍稳定渲染
  root.render(
    React.createElement(
      ConfigProvider,
      null,
      React.createElement(PushStatusCard, {
        pushStatus,
        onPushLevelChange: () => {},
        onResponseEnabledChange: () => {},
      })
    )
  );
  setTimeout(() => { w.__renderDone = true; }, 600);
} catch (e) {
  w.__renderError = (e as Error).message;
  w.__renderDone = true;
}
