import { Modal } from 'antd';
import type { ExecutionRecord } from '../../../types';

export function ExecutionDetailModal({ record, onClose }: {
  record: ExecutionRecord | null;
  onClose: () => void;
}) {
  return (
    <Modal
      title={record ? `执行记录 #${record.id}` : '执行记录'}
      open={!!record}
      onCancel={onClose}
      footer={null}
      width={700}
    >
      {record && (
        <div style={{ maxHeight: '60vh', overflow: 'auto' }}>
          <div style={{ display: 'flex', gap: 16, marginBottom: 12, flexWrap: 'wrap' }}>
            <span><strong>状态:</strong> {record.status}</span>
            <span><strong>执行器:</strong> {record.executor || '-'}</span>
            <span><strong>触发:</strong> {record.trigger_type}</span>
            {record.model && <span><strong>模型:</strong> {record.model}</span>}
          </div>
          <div style={{ marginBottom: 8, fontSize: 12, color: 'var(--color-text-secondary)' }}>
            开始: {record.started_at ? new Date(record.started_at).toLocaleString() : '-'}
            {record.finished_at && ` | 结束: ${new Date(record.finished_at).toLocaleString()}`}
          </div>
          {record.result && (
            <div style={{ marginBottom: 12 }}>
              <strong>结果:</strong>
              <pre style={{ background: 'var(--color-fill-quaternary)', padding: 8, borderRadius: 4, fontSize: 12, maxHeight: 200, overflow: 'auto', whiteSpace: 'pre-wrap', marginTop: 4 }}>
                {record.result}
              </pre>
            </div>
          )}
          {record.stdout && (
            <div style={{ marginBottom: 12 }}>
              <strong>输出:</strong>
              <pre style={{ background: 'var(--color-fill-quaternary)', padding: 8, borderRadius: 4, fontSize: 12, maxHeight: 200, overflow: 'auto', whiteSpace: 'pre-wrap', marginTop: 4 }}>
                {record.stdout}
              </pre>
            </div>
          )}
          {record.stderr && (
            <div>
              <strong>错误:</strong>
              <pre style={{ background: 'var(--color-fill-quaternary)', padding: 8, borderRadius: 4, fontSize: 12, maxHeight: 150, overflow: 'auto', whiteSpace: 'pre-wrap', marginTop: 4, color: 'var(--color-error)' }}>
                {record.stderr}
              </pre>
            </div>
          )}
        </div>
      )}
    </Modal>
  );
}
