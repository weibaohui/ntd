import { useEffect, useMemo, useState } from 'react';
import { List, Checkbox, Button, message, Alert, Typography, Empty } from 'antd';
import { CheckOutlined } from '@ant-design/icons';
import { createTodo, batchUpdateTodosExecutor } from '@/utils/database';
import type { ParseResult, Proposal } from './parseProposals';
import { parseProposals } from './parseProposals';
import { PROPOSAL_EXECUTOR } from './proposalPrompt';

const { Text, Paragraph } = Typography;

interface ProposalCompletedProps {
  /** AI 输出的原文，由 ActionButton 完成态插槽注入；内部用 parseProposals 解析 */
  result: string;
  /** 当前工作空间 ID，批量创建 Todo 时归属 */
  workspaceId: number;
  /** 关闭宿主 Drawer（ActionButton 的 close），创建成功或用户点「关闭」时调用 */
  onClose: () => void;
}

/**
 * 黑板「生成建议」完成态视图：把 AI 输出解析成 Todo 建议列表，用户勾选后批量创建。
 *
 * 设计取舍：
 * - 作为 ActionButton 的 completedView 插槽渲染，不再自带 Drawer 外壳——
 *   前置 prompt/执行器选择、执行态、失败态都由 ActionButton 统一承载，此处只负责完成态。
 * - 操作栏用 sticky 底栏，替代原先 Drawer 的 pinned footer，视觉等价且自洽。
 * - 未解析出建议时透出 AI 原文，绝不静默失败（与 ActionButton 的 failed 态互补：
 *   后者管「执行失败」，这里管「执行成功但输出无法解析」）。
 */
export function ProposalCompleted({ result, workspaceId, onClose }: ProposalCompletedProps) {
  // 解析随 result 变化；useMemo 避免每次重渲染都重跑 YAML 解析
  const parseResult: ParseResult = useMemo(() => parseProposals(result), [result]);
  const { proposals, raw } = parseResult;
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [creating, setCreating] = useState(false);

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

  return (
    <div style={{ display: 'flex', flexDirection: 'column', minHeight: '100%' }}>
      <div style={{ flex: 1, overflow: 'auto', minHeight: 0 }}>
        {renderProposalBody(proposals, raw, selected, handleToggle)}
      </div>
      {/* sticky 底栏：替代原 Drawer footer，操作始终可达 */}
      <div
        style={{
          position: 'sticky',
          bottom: 0,
          background: 'var(--color-bg-elevated, #fff)',
          borderTop: '1px solid var(--color-border-secondary, #f0f0f0)',
          padding: '8px 0',
        }}
      >
        {renderProposalFooter(proposals.length, selected.size, creating, handleCreate, onClose)}
      </div>
    </div>
  );
}

/** 批量创建勾选的建议为 Todo，并统一设置执行器为 pi。失败时弹错并不关闭，便于重试。 */
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

/** 渲染建议列表主体：有建议渲染可勾选列表，否则兜底透出 AI 原文。 */
function renderProposalBody(
  proposals: Proposal[],
  raw: string,
  selected: Set<number>,
  onToggle: (index: number) => void,
) {
  if (proposals.length > 0) {
    return (
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
    );
  }
  return renderRawFallback(raw);
}

/** 兜底区：未解析出建议时透出 AI 原文，让用户看见实际输出。 */
function renderRawFallback(raw: string) {
  if (!raw) {
    return <Empty description="暂无建议" style={{ padding: '24px 0' }} />;
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

/** 渲染底栏：已选计数 + 关闭 / 批量创建按钮。 */
function renderProposalFooter(
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
