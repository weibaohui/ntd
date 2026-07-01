// Todo 列表骨架屏行组件。

export function SkeletonRow() {
  return <div className="skeleton-row" />;
}

export function SkeletonList() {
  return (
    <div style={{ padding: '12px 16px' }}>
      {Array.from({ length: 6 }).map((_, i) => (
        <SkeletonRow key={i} />
      ))}
    </div>
  );
}
