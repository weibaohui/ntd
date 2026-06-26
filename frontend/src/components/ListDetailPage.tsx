import { useState, useEffect } from 'react';
import type { ReactNode } from 'react';
import { Button } from 'antd';
import { MenuFoldOutlined, MenuUnfoldOutlined } from '@ant-design/icons';
import { PageCard } from './common/PageCard';
import { EmptyDetailPlaceholder } from './EmptyDetailPlaceholder';
import { useIsMobile } from '@/hooks/useIsMobile';
import { SIDEBAR_WIDTH } from '@/constants';

interface ListDetailPageProps {
  icon: ReactNode;
  title: string;
  extra?: ReactNode;
  listPanel: ReactNode;
  detailPanel: ReactNode | null;
  storageKey?: string;
}

export function ListDetailPage({
  icon,
  title,
  extra,
  listPanel,
  detailPanel,
  storageKey = 'list_detail_sidebar_collapsed',
}: ListDetailPageProps) {
  const isMobile = useIsMobile();
  const [sidebarCollapsed, setSidebarCollapsed] = useState(() => {
    try {
      return localStorage.getItem(storageKey) === 'true';
    } catch {
      return false;
    }
  });

  useEffect(() => {
    try {
      localStorage.setItem(storageKey, String(sidebarCollapsed));
    } catch {}
  }, [sidebarCollapsed, storageKey]);

  const toggleSidebar = () => {
    setSidebarCollapsed(v => !v);
  };

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

  const headerExtra = (
    <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
      {extra}
      <Button
        type="text"
        size="small"
        icon={sidebarCollapsed ? <MenuUnfoldOutlined /> : <MenuFoldOutlined />}
        onClick={toggleSidebar}
        style={{ padding: '0 4px' }}
      />
    </div>
  );

  return (
    <PageCard
      icon={icon}
      title={title}
      extra={headerExtra}
      className="list-detail-page-card"
      style={{ height: '100%', flex: 1, minWidth: 0 }}
      contentStyle={{ padding: 0, display: 'flex', flexDirection: 'row', height: 'calc(100% - 43px)' }}
    >
      <div
        className="list-detail-page-sidebar"
        style={{
          width: sidebarCollapsed ? 0 : SIDEBAR_WIDTH.desktop,
          flexShrink: 0,
          height: '100%',
          overflow: 'hidden',
          borderRight: sidebarCollapsed ? 'none' : '1px solid var(--color-border-light)',
          transition: 'width 0.2s ease, border-right-width 0.2s ease',
        }}
      >
        <div style={{ width: SIDEBAR_WIDTH.desktop, height: '100%' }}>
          {listPanel}
        </div>
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
