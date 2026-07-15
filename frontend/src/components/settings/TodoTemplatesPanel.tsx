// 旧版"事项模板"面板（占位）
// 已迁移到 LeftRail 的「模板管理」菜单 → 「事项模板」Tab
// 此文件保留为向后兼容引用，未来可移除

import { Alert } from 'antd';
import { PageCard } from '@/components/common/PageCard';

export function TodoTemplatesPanel() {
  return (
    <PageCard title="事项模板">
      <Alert
        type="info"
        showIcon
        message="事项模板已迁移到「模板管理」菜单中"
        description="请通过左侧导航的「模板管理」→「事项模板」Tab 访问完整功能。"
      />
    </PageCard>
  );
}
