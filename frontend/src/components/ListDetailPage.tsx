import type { ReactNode } from 'react';
import { PageCard } from './common/PageCard';
import { EmptyDetailPlaceholder } from './EmptyDetailPlaceholder';
import { useIsMobile } from '@/hooks/useIsMobile';
import { SIDEBAR_WIDTH } from '@/constants';

interface ListDetailPageProps {
  icon: ReactNode;
  title: string;
  listPanel: ReactNode;
  detailPanel: ReactNode | null;
}

export function ListDetailPage({ icon, title, listPanel, detailPanel }: ListDetailPageProps) {
  const isMobile = useIsMobile();

  if (isMobile) {
    return (
      <>
        <div style={{ width: SIDEBAR_WIDTH.mobile, flexShrink: 0, height: '100%' }}>
          {listPanel}
        </div>
        <div style={{ flex: 1, minWidth: 0, height: '100%', overflow: 'hidden' }}>
          {detailPanel ?? <EmptyDetailPlaceholder />}
        </div>
      </>
    );
  }

  return (
    <PageCard
      icon={icon}
      title={title}
      className="list-detail-page-card"
      style={{ height: '100%', flex: 1, minWidth: 0 }}
      contentStyle={{ padding: 0, display: 'flex', flexDirection: 'row', height: 'calc(100% - 43px)' }}
    >
      <div
        style={{
          width: SIDEBAR_WIDTH.desktop,
          flexShrink: 0,
          height: '100%',
          borderRight: '1px solid var(--color-border-light)',
        }}
      >
        {listPanel}
      </div>

      <div
        className="list-detail-page-right"
        style={{
          flex: 1,
          minWidth: 0,
          height: '100%',
          overflow: 'hidden',
        }}
      >
        {detailPanel ?? <EmptyDetailPlaceholder />}
      </div>
    </PageCard>
  );
}
