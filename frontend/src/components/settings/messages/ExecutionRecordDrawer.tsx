import { Drawer } from 'antd';
import { Tag } from 'antd';
import type { ExecutionRecord } from '@/types';

export function ExecutionRecordDrawer({ record, onClose }: {
  record: ExecutionRecord | null;
  onClose: () => void;
}) {
  const statusColor = record?.status === 'success' ? 'green' : record?.status === 'failed' ? 'red' : 'blue';
  return (
    <Drawer
      title={`执行记录 #${record?.id ?? ''}`}
      placement="right"
      width={600}
      open={!!record}
      onClose={onClose}
      destroyOnClose
    >
      {record && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          {/* 状态行 */}
          <div style={{ display: 'flex', gap: 12, flexWrap: 'wrap' }}>
            <Tag color={statusColor}>{record.status}</Tag>
            {record.executor && <Tag>{record.executor}</Tag>}
            <Tag>{record.trigger_type}</Tag>
            {record.model && <Tag>{record.model}</Tag>}
          </div>
          {/* 时间行 */}
          <div style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>
            开始: {record.started_at ? new Date(record.started_at).toLocaleString() : '-'}
            {record.finished_at && ` → 结束: ${new Date(record.finished_at).toLocaleString()}`}
          </div>
          {/* 结果 */}
          {record.result && (
            <div>
              <strong style={{ fontSize: 13 }}>结果</strong>
              <pre style={{
                background: 'var(--color-fill-quaternary)',
                padding: 10, borderRadius: 6, fontSize: 12,
                maxHeight: 200, overflow: 'auto',
                whiteSpace: 'pre-wrap', lineHeight: 1.6, marginTop: 4,
              }}>
                {record.result}
              </pre>
            </div>
          )}
          {/* 输出 */}
          {record.stdout && (
            <div>
              <strong style={{ fontSize: 13 }}>输出</strong>
              <pre style={{
                background: 'var(--color-fill-quaternary)',
                padding: 10, borderRadius: 6, fontSize: 12,
                maxHeight: 300, overflow: 'auto',
                whiteSpace: 'pre-wrap', lineHeight: 1.6, marginTop: 4,
              }}>
                {record.stdout}
              </pre>
            </div>
          )}
          {/* 错误 */}
          {record.stderr && (
            <div>
              <strong style={{ fontSize: 13, color: 'var(--color-error)' }}>错误</strong>
              <pre style={{
                background: '#fff1f0',
                padding: 10, borderRadius: 6, fontSize: 12,
                maxHeight: 150, overflow: 'auto',
                whiteSpace: 'pre-wrap', lineHeight: 1.6, marginTop: 4,
                color: 'var(--color-error)',
              }}>
                {record.stderr}
              </pre>
            </div>
          )}
        </div>
      )}
    </Drawer>
  );
}