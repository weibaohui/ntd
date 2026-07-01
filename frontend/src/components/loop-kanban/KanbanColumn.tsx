// LoopKanban 看板列组件。

import type { LoopExecutionWithLoopName } from './index';
import type { ColumnDef } from './helpers';

// 列头
function ColumnHeader({ col, count }: { col: ColumnDef; count: number }) {
  return (
    <div
      className="loop-kanban-column-header"
      style={{
        borderBottom: `3px solid ${col.color}`,
        padding: '8px 12px',
        marginBottom: 8,
      }}
    >
      <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
        <div
          className="loop-kanban-column-dot"
          style={{ width: 8, height: 8, borderRadius: 4, backgroundColor: col.color }}
        />
        <span style={{ fontWeight: 600, fontSize: 13 }}>{col.label}</span>
        <span
          className="loop-kanban-column-count"
          style={{
            background: `${col.color}18`,
            color: col.color,
            borderRadius: 10,
            padding: '0 6px',
            fontSize: 11,
            fontWeight: 600,
          }}
        >
          {count}
        </span>
      </div>
    </div>
  );
}

// 列体
function ColumnBody({ items, renderCard }: { items: LoopExecutionWithLoopName[]; renderCard: (exec: LoopExecutionWithLoopName) => React.ReactNode }) {
  return (
    <div className="loop-kanban-column-body" style={{ flex: 1, minHeight: 0, overflowY: 'auto', padding: '0 4px' }}>
      {items.length === 0 ? (
        <div style={{ textAlign: 'center', padding: '20px 0', color: 'var(--color-text-tertiary)', fontSize: 12 }}>
          暂无
        </div>
      ) : (
        items.map(renderCard)
      )}
    </div>
  );
}

// 看板列组件
interface KanbanColumnProps {
  col: ColumnDef;
  items: LoopExecutionWithLoopName[];
  renderCard: (exec: LoopExecutionWithLoopName) => React.ReactNode;
}

export function KanbanColumn({ col, items, renderCard }: KanbanColumnProps) {
  return (
    <div
      className="loop-kanban-column"
      style={{
        minWidth: 220,
        maxWidth: 280,
        flex: 1,
        display: 'flex',
        flexDirection: 'column',
        minHeight: 0,
      }}
    >
      <ColumnHeader col={col} count={items.length} />
      <ColumnBody items={items} renderCard={renderCard} />
    </div>
  );
}
