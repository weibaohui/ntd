import { Button } from 'antd';
import { PlusOutlined } from '@ant-design/icons';
import type { QuickButton } from '@/utils/database';

/**
 * 回复输入框上方的快捷按钮条：渲染用户自定义按钮 + 末尾「+」管理入口。
 * 点按钮 → onInsert(话术) 由外层填入输入框；点「+」→ onManage 打开管理弹窗。
 * 空列表时仍保留「+」，保证随时能新增第一个按钮。
 */
export function QuickButtonBar({
  buttons,
  onInsert,
  onManage,
}: {
  buttons: QuickButton[];
  onInsert: (text: string) => void;
  onManage: () => void;
}) {
  return (
    // overflowX auto：按钮多了横向滚动而非撑爆布局；flexShrink 0 防按钮被压缩
    <div style={{ display: 'flex', gap: 4, overflowX: 'auto', marginBottom: 4 }}>
      {buttons.map((b) => (
        <Button
          key={b.id}
          size="small"
          onClick={() => onInsert(b.prompt_text)}
          style={{ borderRadius: 12, fontSize: 12, flexShrink: 0, whiteSpace: 'nowrap' }}
        >
          {b.button_name}
        </Button>
      ))}
      <Button
        size="small"
        type="dashed"
        icon={<PlusOutlined />}
        onClick={onManage}
        style={{ borderRadius: 12, fontSize: 12, flexShrink: 0 }}
      />
    </div>
  );
}
