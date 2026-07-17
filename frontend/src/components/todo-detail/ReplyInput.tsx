import { useState, useEffect, useRef } from 'react';
import { Input, Button } from 'antd';
import type { InputRef } from 'antd';
import { SendOutlined } from '@ant-design/icons';
import type { ExecutionRecord } from '@/types';
import { getQuickButtons } from '@/utils/database';
import type { QuickButton } from '@/utils/database';
import { QuickButtonBar } from './QuickButtonBar';
import { QuickButtonManageModal } from './QuickButtonManageModal';

/**
 * 论坛式内联回复框 —— 输入框 + 回复按钮 + 上方一排用户自定义快捷按钮。
 * 点快捷按钮把预设话术填入输入框（覆盖），用户可再编辑后点「回复」继续会话。
 */
export function ReplyInput({
  record,
  onReply,
  loading,
}: {
  record: ExecutionRecord;
  onReply: (record: ExecutionRecord, message: string) => Promise<void>;
  loading?: boolean;
}) {
  const [message, setMessage] = useState('');
  const [buttons, setButtons] = useState<QuickButton[]>([]);
  const [manageOpen, setManageOpen] = useState(false);
  // 填入话术后自动聚焦输入框，方便用户立刻接着编辑
  const inputRef = useRef<InputRef>(null);

  // 每个会话线程各有一个 ReplyInput，各自拉一次全局按钮列表；
  // 列表只读且数据量小，不做跨组件缓存（YAGNI）。
  useEffect(() => {
    getQuickButtons().then(setButtons).catch(() => {});
  }, []);

  // 弹窗内增删改后重拉，保持按钮条与全局数据同步
  const reloadButtons = () => {
    getQuickButtons().then(setButtons).catch(() => {});
  };

  // 点快捷按钮：覆盖填入话术（Q10 决策）+ 聚焦输入框
  const handleInsert = (text: string) => {
    setMessage(text);
    inputRef.current?.focus();
  };

  const handleReply = async () => {
    const trimmed = message.trim();
    if (!trimmed || loading) return;
    await onReply(record, trimmed);
    setMessage('');
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleReply();
    }
  };

  return (
    // 外层 block 容器：上方按钮条 + 下方输入行（输入行沿用原有 flex 横向布局）
    <div style={{ marginLeft: 24, marginTop: 4, marginBottom: 8 }}>
      <QuickButtonBar buttons={buttons} onInsert={handleInsert} onManage={() => setManageOpen(true)} />
      <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
        <Input
          ref={inputRef}
          size="small"
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="输入回复内容..."
          disabled={loading}
          style={{ flex: 1, borderRadius: 16, fontSize: 12 }}
        />
        <Button
          type="primary"
          size="small"
          icon={<SendOutlined />}
          onClick={handleReply}
          loading={loading}
          disabled={!message.trim()}
          style={{ borderRadius: 16 }}
        >
          回复
        </Button>
      </div>
      <QuickButtonManageModal open={manageOpen} onClose={() => setManageOpen(false)} onChanged={reloadButtons} />
    </div>
  );
}
