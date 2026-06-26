/**
 * 未选中详情时的静态占位组件
 * 模拟详情页的大致布局，但无动画效果，因为这不是加载状态
 */
export function EmptyDetailPlaceholder() {
  return (
    <div className="detail-panel" style={{ padding: 20 }}>
      {/* 头部卡片区域 */}
      <div
        style={{
          background: 'var(--color-bg-card)',
          borderRadius: 'var(--radius-md)',
          padding: 16,
          marginBottom: 20,
        }}
      >
        {/* 标题行 */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 12 }}>
          {/* 状态图标占位 */}
          <div style={{ width: 20, height: 20, borderRadius: '50%', background: 'var(--color-fill-quaternary)' }} />
          {/* 标题占位 */}
          <div style={{ height: 16, width: 200, borderRadius: 4, background: 'var(--color-fill-quaternary)' }} />
          <div style={{ marginLeft: 'auto', display: 'flex', gap: 8 }}>
            {/* 按钮占位 */}
            <div style={{ width: 28, height: 28, borderRadius: 6, background: 'var(--color-fill-quaternary)' }} />
            <div style={{ width: 28, height: 28, borderRadius: 6, background: 'var(--color-fill-quaternary)' }} />
          </div>
        </div>

        {/* 标签行占位 */}
        <div style={{ display: 'flex', gap: 8, marginBottom: 16, flexWrap: 'wrap' }}>
          <div style={{ height: 22, width: 60, borderRadius: 4, background: 'var(--color-fill-quaternary)' }} />
          <div style={{ height: 22, width: 80, borderRadius: 4, background: 'var(--color-fill-quaternary)' }} />
          <div style={{ height: 22, width: 120, borderRadius: 4, background: 'var(--color-fill-quaternary)' }} />
        </div>

        {/* Prompt 区域占位 */}
        <div style={{ marginBottom: 16 }}>
          <div style={{ height: 14, width: '100%', borderRadius: 4, background: 'var(--color-fill-quaternary)', marginBottom: 8 }} />
          <div style={{ height: 14, width: '80%', borderRadius: 4, background: 'var(--color-fill-quaternary)' }} />
        </div>

        {/* 按钮区域占位 */}
        <div style={{ display: 'flex', gap: 8 }}>
          <div style={{ flex: 1, height: 36, borderRadius: 8, background: 'var(--color-fill-quaternary)' }} />
          <div style={{ flex: 1, height: 36, borderRadius: 8, background: 'var(--color-fill-quaternary)' }} />
        </div>
      </div>

      {/* 执行历史标题占位 */}
      <div style={{ display: 'flex', alignItems: 'center', marginBottom: 12 }}>
        <div style={{ height: 16, width: 100, borderRadius: 4, background: 'var(--color-fill-quaternary)' }} />
        <div style={{ marginLeft: 'auto', display: 'flex', gap: 8 }}>
          <div style={{ height: 28, width: 80, borderRadius: 6, background: 'var(--color-fill-quaternary)' }} />
          <div style={{ width: 28, height: 28, borderRadius: 6, background: 'var(--color-fill-quaternary)' }} />
        </div>
      </div>

      {/* 执行历史列表占位 */}
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
              <div style={{ height: 14, width: '100%', borderRadius: 4, background: 'var(--color-fill-quaternary)', marginBottom: 8 }} />
              <div style={{ height: 14, width: '60%', borderRadius: 4, background: 'var(--color-fill-quaternary)' }} />
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
            <div style={{ height: 14, width: '40%', borderRadius: 4, background: 'var(--color-fill-quaternary)', marginBottom: 12 }} />
            <div style={{ height: 14, width: '100%', borderRadius: 4, background: 'var(--color-fill-quaternary)', marginBottom: 8 }} />
            <div style={{ height: 14, width: '100%', borderRadius: 4, background: 'var(--color-fill-quaternary)', marginBottom: 8 }} />
            <div style={{ height: 14, width: '70%', borderRadius: 4, background: 'var(--color-fill-quaternary)' }} />
          </div>
          <div
            style={{
              background: 'var(--color-bg-tertiary)',
              borderRadius: 'var(--radius-md)',
              padding: 16,
            }}
          >
            <div style={{ height: 12, width: '30%', borderRadius: 4, background: 'var(--color-fill-quaternary)', marginBottom: 12 }} />
            <div style={{ height: 12, width: '100%', borderRadius: 4, background: 'var(--color-fill-quaternary)', marginBottom: 8 }} />
            <div style={{ height: 12, width: '100%', borderRadius: 4, background: 'var(--color-fill-quaternary)', marginBottom: 8 }} />
            <div style={{ height: 12, width: '80%', borderRadius: 4, background: 'var(--color-fill-quaternary)', marginBottom: 8 }} />
            <div style={{ height: 12, width: '100%', borderRadius: 4, background: 'var(--color-fill-quaternary)', marginBottom: 8 }} />
            <div style={{ height: 12, width: '60%', borderRadius: 4, background: 'var(--color-fill-quaternary)' }} />
          </div>
        </div>
      </div>
    </div>
  );
}
