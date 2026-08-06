// 讨论区输入器：Markdown 编辑 + inline @选择器 + 快捷话术（QuickButtonBar）+ 发送。
// 输入 @ 弹候选（专家优先 + 执行器）；点快捷话术追加到正文。后端按正文 @token 解析触发执行。
// @检测/候选构造是纯函数，提取到 ./utils 便于单测；本组件只负责渲染与副作用。

import { useEffect, useState } from 'react';
import { Button, Space, Typography, theme } from 'antd';
import { SendOutlined } from '@ant-design/icons';
import { MdEditor } from '@/components/MdEditor';
import { getAllExperts } from '@/utils/database/experts';
import { getQuickButtons } from '@/utils/database';
import type { QuickButton } from '@/utils/database';
import { QuickButtonBar } from '@/components/todo-detail/QuickButtonBar';
import { QuickButtonManageModal } from '@/components/todo-detail/QuickButtonManageModal';
import type { ExpertMetadata } from '@/types/expert';
import { detectAtToken, buildCandidates, type MentionCandidate } from './utils';

const { Text } = Typography;

interface DiscussionComposerProps {
  value: string;
  onChange: (v: string) => void;
  onSend: () => void;
  sending: boolean;
  /** 所属工作空间，用于拉取快捷话术（quick_buttons）。 */
  workspaceId: number;
  /** 回复目标提示（如 "#3 张三"）；null=主楼层新帖。 */
  replyTo?: string | null;
  onCancelReply?: () => void;
}

interface MentionsPickerProps {
  candidates: MentionCandidate[];
  onPick: (name: string) => void;
}

/** 候选浮层：渲染在编辑器上方（absolute bottom:100%），点击选中插入。 */
function MentionsPicker({ candidates, onPick }: MentionsPickerProps) {
  // 用 antd 主题 token 取色，跟随明暗主题，避免硬编码颜色（前端规范：禁硬编码色值）。
  const { token } = theme.useToken();
  // hover 高亮用 state 驱动而非直接改 DOM style：React 惯用写法，颜色全程走 token 不硬编码。
  const [hoverIdx, setHoverIdx] = useState<number | null>(null);
  return (
    <div
      style={{
        position: 'absolute', bottom: '100%', left: 0, right: 0, marginBottom: 2,
        background: token.colorBgContainer, border: `1px solid ${token.colorBorder}`,
        borderRadius: 6, maxHeight: 200, overflowY: 'auto', zIndex: 20,
        boxShadow: token.boxShadowSecondary,
      }}
    >
      {candidates.map((c, i) => (
        <button
          key={`${c.kind}:${c.name}`}
          type="button"
          onClick={() => onPick(c.name)}
          // 用原生 button 而非 div：键盘用户可用 Tab 聚焦、Enter/Space 激活（可访问性，CodeRabbit）。
          style={{
            padding: '6px 12px', cursor: 'pointer', display: 'flex', alignItems: 'center', gap: 8,
            border: 'none', textAlign: 'left', width: '100%',
            // hover 时高亮背景由 hoverIdx 三元决定，颜色取 antd 控件 hover token，明暗主题自适应。
            background: hoverIdx === i ? token.controlItemBgHover : 'transparent',
          }}
          onMouseEnter={() => setHoverIdx(i)}
          onMouseLeave={() => setHoverIdx(null)}
        >
          <Text style={{ fontSize: 12, color: c.kind === 'expert' ? token.colorPrimary : token.colorSuccess }}>
            {c.kind === 'expert' ? '专家' : '执行器'}
          </Text>
          <Text>{c.display}</Text>
        </button>
      ))}
    </div>
  );
}

export function DiscussionComposer({
  value, onChange, onSend, sending, workspaceId, replyTo, onCancelReply,
}: DiscussionComposerProps) {
  // 专家 + 快捷话术挂载时各拉一次（数据小、几乎不变）；失败只 warn，不阻塞输入。
  const [experts, setExperts] = useState<ExpertMetadata[]>([]);
  const [buttons, setButtons] = useState<QuickButton[]>([]);
  const [manageOpen, setManageOpen] = useState(false);
  useEffect(() => {
    getAllExperts().then(setExperts).catch((e) => console.warn('专家列表加载失败', e));
    if (workspaceId) getQuickButtons(workspaceId).then(setButtons).catch((e) => console.warn('快捷话术加载失败', e));
  }, [workspaceId]);

  // 管理弹窗内增删改后重拉，保持按钮条与 workspace 数据同步。
  const reloadButtons = () => {
    if (workspaceId) getQuickButtons(workspaceId).then(setButtons).catch((e) => console.warn('快捷话术加载失败', e));
  };

  // 末尾 @ 检测 → 候选；无候选（含无 @）时不显示浮层。
  const atToken = detectAtToken(value);
  const candidates = atToken ? buildCandidates(atToken.query, experts) : [];
  const showPicker = atToken !== null && candidates.length > 0;

  /** 选中候选：把末尾的 @query 替换为 @<规范名> + 空格，避免后续输入并入名字。 */
  const insertMention = (name: string) => {
    onChange(value.replace(/@([^\s@]*)$/, `@${name} `));
  };
  /** 快捷话术：追加到正文末尾（前面补空格防粘连），用户可继续编辑后发送。 */
  const handleQuickInsert = (text: string) => {
    const needSpace = value.length > 0 && !value.endsWith(' ');
    onChange(`${value}${needSpace ? ' ' : ''}${text}`);
  };

  const canSend = value.trim().length > 0 && !sending;

  return (
    <div>
      {replyTo ? (
        <div style={{ marginBottom: 8 }}>
          <Text type="secondary">回复 {replyTo} </Text>
          <Button size="small" type="link" onClick={onCancelReply}>取消</Button>
        </div>
      ) : null}
      {/* 快捷话术挂在输入框上方（设计§7.4 / spec F9）：点按钮追加预设话术，点「+」管理。 */}
      <QuickButtonBar buttons={buttons} onInsert={handleQuickInsert} onManage={() => setManageOpen(true)} />
      <div style={{ position: 'relative' }}>
        {showPicker ? <MentionsPicker candidates={candidates} onPick={insertMention} /> : null}
        <MdEditor value={value} onChange={onChange} height={140} />
      </div>
      <Space style={{ marginTop: 8, width: '100%', justifyContent: 'space-between', flexWrap: 'wrap' }}>
        <Text type="secondary" style={{ fontSize: 12 }}>
          输入 @ 触发选择专家/执行器，如 @前端架构师、@codex
        </Text>
        <Button type="primary" icon={<SendOutlined />} disabled={!canSend} loading={sending} onClick={onSend}>
          发送
        </Button>
      </Space>
      <QuickButtonManageModal
        open={manageOpen}
        workspaceId={workspaceId}
        onClose={() => setManageOpen(false)}
        onChanged={reloadButtons}
      />
    </div>
  );
}
