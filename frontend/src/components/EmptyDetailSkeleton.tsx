import { Skeleton } from 'antd';

/**
 * 未选中详情时的骨架屏占位组件
 * 模拟详情页的大致布局，提升空白状态的视觉体验
 */
export function EmptyDetailSkeleton() {
  return (
    <div className="detail-panel" style={{ padding: 20 }}>
      {/* 头部卡片区域骨架 */}
      <div
        style={{
          background: 'var(--color-bg-card)',
          borderRadius: 'var(--radius-md)',
          padding: 16,
          marginBottom: 20,
        }}
      >
        {/* 标题行骨架 */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 12 }}>
          <Skeleton.Avatar active size="small" shape="circle" />
          <Skeleton.Input active size="small" style={{ width: 200 }} />
          <div style={{ marginLeft: 'auto', display: 'flex', gap: 8 }}>
            <Skeleton.Button active size="small" shape="circle" />
            <Skeleton.Button active size="small" shape="circle" />
          </div>
        </div>

        {/* 标签行骨架 */}
        <div style={{ display: 'flex', gap: 8, marginBottom: 16, flexWrap: 'wrap' }}>
          <Skeleton.Button active size="small" style={{ width: 60 }} />
          <Skeleton.Button active size="small" style={{ width: 80 }} />
          <Skeleton.Input active size="small" style={{ width: 120 }} />
        </div>

        {/* Prompt 区域骨架 */}
        <div style={{ marginBottom: 16 }}>
          <Skeleton active paragraph={{ rows: 2, width: ['100%', '80%'] }} title={false} />
        </div>

        {/* 按钮区域骨架 */}
        <div style={{ display: 'flex', gap: 8 }}>
          <Skeleton.Button active style={{ flex: 1, height: 36 }} />
          <Skeleton.Button active style={{ flex: 1, height: 36 }} />
        </div>
      </div>

      {/* 执行历史标题骨架 */}
      <div style={{ display: 'flex', alignItems: 'center', marginBottom: 12 }}>
        <Skeleton.Input active size="small" style={{ width: 100 }} />
        <div style={{ marginLeft: 'auto', display: 'flex', gap: 8 }}>
          <Skeleton.Button active size="small" style={{ width: 80 }} />
          <Skeleton.Button active size="small" shape="circle" />
        </div>
      </div>

      {/* 执行历史列表骨架 */}
      <div style={{ display: 'flex', gap: 16 }}>
        {/* 左侧列表 */}
        <div style={{ width: 240, flexShrink: 0, display: 'flex', flexDirection: 'column', gap: 8 }}>
          {[1, 2, 3, 4].map((i) => (
            <div
              key={i}
              style={{
                background: 'var(--color-bg-card)',
                borderRadius: 'var(--radius-md)',
                padding: 12,
              }}
            >
              <Skeleton active paragraph={{ rows: 2, width: ['100%', '60%'] }} title={false} />
            </div>
          ))}
        </div>

        {/* 右侧详情 */}
        <div style={{ flex: 1, minWidth: 0 }}>
          <div
            style={{
              background: 'var(--color-bg-card)',
              borderRadius: 'var(--radius-md)',
              padding: 16,
              marginBottom: 12,
            }}
          >
            <Skeleton active paragraph={{ rows: 1, width: '40%' }} title={false} />
            <div style={{ height: 12 }} />
            <Skeleton active paragraph={{ rows: 3, width: ['100%', '100%', '70%'] }} title={false} />
          </div>
          <div
            style={{
              background: 'var(--color-bg-tertiary)',
              borderRadius: 'var(--radius-md)',
              padding: 16,
            }}
          >
            <Skeleton active paragraph={{ rows: 6, width: ['30%', '100%', '100%', '80%', '100%', '60%'] }} title={false} />
          </div>
        </div>
      </div>
    </div>
  );
}
