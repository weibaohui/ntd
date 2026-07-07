import { useEffect, useState } from 'react';
import { Drawer, List, Checkbox, Button, message, Alert, Typography, Empty } from 'antd';
import { CheckOutlined } from '@ant-design/icons';
import { useIsMobile } from '@/hooks/useIsMobile';
import { createTodo, batchUpdateTodosExecutor } from '@/utils/database';
import type { ParseResult, Proposal } from './parseProposals';
import { PROPOSAL_EXECUTOR } from './proposalPrompt';

const { Text, Paragraph } = Typography;

interface ProposalDrawerProps {
  open: boolean;
  onClose: () => void;
  /** parseProposals 的结果：建议列表 + AI 原始输出（兜底透出用） */
  parseResult: ParseResult;
  workspaceId: number;
  /** 执行失败时的错误信息，非空时在顶部告警展示 */
  errorMessage?: string;
}

/**
 * Todo 建议列表 Drawer。
 *
 * 展示 parseProposals 解析出的建议，用户勾选后批量创建为 pending Todo，
 * 并统一把执行器设为 pi。未解析出建议时透出 AI 原文，绝不静默失败。
 */
export function ProposalDrawer({ open, onClose, parseResult, workspaceId, errorMessage }: ProposalDrawerProps) {
  const { proposals, raw } = parseResult;
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [creating, setCreating] = useState(false);
  const isMobile = useIsMobile();

  // 建议列表变化时默认全选：贴合「批量确认」意图，用户再手动取消不想要的
  useEffect(() => {
    setSelected(new Set(proposals.map((_, i) => i)));
  }, [proposals]);

  const handleToggle = (index: number) => setSelected(toggleInSet(selected, index));

  const handleCreate = () => {
    const picked = proposals.filter((_, i) => selected.has(i));
    if (picked.length === 0) {
      message.warning('请至少勾选一条建议');
      return;
    }
    void createProposedTodos(picked, workspaceId, setCreating, onClose);
  };

  const hasError = !!errorMessage;

  return (
    <Drawer
      title="Todo 建议"
      open={open}
      onClose={onClose}
      placement={isMobile ? 'bottom' : 'right'}
      width={isMobile ? '100%' : 560}
      height={isMobile ? '85vh' : undefined}
      maskClosable={false}
      destroyOnHidden
      footer={renderFooter(proposals.length, selected.size, creating, handleCreate, onClose)}
    >
      {renderDrawerBody(proposals, raw, selected, hasError, errorMessage, handleToggle)}
    </Drawer>
  );
}

/** 批量创建勾选的建议为 Todo，并统一设置执行器为 pi。失败时弹错并不关闭 Drawer，便于重试。 */
async function createProposedTodos(
  picked: Proposal[],
  workspaceId: number,
  setCreating: (v: boolean) => void,
  onClose: () => void,
): Promise<void> {
  setCreating(true);
  try {
    // createTodo 不直接支持 executor 参数，故先逐条创建收集 id，再一次批量设执行器
    const created = await Promise.all(
      picked.map(p => createTodo(p.title, p.prompt, [], workspaceId)),
    );
    const ids = created.map(t => t.id);
    // batchUpdateTodosExecutor 内部对网络/后端错误做了 catch 并返回 failed 列表（不抛），
    // 因此必须检查返回值，否则执行器设置失败时会误报成功
    let executorFailed = 0;
    if (ids.length > 0) {
      const result = await batchUpdateTodosExecutor(ids, PROPOSAL_EXECUTOR);
      executorFailed = result.failed.length;
    }
    if (executorFailed > 0) {
      message.warning(`已创建 ${ids.length} 条 Todo，但 ${executorFailed} 条执行器设置失败，请在列表核对`);
    } else {
      message.success(`已创建 ${ids.length} 条 Todo（执行器：${PROPOSAL_EXECUTOR}）`);
    }
    onClose();
  } catch (err: unknown) {
    message.error(err instanceof Error ? err.message : '创建失败');
  } finally {
    setCreating(false);
  }
}

/** 渲染 Drawer 主体：有建议渲染列表，否则兜底透出 AI 原文。 */
function renderDrawerBody(
  proposals: Proposal[],
  raw: string,
  selected: Set<number>,
  hasError: boolean,
  errorMessage: string | undefined,
  onToggle: (index: number) => void,
) {
  return (
    <>
      {hasError && errorMessage && (
        <Alert type="error" message={errorMessage} showIcon style={{ marginBottom: 12 }} />
      )}
      {proposals.length > 0 ? (
        <List
          dataSource={proposals}
          renderItem={(item, index) => (
            <List.Item style={{ alignItems: 'flex-start' }}>
              <Checkbox
                checked={selected.has(index)}
                onChange={() => onToggle(index)}
                style={{ marginRight: 8, marginTop: 4 }}
              />
              <div style={{ flex: 1, minWidth: 0 }}>
                <Text strong>{item.title}</Text>
                <Paragraph
                  type="secondary"
                  style={{ fontSize: 12, marginTop: 4, marginBottom: 0, whiteSpace: 'pre-wrap' }}
                  ellipsis={{ expandable: true, symbol: '展开' }}
                >
                  {item.prompt}
                </Paragraph>
              </div>
            </List.Item>
          )}
        />
      ) : (
        renderRawFallback(raw)
      )}
    </>
  );
}

/** 兜底区：未解析出建议时透出 AI 原文，让用户看见实际输出。 */
function renderRawFallback(raw: string) {
  if (!raw) {
    return <Empty description="暂无建议" />;
  }
  return (
    <>
      <Alert
        type="warning"
        message="未能从 AI 输出中解析出有效建议"
        description="可能是 AI 输出格式异常。以下是原始输出，供参考："
        showIcon
        style={{ marginBottom: 12 }}
      />
      <pre style={{ whiteSpace: 'pre-wrap', wordBreak: 'break-word', fontSize: 12, background: 'var(--color-bg-elevated)', padding: 12, borderRadius: 6, margin: 0 }}>
        {raw}
      </pre>
    </>
  );
}

/** 渲染底部：已选计数 + 关闭 / 批量创建按钮。 */
function renderFooter(
  total: number,
  selectedCount: number,
  creating: boolean,
  onCreate: () => void,
  onClose: () => void,
) {
  return (
    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
      <Text type="secondary" style={{ fontSize: 13 }}>
        已选 {selectedCount} / {total} 条
      </Text>
      <div>
        <Button onClick={onClose} disabled={creating} style={{ marginRight: 8 }}>关闭</Button>
        <Button type="primary" icon={<CheckOutlined />} loading={creating} onClick={onCreate} disabled={total === 0}>
          批量创建
        </Button>
      </div>
    </div>
  );
}

/** 不可变地翻转 Set 中某个 key 的存在性，返回新 Set。 */
function toggleInSet(set: Set<number>, index: number): Set<number> {
  const next = new Set(set);
  if (next.has(index)) {
    next.delete(index);
  } else {
    next.add(index);
  }
  return next;
}
